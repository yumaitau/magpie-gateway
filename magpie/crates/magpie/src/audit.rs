//! Structured audit events. The store refuses updates and deletes.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct AuditEvent {
	pub id: String,
	pub at: DateTime<Utc>,
	pub kind: String,
	pub actor: String,
	pub agent_id: String,
	pub tool: String,
	pub model: String,
	pub decision: String,
	pub reason: String,
	/// JSON detail. Callers must already have redacted matched text.
	pub detail: serde_json::Value,
	#[serde(default)]
	pub telemetry: Telemetry,
}

/// Correlation and usage named after the OpenTelemetry GenAI semantic
/// conventions, so exported events join the traces of the agent that caused
/// them. Empty fields are unknown and are not exported.
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", default)]
pub struct Telemetry {
	/// `gen_ai.operation.name`
	#[serde(skip_serializing_if = "String::is_empty")]
	pub operation: String,
	/// `gen_ai.provider.name`
	#[serde(skip_serializing_if = "String::is_empty")]
	pub provider: String,
	/// `gen_ai.conversation.id`
	#[serde(skip_serializing_if = "String::is_empty")]
	pub conversation_id: String,
	/// W3C trace context of the request that caused the event.
	#[serde(skip_serializing_if = "String::is_empty")]
	pub trace_id: String,
	#[serde(skip_serializing_if = "String::is_empty")]
	pub span_id: String,
	/// `gen_ai.usage.input_tokens`, including cached input.
	#[serde(skip_serializing_if = "Option::is_none")]
	pub input_tokens: Option<u64>,
	/// `gen_ai.usage.output_tokens`
	#[serde(skip_serializing_if = "Option::is_none")]
	pub output_tokens: Option<u64>,
}

impl AuditEvent {
	#[allow(clippy::too_many_arguments)]
	pub fn new(
		kind: &str,
		actor: &str,
		agent_id: &str,
		tool: &str,
		model: &str,
		decision: &str,
		reason: &str,
		detail: serde_json::Value,
	) -> Self {
		Self {
			id: crate::new_id(),
			at: Utc::now(),
			kind: kind.to_string(),
			actor: actor.to_string(),
			agent_id: agent_id.to_string(),
			tool: tool.to_string(),
			model: model.to_string(),
			decision: decision.to_string(),
			reason: reason.to_string(),
			detail,
			telemetry: Telemetry::default(),
		}
	}
}

#[derive(Debug, Clone, Default)]
pub struct AuditQuery {
	pub kind: Option<String>,
	pub actor: Option<String>,
	pub agent_id: Option<String>,
	pub decision: Option<String>,
	pub from: Option<DateTime<Utc>>,
	pub to: Option<DateTime<Utc>>,
	pub limit: usize,
}

pub fn matches(event: &AuditEvent, query: &AuditQuery) -> bool {
	if query.kind.as_ref().is_some_and(|kind| kind != &event.kind) {
		return false;
	}
	if query
		.actor
		.as_ref()
		.is_some_and(|actor| !event.actor.contains(actor))
	{
		return false;
	}
	if query
		.agent_id
		.as_ref()
		.is_some_and(|agent| agent != &event.agent_id)
	{
		return false;
	}
	if query
		.decision
		.as_ref()
		.is_some_and(|decision| decision != &event.decision)
	{
		return false;
	}
	if query.from.is_some_and(|from| event.at < from) {
		return false;
	}
	if query.to.is_some_and(|to| event.at > to) {
		return false;
	}
	true
}

pub fn to_csv(events: &[AuditEvent]) -> String {
	let mut writer = csv::Writer::from_writer(vec![]);
	writer
		.write_record([
			"id", "at", "kind", "actor", "agentId", "tool", "model", "decision", "reason", "detail",
		])
		.expect("header");
	for event in events {
		writer
			.write_record([
				event.id.as_str(),
				&event.at.to_rfc3339(),
				event.kind.as_str(),
				event.actor.as_str(),
				event.agent_id.as_str(),
				event.tool.as_str(),
				event.model.as_str(),
				event.decision.as_str(),
				event.reason.as_str(),
				&event.detail.to_string(),
			])
			.expect("row");
	}
	String::from_utf8(writer.into_inner().expect("csv")).expect("utf8")
}
