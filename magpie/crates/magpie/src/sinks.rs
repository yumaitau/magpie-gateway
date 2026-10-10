//! Audit sink payloads. Nothing is sent unless the portal is given a URL.
//!
//! The OCSF object is a subset shaped for Security Lake style storage. It is
//! not a certified OCSF mapping. Sentinel and Splunk bodies follow the public
//! request shapes those products document for their ingestion APIs.

use serde_json::{Value, json};

use crate::audit::AuditEvent;

#[derive(Debug, Clone, Default)]
pub struct SinkConfig {
	pub otlp_endpoint: String,
	pub sentinel_endpoint: String,
	pub splunk_endpoint: String,
	pub s3_bucket: String,
	pub syslog_address: String,
	pub capture_payloads: bool,
}

impl SinkConfig {
	pub fn any_enabled(&self) -> bool {
		!self.otlp_endpoint.is_empty()
			|| !self.sentinel_endpoint.is_empty()
			|| !self.splunk_endpoint.is_empty()
			|| !self.s3_bucket.is_empty()
			|| !self.syslog_address.is_empty()
	}
}

fn detail_for(event: &AuditEvent, capture_payloads: bool) -> Value {
	if capture_payloads {
		event.detail.clone()
	} else {
		json!({
			"redacted": true,
			"keys": event.detail.as_object().map(|map| map.keys().cloned().collect::<Vec<_>>()).unwrap_or_default()
		})
	}
}

pub fn otlp_log(event: &AuditEvent, capture_payloads: bool) -> Value {
	json!({
		"resourceLogs": [{
			"resource": { "attributes": [{ "key": "service.name", "value": { "stringValue": "magpie-gateway" } }] },
			"scopeLogs": [{
				"scope": { "name": "magpie.audit" },
				"logRecords": [{
					"timeUnixNano": event.at.timestamp_nanos_opt().unwrap_or(0).to_string(),
					"severityText": "INFO",
					"body": { "stringValue": event.reason },
					"attributes": [
						{ "key": "magpie.kind", "value": { "stringValue": event.kind } },
						{ "key": "enduser.id", "value": { "stringValue": event.actor } },
						{ "key": "magpie.agent", "value": { "stringValue": event.agent_id } },
						{ "key": "magpie.tool", "value": { "stringValue": event.tool } },
						{ "key": "magpie.model", "value": { "stringValue": event.model } },
						{ "key": "magpie.decision", "value": { "stringValue": event.decision } },
						{ "key": "magpie.detail", "value": { "stringValue": detail_for(event, capture_payloads).to_string() } }
					]
				}]
			}]
		}]
	})
}

pub fn sentinel_record(event: &AuditEvent, capture_payloads: bool) -> Value {
	json!({
		"TimeGenerated": event.at.to_rfc3339(),
		"EventId": event.id,
		"Kind": event.kind,
		"Actor": event.actor,
		"AgentId": event.agent_id,
		"Tool": event.tool,
		"Model": event.model,
		"Decision": event.decision,
		"Reason": event.reason,
		"Detail": detail_for(event, capture_payloads)
	})
}

pub fn splunk_hec(event: &AuditEvent, capture_payloads: bool) -> Value {
	json!({
		"time": event.at.timestamp(),
		"sourcetype": "magpie:audit",
		"event": sentinel_record(event, capture_payloads)
	})
}

pub fn ocsf_api_activity(event: &AuditEvent, capture_payloads: bool) -> Value {
	json!({
		"class_uid": 6003,
		"class_name": "API Activity",
		"category_uid": 6,
		"category_name": "Application Activity",
		"activity_id": 99,
		"activity_name": "Other",
		"severity_id": 1,
		"time": event.at.timestamp_millis(),
		"message": event.reason,
		"metadata": {
			"product": { "name": "Magpie Gateway", "vendor_name": "Yuma IT" },
			"version": "1.1.0",
			"profiles": ["subset, not a certified OCSF mapping"]
		},
		"actor": { "user": { "name": event.actor } },
		"unmapped": {
			"agent_id": event.agent_id,
			"tool": event.tool,
			"model": event.model,
			"decision": event.decision,
			"detail": detail_for(event, capture_payloads)
		}
	})
}

pub fn syslog_line(event: &AuditEvent) -> String {
	format!(
		"<134>1 {} magpie-gateway magpie - {} [magpie@0 kind=\"{}\" decision=\"{}\" actor=\"{}\" agent=\"{}\" tool=\"{}\" model=\"{}\"] {}",
		event.at.to_rfc3339(),
		event.id,
		escape(&event.kind),
		escape(&event.decision),
		escape(&event.actor),
		escape(&event.agent_id),
		escape(&event.tool),
		escape(&event.model),
		escape(&event.reason)
	)
}

fn escape(value: &str) -> String {
	value.replace('\\', "\\\\").replace('"', "'")
}

#[cfg(test)]
mod tests {
	use super::*;
	use chrono::Utc;

	fn event() -> AuditEvent {
		AuditEvent {
			id: "evt-1".into(),
			at: Utc::now(),
			kind: "dlp".into(),
			actor: "alex@example.gov.au".into(),
			agent_id: "example-agent".into(),
			tool: "example_search".into(),
			model: "example-au".into(),
			decision: "redact".into(),
			reason: "ABN redacted".into(),
			detail: json!({"payload": "ABN 12 000 000 041", "hits": 1}),
		}
	}

	#[test]
	fn payloads_are_omitted_unless_capture_is_on() {
		let body = sentinel_record(&event(), false).to_string();
		assert!(!body.contains("12 000 000 041"));
		assert!(body.contains("redacted"));
		let full = sentinel_record(&event(), true).to_string();
		assert!(full.contains("12 000 000 041"));
	}

	#[test]
	fn each_sink_has_its_shape_and_defaults_are_off() {
		assert!(!SinkConfig::default().any_enabled());
		assert!(otlp_log(&event(), false)["resourceLogs"][0]["scopeLogs"][0]["logRecords"][0]["body"]["stringValue"].is_string());
		assert_eq!(splunk_hec(&event(), false)["sourcetype"], "magpie:audit");
		assert_eq!(ocsf_api_activity(&event(), false)["class_uid"], 6003);
		let line = syslog_line(&event());
		assert!(line.starts_with("<134>1 "));
		assert!(line.contains("decision=\"redact\""));
	}
}
