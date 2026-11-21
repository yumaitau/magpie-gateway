//! Audit sink payloads. Nothing is sent unless the portal is given a URL.
//! The portal delivers OTLP when the standard `OTEL_EXPORTER_OTLP_*` variables name an endpoint.
//!
//! The OCSF object is a subset shaped for Security Lake style storage. It is
//! not a certified OCSF mapping. Sentinel and Splunk bodies follow the public
//! request shapes those products document for their ingestion APIs.

use chrono::Utc;
use rusqlite::params;
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};

use crate::Result;
use crate::audit::AuditEvent;
use crate::store::Store;

pub const SCHEMA: &str = r#"
CREATE TABLE IF NOT EXISTS audit_sinks (
  id TEXT PRIMARY KEY,
  body TEXT NOT NULL
);
CREATE TABLE IF NOT EXISTS audit_sink_cursors (
  sink_id TEXT PRIMARY KEY,
  position INTEGER NOT NULL DEFAULT 0,
  last_ok_at TEXT NOT NULL DEFAULT '',
  last_error TEXT NOT NULL DEFAULT '',
  sent_count INTEGER NOT NULL DEFAULT 0
);
"#;

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct AuditSink {
	pub id: String,
	pub name: String,
	/// "sentinel" | "otlp" | "splunk" | "syslog"
	pub kind: String,
	pub enabled: bool,
	pub capture_payloads: bool,
	/// Generic endpoint (OTLP /v1/logs, Splunk HEC, Syslog HTTP) or Sentinel DCE URL.
	pub endpoint: String,
	/// Microsoft Sentinel Logs Ingestion API fields.
	pub dcr_immutable_id: String,
	pub stream_name: String,
	/// "managed-identity" | "client-credentials"
	pub auth_mode: String,
	pub tenant_id: String,
	pub client_id: String,
	/// Secret: Entra client secret, Splunk HEC token, or Bearer token. Never returned in GET responses.
	pub secret: String,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SinkStatus {
	pub sink_id: String,
	pub position: i64,
	pub last_ok_at: String,
	pub last_error: String,
	pub sent_count: i64,
}

impl Store {
	pub fn audit_sinks(&self) -> Result<Vec<AuditSink>> {
		let mut stmt = self.conn.prepare("SELECT body FROM audit_sinks ORDER BY id")?;
		let rows = stmt.query_map([], |row| row.get::<_, String>(0))?;
		rows.map(|r| Ok(serde_json::from_str(&r?)?)).collect()
	}

	pub fn save_audit_sink(&self, sink: &AuditSink, actor: &str) -> Result<()> {
		let tx = self.conn.unchecked_transaction()?;
		let existing = self
			.audit_sinks()?
			.into_iter()
			.find(|item| item.id == sink.id);
		let mut saved = sink.clone();
		if saved.secret.is_empty()
			&& let Some(prev) = &existing
		{
			saved.secret = prev.secret.clone();
		}
		self.conn.execute(
			"INSERT INTO audit_sinks (id, body) VALUES (?1, ?2)
			 ON CONFLICT(id) DO UPDATE SET body = excluded.body",
			params![saved.id, serde_json::to_string(&saved)?],
		)?;
		// Start a new or newly-enabled sink at the latest event so it does not replay old history.
		let start_pos: i64 = self.conn.query_row(
			"SELECT COALESCE(MAX(rowid), 0) FROM audit_events",
			[],
			|r| r.get(0),
		)?;
		self.conn.execute(
			"INSERT OR IGNORE INTO audit_sink_cursors (sink_id, position, last_ok_at, last_error, sent_count)
			 VALUES (?1, ?2, '', '', 0)",
			params![saved.id, start_pos],
		)?;
		self.append_audit(&AuditEvent::new(
			"sinks",
			actor,
			"",
			"",
			"",
			"allow",
			"SIEM audit sink saved",
			json!({"id": saved.id, "name": saved.name, "kind": saved.kind, "enabled": saved.enabled}),
		))?;
		tx.commit()?;
		Ok(())
	}

	pub fn delete_audit_sink(&self, id: &str, actor: &str) -> Result<()> {
		let tx = self.conn.unchecked_transaction()?;
		self.conn
			.execute("DELETE FROM audit_sinks WHERE id = ?1", [id])?;
		self.conn
			.execute("DELETE FROM audit_sink_cursors WHERE sink_id = ?1", [id])?;
		self.append_audit(&AuditEvent::new(
			"sinks",
			actor,
			"",
			"",
			"",
			"allow",
			"SIEM audit sink removed",
			json!({"id": id}),
		))?;
		tx.commit()?;
		Ok(())
	}

	pub fn audit_sink_statuses(&self) -> Result<Vec<SinkStatus>> {
		let mut stmt = self.conn.prepare(
			"SELECT sink_id, position, last_ok_at, last_error, sent_count FROM audit_sink_cursors",
		)?;
		let rows = stmt.query_map([], |r| {
			Ok(SinkStatus {
				sink_id: r.get(0)?,
				position: r.get(1)?,
				last_ok_at: r.get(2)?,
				last_error: r.get(3)?,
				sent_count: r.get(4)?,
			})
		})?;
		rows.map(|r| Ok(r?)).collect()
	}

	pub fn audit_sink_batch(&self, sink_id: &str, limit: usize) -> Result<Vec<(i64, AuditEvent)>> {
		let position: i64 = self
			.conn
			.query_row(
				"SELECT COALESCE((SELECT position FROM audit_sink_cursors WHERE sink_id = ?1), 0)",
				[sink_id],
				|r| r.get(0),
			)?;
		let mut stmt = self.conn.prepare(
			"SELECT id, at, kind, actor, agent_id, tool, model, decision, reason, detail, telemetry, rowid
			 FROM audit_events WHERE rowid > ?1 ORDER BY rowid LIMIT ?2",
		)?;
		let mut rows = stmt.query(params![position, limit as i64])?;
		let mut out = Vec::new();
		while let Some(row) = rows.next()? {
			out.push((row.get(11)?, crate::store::read_audit(row)?));
		}
		Ok(out)
	}

	pub fn audit_sink_record_result(
		&self,
		sink_id: &str,
		advance_to: Option<i64>,
		sent_delta: usize,
		error: Option<&str>,
	) -> Result<()> {
		let now = Utc::now().to_rfc3339();
		if let Some(pos) = advance_to {
			if let Some(err) = error {
				self.conn.execute(
					"UPDATE audit_sink_cursors SET position = ?2, last_error = ?3 WHERE sink_id = ?1",
					params![sink_id, pos, err],
				)?;
			} else {
				self.conn.execute(
					"UPDATE audit_sink_cursors SET position = ?2, last_ok_at = ?3, last_error = '', sent_count = sent_count + ?4 WHERE sink_id = ?1",
					params![sink_id, pos, now, sent_delta as i64],
				)?;
			}
		} else if let Some(err) = error {
			self.conn.execute(
				"UPDATE audit_sink_cursors SET last_error = ?2 WHERE sink_id = ?1",
				params![sink_id, err],
			)?;
		}
		Ok(())
	}
}

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

/// One OTLP/JSON logs request. Attribute names follow the OpenTelemetry GenAI
/// semantic conventions; policy fields that have no convention use `magpie.*`.
pub fn otlp_logs(events: &[AuditEvent], capture_payloads: bool) -> Value {
	json!({
		"resourceLogs": [{
			"resource": { "attributes": [{ "key": "service.name", "value": { "stringValue": "magpie-gateway" } }] },
			"scopeLogs": [{
				"scope": { "name": "magpie.audit" },
				"logRecords": events.iter().map(|event| otlp_record(event, capture_payloads)).collect::<Vec<_>>()
			}]
		}]
	})
}

pub fn otlp_log(event: &AuditEvent, capture_payloads: bool) -> Value {
	otlp_logs(std::slice::from_ref(event), capture_payloads)
}

fn otlp_record(event: &AuditEvent, capture_payloads: bool) -> Value {
	let t = &event.telemetry;
	let mut attributes = Vec::new();
	for (key, value) in [
		("gen_ai.operation.name", t.operation.as_str()),
		("gen_ai.provider.name", &t.provider),
		("gen_ai.request.model", &event.model),
		("gen_ai.agent.id", &event.agent_id),
		("gen_ai.tool.name", &event.tool),
		("gen_ai.conversation.id", &t.conversation_id),
		("user.id", &event.actor),
		("magpie.event.id", &event.id),
		("magpie.kind", &event.kind),
		("magpie.decision", &event.decision),
	] {
		if !value.is_empty() {
			attributes.push(json!({ "key": key, "value": { "stringValue": value } }));
		}
	}
	for (key, value) in [
		("gen_ai.usage.input_tokens", t.input_tokens),
		("gen_ai.usage.output_tokens", t.output_tokens),
	] {
		if let Some(value) = value {
			attributes.push(json!({ "key": key, "value": { "intValue": value.to_string() } }));
		}
	}
	attributes.push(json!({ "key": "magpie.detail", "value": { "stringValue": detail_for(event, capture_payloads).to_string() } }));
	let mut record = json!({
		"timeUnixNano": event.at.timestamp_nanos_opt().unwrap_or(0).to_string(),
		"severityNumber": 9,
		"severityText": "INFO",
		"body": { "stringValue": event.reason },
		"attributes": attributes
	});
	if !t.trace_id.is_empty() {
		record["traceId"] = json!(t.trace_id);
		record["spanId"] = json!(t.span_id);
	}
	record
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
			telemetry: Default::default(),
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

	#[test]
	fn otlp_uses_genai_conventions_and_joins_the_trace() {
		let mut event = event();
		event.telemetry = crate::audit::Telemetry {
			operation: "chat".into(),
			provider: "anthropic".into(),
			conversation_id: "session-1".into(),
			trace_id: "4bf92f3577b34da6a3ce929d0e0e4736".into(),
			span_id: "00f067aa0ba902b7".into(),
			input_tokens: Some(12),
			output_tokens: Some(3),
		};
		let body = otlp_logs(&[event.clone(), event], false);
		let records = &body["resourceLogs"][0]["scopeLogs"][0]["logRecords"];
		assert_eq!(records.as_array().unwrap().len(), 2);
		let record = &records[0];
		assert_eq!(record["traceId"], "4bf92f3577b34da6a3ce929d0e0e4736");
		assert_eq!(record["spanId"], "00f067aa0ba902b7");
		let attribute = |key: &str| {
			record["attributes"]
				.as_array()
				.unwrap()
				.iter()
				.find(|a| a["key"] == key)
				.map(|a| a["value"].clone())
		};
		assert_eq!(
			attribute("gen_ai.provider.name").unwrap()["stringValue"],
			"anthropic"
		);
		assert_eq!(
			attribute("gen_ai.request.model").unwrap()["stringValue"],
			"example-au"
		);
		assert_eq!(
			attribute("gen_ai.agent.id").unwrap()["stringValue"],
			"example-agent"
		);
		assert_eq!(
			attribute("gen_ai.tool.name").unwrap()["stringValue"],
			"example_search"
		);
		assert_eq!(
			attribute("gen_ai.conversation.id").unwrap()["stringValue"],
			"session-1"
		);
		assert_eq!(
			attribute("gen_ai.usage.input_tokens").unwrap()["intValue"],
			"12"
		);
		assert_eq!(
			attribute("user.id").unwrap()["stringValue"],
			"alex@example.gov.au"
		);
		assert!(attribute("magpie.agent").is_none());
		assert!(!body.to_string().contains("12 000 000 041"));
		let untraced = otlp_log(&super::tests::event(), false);
		let untraced = &untraced["resourceLogs"][0]["scopeLogs"][0]["logRecords"][0];
		assert!(untraced.get("traceId").is_none());
		assert!(!untraced.to_string().contains("gen_ai.conversation.id"));
	}
}
