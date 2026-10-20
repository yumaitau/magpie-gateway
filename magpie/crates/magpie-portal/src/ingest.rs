//! OTLP/HTTP receiver for agent telemetry. Claude Platform traces and the log
//! events of Claude Code, Codex and Grok become `telemetry` audit events, so
//! agent activity outside the proxy sits in the same audit trail.
//!
//! Prompts, responses, tool arguments and results are never stored: content
//! attributes and long values are dropped and only their keys are kept.
use std::collections::BTreeMap;
use std::io::Read;

use axum::body::Bytes;
use axum::extract::DefaultBodyLimit;
use prost::Message;

use super::*;

/// Largest request body, before and after gzip.
const MAX_BODY: usize = 16 << 20;
/// Longer attribute values are treated as content and dropped.
const MAX_VALUE: usize = 1024;

pub(super) fn routes() -> Router<AppState> {
	Router::new()
		.route("/otlp/v1/traces", post(traces))
		.route("/otlp/v1/logs", post(logs))
		.route("/otlp/v1/metrics", post(metrics))
		.layer(DefaultBodyLimit::max(MAX_BODY))
}

/// Attributes that carry prompts, responses or tool payloads.
const CONTENT: &[&str] = &[
	"gen_ai.input.messages",
	"gen_ai.output.messages",
	"gen_ai.system_instructions",
	"gen_ai.tool.call.arguments",
	"gen_ai.tool.call.result",
	"gen_ai.request.stop_sequences",
	"anthropic.tool_result.content",
	"anthropic.session.outcome_evaluation.explanation",
	"prompt",
	"arguments",
	"output",
	"prompt_text",
	"response",
	"tool_parameters",
	"tool_input",
	"tool_output",
	"full_command",
	"error_message",
	"ai.prompt",
	"ai.prompt.messages",
	"ai.response.text",
	"ai.response.toolCalls",
	"ai.toolCall.args",
	"ai.toolCall.result",
];

#[derive(Clone, Copy, PartialEq, Eq)]
enum Signal {
	Trace,
	Log,
	/// Accepted and discarded, so agents that export metrics beside logs do
	/// not report failures. Logs carry the same usage.
	Metric,
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum Source {
	ClaudePlatform,
	ClaudeCode,
	Codex,
	Grok,
	Other,
}

impl Source {
	fn id(self) -> &'static str {
		match self {
			Source::ClaudePlatform => "claude-platform",
			Source::ClaudeCode => "claude-code",
			Source::Codex => "codex",
			Source::Grok => "grok",
			Source::Other => "otlp",
		}
	}

	fn label(self) -> &'static str {
		match self {
			Source::ClaudePlatform => "Claude Platform",
			Source::ClaudeCode => "Claude Code",
			Source::Codex => "Codex",
			Source::Grok => "Grok",
			Source::Other => "OpenTelemetry",
		}
	}

	fn provider(self) -> &'static str {
		match self {
			Source::ClaudePlatform | Source::ClaudeCode => "anthropic",
			Source::Codex => "openai",
			Source::Grok => "x_ai",
			Source::Other => "",
		}
	}
}

/// One span or log record, from either encoding.
#[derive(Default)]
struct Record {
	resource: BTreeMap<String, Value>,
	scope: String,
	name: String,
	/// OTLP `SpanKind`; zero for logs.
	kind: i32,
	start_ns: u64,
	end_ns: u64,
	trace_id: String,
	span_id: String,
	/// OTLP `StatusCode`: 2 is error.
	status: i32,
	status_message: String,
	attributes: BTreeMap<String, Value>,
	body: Option<Value>,
}

async fn traces(State(state): State<AppState>, headers: HeaderMap, body: Bytes) -> Response {
	receive(&state, &headers, &body, Signal::Trace)
}

async fn logs(State(state): State<AppState>, headers: HeaderMap, body: Bytes) -> Response {
	receive(&state, &headers, &body, Signal::Log)
}

async fn metrics(State(state): State<AppState>, headers: HeaderMap, body: Bytes) -> Response {
	receive(&state, &headers, &body, Signal::Metric)
}

fn receive(state: &AppState, headers: &HeaderMap, body: &[u8], signal: Signal) -> Response {
	let token = &state.inner.config.otlp_token;
	if token.is_empty() {
		return (StatusCode::NOT_FOUND, "OTLP ingest is off").into_response();
	}
	let presented = header_str(headers, "authorization")
		.and_then(|value| value.strip_prefix("Bearer "))
		.unwrap_or("");
	if !identity::constant_time_eq(presented.trim().as_bytes(), token.as_bytes()) {
		return (StatusCode::UNAUTHORIZED, "OTLP bearer token required").into_response();
	}
	let json = match header_str(headers, "content-type")
		.unwrap_or("")
		.split(';')
		.next()
		.unwrap_or("")
		.trim()
	{
		"application/x-protobuf" | "application/protobuf" => false,
		"application/json" => true,
		_ => {
			return (
				StatusCode::UNSUPPORTED_MEDIA_TYPE,
				"Send application/x-protobuf or application/json",
			)
				.into_response();
		},
	};
	let body = match header_str(headers, "content-encoding").map(str::trim) {
		None | Some("" | "identity") => body.to_vec(),
		Some("gzip") => match gunzip(body) {
			Ok(body) => body,
			Err(error) => return (StatusCode::BAD_REQUEST, error).into_response(),
		},
		Some(_) => {
			return (
				StatusCode::UNSUPPORTED_MEDIA_TYPE,
				"Only gzip encoding is supported",
			)
				.into_response();
		},
	};
	let records = match (json, signal) {
		(_, Signal::Metric) => Ok(Vec::new()),
		(false, Signal::Trace) => proto::Traces::decode(body.as_slice())
			.map(proto::Traces::records)
			.map_err(|_| "Invalid OTLP traces"),
		(false, Signal::Log) => proto::Logs::decode(body.as_slice())
			.map(proto::Logs::records)
			.map_err(|_| "Invalid OTLP logs"),
		(true, _) => serde_json::from_slice::<Value>(&body)
			.map(|value| from_json(&value, signal))
			.map_err(|_| "Invalid OTLP JSON"),
	};
	let records = match records {
		Ok(records) => records,
		Err(error) => return (StatusCode::BAD_REQUEST, error).into_response(),
	};
	let events: Vec<AuditEvent> = records
		.iter()
		.filter_map(|record| event(record, signal))
		.collect();
	{
		let store = state.inner.store.lock().expect("db");
		for event in &events {
			if store.append_audit(event).is_err() {
				return (StatusCode::SERVICE_UNAVAILABLE, "Audit store unavailable").into_response();
			}
		}
	}
	// An empty Export*ServiceResponse is a full success in either encoding.
	if json {
		([(header::CONTENT_TYPE, "application/json")], "{}").into_response()
	} else {
		[(header::CONTENT_TYPE, "application/x-protobuf")].into_response()
	}
}

fn gunzip(body: &[u8]) -> Result<Vec<u8>, &'static str> {
	let mut out = Vec::new();
	flate2::read::GzDecoder::new(body)
		.take(MAX_BODY as u64 + 1)
		.read_to_end(&mut out)
		.map_err(|_| "Invalid gzip body")?;
	if out.len() > MAX_BODY {
		return Err("Decompressed body is too large");
	}
	Ok(out)
}

fn source(record: &Record) -> Source {
	let service = text(&record.resource, "service.name").to_ascii_lowercase();
	let name = record.name.as_str();
	let provider = text(&record.attributes, "gen_ai.provider.name");
	if service == "claude-platform" || record.scope.starts_with("com.anthropic.") {
		Source::ClaudePlatform
	} else if service.starts_with("claude-code") || name.starts_with("claude_code.") {
		Source::ClaudeCode
	} else if service.starts_with("codex") || name.starts_with("codex.") {
		Source::Codex
	} else if service.starts_with("grok")
		|| name.starts_with("grok_code.")
		|| matches!(provider, "xai" | "x_ai")
	{
		Source::Grok
	} else {
		Source::Other
	}
}

fn text<'a>(map: &'a BTreeMap<String, Value>, key: &str) -> &'a str {
	map.get(key).and_then(Value::as_str).unwrap_or("")
}

fn first(record: &Record, keys: &[&str]) -> String {
	keys
		.iter()
		.find_map(|key| {
			let value = match record.attributes.get(*key).or(record.resource.get(*key))? {
				Value::String(value) => value.trim().to_string(),
				Value::Number(value) => value.to_string(),
				_ => return None,
			};
			(!value.is_empty() && value.len() <= 256).then_some(value)
		})
		.unwrap_or_default()
}

fn number(record: &Record, keys: &[&str]) -> Option<u64> {
	keys
		.iter()
		.find_map(|key| match record.attributes.get(*key)? {
			Value::Number(value) => value.as_u64(),
			Value::String(value) => value.trim().parse().ok(),
			_ => None,
		})
}

fn failed(record: &Record) -> bool {
	record.status == 2
		|| record.attributes.contains_key("error.type")
		|| matches!(record.attributes.get("success"), Some(Value::Bool(false)))
		|| text(&record.attributes, "success") == "false"
}

/// Input tokens including cache reads and writes, as Magpie records them.
fn input_tokens(record: &Record, source: Source) -> Option<u64> {
	let base = number(
		record,
		&[
			"gen_ai.usage.input_tokens",
			"input_token_count",
			"ai.usage.inputTokens",
			"ai.usage.promptTokens",
			"input_tokens",
		],
	)?;
	// Claude Code reports cache reads and writes beside its input tokens.
	if source == Source::ClaudeCode && !record.attributes.contains_key("gen_ai.usage.input_tokens") {
		return Some(
			base
				+ number(record, &["cache_read_tokens"]).unwrap_or(0)
				+ number(record, &["cache_creation_tokens"]).unwrap_or(0),
		);
	}
	Some(base)
}

/// Claude Platform repeats usage on nested spans. Keep it only on the spans
/// Anthropic says to sum, so totals count each token once.
fn counts_usage(record: &Record, source: Source) -> bool {
	if source != Source::ClaudePlatform {
		return true;
	}
	match record.name.as_str() {
		"anthropic.messages.create" => record.kind == 2,
		"anthropic.compaction" | "anthropic.session.outcome_evaluation" => true,
		"anthropic.model_request" => record.attributes.contains_key("anthropic.session.id"),
		_ => false,
	}
}

fn event(record: &Record, signal: Signal) -> Option<AuditEvent> {
	// Magpie's own audit export, sent back here by a misconfigured collector.
	if text(&record.resource, "service.name") == "magpie-gateway" || record.scope == "magpie.audit" {
		return None;
	}
	let source = source(record);
	// Claude Code's `event.name` drops the documented `claude_code.` prefix.
	let name = if source == Source::ClaudeCode && !record.name.contains('.') {
		format!("claude_code.{}", record.name)
	} else {
		record.name.clone()
	};
	let model = first(
		record,
		&[
			"gen_ai.response.model",
			"gen_ai.request.model",
			"model",
			"ai.response.model",
			"ai.model.id",
		],
	);
	let tool = first(
		record,
		&[
			"gen_ai.tool.name",
			"tool_name",
			"tool.name",
			"ai.toolCall.name",
		],
	);
	let workspace = first(record, &["anthropic.workspace.id"]);
	let actor = match first(
		record,
		&[
			"user.email",
			"enduser.id",
			"user.id",
			"user.account_uuid",
			"user.account_id",
		],
	) {
		actor if !actor.is_empty() => actor,
		_ if !workspace.is_empty() => format!("workspace:{workspace}"),
		_ => source.id().into(),
	};
	let agent = match first(record, &["gen_ai.agent.id", "gen_ai.agent.name"]) {
		agent if !agent.is_empty() => agent,
		_ => source.id().into(),
	};
	let provider = match first(record, &["gen_ai.provider.name"]) {
		provider if !provider.is_empty() => provider,
		_ => source.provider().into(),
	};
	let operation = match first(record, &["gen_ai.operation.name"]) {
		operation if !operation.is_empty() => operation,
		_ if !tool.is_empty() => "execute_tool".into(),
		_ if !model.is_empty() => "chat".into(),
		_ => String::new(),
	};
	let usage = counts_usage(record, source);
	let at_ns = if record.start_ns > 0 {
		record.start_ns
	} else {
		record.end_ns
	};
	let at = match i64::try_from(at_ns) {
		Ok(ns) if ns > 0 => chrono::DateTime::from_timestamp_nanos(ns),
		_ => Utc::now(),
	};
	let failed = failed(record);
	let mut omitted = Vec::new();
	let attributes = keep(&record.attributes, &mut omitted);
	let resource = keep(&record.resource, &mut omitted);
	if record.body.is_some() {
		omitted.push("body".to_string());
	}
	let mut detail = json!({
		"source": source.id(),
		"signal": if signal == Signal::Trace { "trace" } else { "log" },
		"name": name,
		"scope": record.scope,
		"status": if failed { "error" } else { "ok" },
		"attributes": attributes,
		"resource": resource,
	});
	if signal == Signal::Trace {
		detail["spanKind"] = json!(
			[
				"unspecified",
				"internal",
				"server",
				"client",
				"producer",
				"consumer"
			]
			.get(record.kind as usize)
			.unwrap_or(&"unspecified")
		);
		if record.end_ns >= record.start_ns && record.start_ns > 0 {
			detail["durationMs"] = json!((record.end_ns - record.start_ns) / 1_000_000);
		}
	}
	if !record.status_message.is_empty() && record.status_message.len() <= MAX_VALUE {
		detail["statusMessage"] = json!(record.status_message);
	}
	if !omitted.is_empty() {
		detail["contentOmitted"] = json!(omitted);
	}
	let mut event = AuditEvent::new(
		"telemetry",
		&actor,
		&agent,
		&tool,
		&model,
		"log",
		&format!(
			"{} {}{}",
			source.label(),
			name,
			if failed { " failed" } else { "" }
		),
		detail,
	);
	event.at = at;
	event.telemetry = magpie::audit::Telemetry {
		operation,
		provider,
		conversation_id: first(
			record,
			&[
				"gen_ai.conversation.id",
				"conversation.id",
				"session.id",
				"anthropic.session.id",
			],
		),
		trace_id: record.trace_id.clone(),
		span_id: record.span_id.clone(),
		input_tokens: usage.then(|| input_tokens(record, source)).flatten(),
		output_tokens: usage
			.then(|| {
				number(
					record,
					&[
						"gen_ai.usage.output_tokens",
						"output_token_count",
						"ai.usage.outputTokens",
						"ai.usage.completionTokens",
						"output_tokens",
					],
				)
			})
			.flatten(),
	};
	Some(event)
}

/// Metadata attributes only. Content keys and long values go to `omitted`.
fn keep(map: &BTreeMap<String, Value>, omitted: &mut Vec<String>) -> Value {
	let mut kept = serde_json::Map::new();
	for (key, value) in map {
		let long = match value {
			Value::String(value) => value.len() > MAX_VALUE,
			Value::Array(_) | Value::Object(_) => value.to_string().len() > MAX_VALUE,
			_ => false,
		};
		if long || CONTENT.contains(&key.as_str()) {
			omitted.push(key.clone());
		} else {
			kept.insert(key.clone(), value.clone());
		}
	}
	Value::Object(kept)
}

fn hex(bytes: &[u8]) -> String {
	if bytes.iter().all(|b| *b == 0) {
		return String::new();
	}
	bytes.iter().map(|b| format!("{b:02x}")).collect()
}

/// OTLP/JSON: lowerCamelCase fields, hex IDs, 64-bit integers as strings.
fn from_json(value: &Value, signal: Signal) -> Vec<Record> {
	let (resources, scopes, items) = match signal {
		Signal::Trace => ("resourceSpans", "scopeSpans", "spans"),
		Signal::Log => ("resourceLogs", "scopeLogs", "logRecords"),
		Signal::Metric => return Vec::new(),
	};
	let list = |value: &Value, key: &str| value[key].as_array().cloned().unwrap_or_default();
	let u64_of = |value: &Value| match value {
		Value::String(value) => value.parse().unwrap_or(0),
		value => value.as_u64().unwrap_or(0),
	};
	let id = |value: &Value| {
		let id = value.as_str().unwrap_or("").to_ascii_lowercase();
		if id.bytes().all(|b| b.is_ascii_hexdigit()) && id.bytes().any(|b| b != b'0') {
			id
		} else {
			String::new()
		}
	};
	let mut records = Vec::new();
	for resource in list(value, resources) {
		let resource_attributes = json_attributes(&resource["resource"]["attributes"]);
		for scope in list(&resource, scopes) {
			let scope_name = scope["scope"]["name"].as_str().unwrap_or("").to_string();
			for item in list(&scope, items) {
				let attributes = json_attributes(&item["attributes"]);
				let body = item.get("body").map(json_any).filter(|v| !v.is_null());
				let name = match signal {
					Signal::Trace | Signal::Metric => item["name"].as_str().unwrap_or("").to_string(),
					Signal::Log => log_name(
						item["eventName"].as_str().unwrap_or(""),
						&attributes,
						body.as_ref(),
					),
				};
				let (start, end) = match signal {
					Signal::Trace | Signal::Metric => (
						u64_of(&item["startTimeUnixNano"]),
						u64_of(&item["endTimeUnixNano"]),
					),
					Signal::Log => {
						let time = u64_of(&item["timeUnixNano"]);
						let observed = u64_of(&item["observedTimeUnixNano"]);
						(if time > 0 { time } else { observed }, 0)
					},
				};
				records.push(Record {
					resource: resource_attributes.clone(),
					scope: scope_name.clone(),
					name,
					kind: json_enum(
						&item["kind"],
						"SPAN_KIND_",
						&[
							"UNSPECIFIED",
							"INTERNAL",
							"SERVER",
							"CLIENT",
							"PRODUCER",
							"CONSUMER",
						],
					),
					start_ns: start,
					end_ns: end,
					trace_id: id(&item["traceId"]),
					span_id: id(&item["spanId"]),
					status: json_enum(
						&item["status"]["code"],
						"STATUS_CODE_",
						&["UNSET", "OK", "ERROR"],
					),
					status_message: item["status"]["message"].as_str().unwrap_or("").to_string(),
					attributes,
					body,
				});
			}
		}
	}
	records
}

/// Enums are integers in OTLP/JSON, but some exporters send the names.
fn json_enum(value: &Value, prefix: &str, names: &[&str]) -> i32 {
	match value {
		Value::Number(n) => n.as_i64().unwrap_or(0) as i32,
		Value::String(name) => names
			.iter()
			.position(|n| name.strip_prefix(prefix).unwrap_or(name) == *n)
			.unwrap_or(0) as i32,
		_ => 0,
	}
}

fn json_attributes(value: &Value) -> BTreeMap<String, Value> {
	value
		.as_array()
		.into_iter()
		.flatten()
		.filter_map(|kv| Some((kv["key"].as_str()?.to_string(), json_any(&kv["value"]))))
		.collect()
}

fn json_any(value: &Value) -> Value {
	if let Some(v) = value.get("stringValue") {
		v.clone()
	} else if let Some(v) = value.get("boolValue") {
		v.clone()
	} else if let Some(v) = value.get("intValue") {
		match v {
			Value::String(s) => s.parse::<i64>().map(Value::from).unwrap_or(Value::Null),
			v => v.clone(),
		}
	} else if let Some(v) = value.get("doubleValue") {
		v.clone()
	} else if let Some(v) = value.get("arrayValue") {
		Value::Array(
			v["values"]
				.as_array()
				.into_iter()
				.flatten()
				.map(json_any)
				.collect(),
		)
	} else if let Some(v) = value.get("kvlistValue") {
		Value::Object(json_attributes(&v["values"]).into_iter().collect())
	} else if let Some(v) = value.get("bytesValue") {
		v.clone()
	} else {
		Value::Null
	}
}

/// Event name from the `event.name` attribute (Codex, Claude Code), the
/// record's `eventName` (Grok), or a short body.
fn log_name(
	event_name: &str,
	attributes: &BTreeMap<String, Value>,
	body: Option<&Value>,
) -> String {
	let body = body.and_then(Value::as_str).unwrap_or("");
	[text(attributes, "event.name"), event_name, body]
		.into_iter()
		.map(str::trim)
		.find(|name| !name.is_empty() && name.len() <= 128 && !name.contains(char::is_whitespace))
		.unwrap_or("log")
		.to_string()
}

/// The OTLP protobuf messages Magpie reads. Unknown fields are skipped.
mod proto {
	use super::*;

	#[derive(Clone, PartialEq, Message)]
	pub struct Traces {
		#[prost(message, repeated, tag = "1")]
		pub resource_spans: Vec<ResourceSpans>,
	}

	#[derive(Clone, PartialEq, Message)]
	pub struct ResourceSpans {
		#[prost(message, optional, tag = "1")]
		pub resource: Option<Resource>,
		#[prost(message, repeated, tag = "2")]
		pub scope_spans: Vec<ScopeSpans>,
	}

	#[derive(Clone, PartialEq, Message)]
	pub struct ScopeSpans {
		#[prost(message, optional, tag = "1")]
		pub scope: Option<Scope>,
		#[prost(message, repeated, tag = "2")]
		pub spans: Vec<Span>,
	}

	#[derive(Clone, PartialEq, Message)]
	pub struct Span {
		#[prost(bytes = "vec", tag = "1")]
		pub trace_id: Vec<u8>,
		#[prost(bytes = "vec", tag = "2")]
		pub span_id: Vec<u8>,
		#[prost(string, tag = "5")]
		pub name: String,
		#[prost(int32, tag = "6")]
		pub kind: i32,
		#[prost(fixed64, tag = "7")]
		pub start_time_unix_nano: u64,
		#[prost(fixed64, tag = "8")]
		pub end_time_unix_nano: u64,
		#[prost(message, repeated, tag = "9")]
		pub attributes: Vec<KeyValue>,
		#[prost(message, optional, tag = "15")]
		pub status: Option<Status>,
	}

	#[derive(Clone, PartialEq, Message)]
	pub struct Status {
		#[prost(string, tag = "2")]
		pub message: String,
		#[prost(int32, tag = "3")]
		pub code: i32,
	}

	#[derive(Clone, PartialEq, Message)]
	pub struct Logs {
		#[prost(message, repeated, tag = "1")]
		pub resource_logs: Vec<ResourceLogs>,
	}

	#[derive(Clone, PartialEq, Message)]
	pub struct ResourceLogs {
		#[prost(message, optional, tag = "1")]
		pub resource: Option<Resource>,
		#[prost(message, repeated, tag = "2")]
		pub scope_logs: Vec<ScopeLogs>,
	}

	#[derive(Clone, PartialEq, Message)]
	pub struct ScopeLogs {
		#[prost(message, optional, tag = "1")]
		pub scope: Option<Scope>,
		#[prost(message, repeated, tag = "2")]
		pub log_records: Vec<LogRecord>,
	}

	#[derive(Clone, PartialEq, Message)]
	pub struct LogRecord {
		#[prost(fixed64, tag = "1")]
		pub time_unix_nano: u64,
		#[prost(message, optional, tag = "5")]
		pub body: Option<AnyValue>,
		#[prost(message, repeated, tag = "6")]
		pub attributes: Vec<KeyValue>,
		#[prost(bytes = "vec", tag = "9")]
		pub trace_id: Vec<u8>,
		#[prost(bytes = "vec", tag = "10")]
		pub span_id: Vec<u8>,
		#[prost(fixed64, tag = "11")]
		pub observed_time_unix_nano: u64,
		#[prost(string, tag = "12")]
		pub event_name: String,
	}

	#[derive(Clone, PartialEq, Message)]
	pub struct Resource {
		#[prost(message, repeated, tag = "1")]
		pub attributes: Vec<KeyValue>,
	}

	#[derive(Clone, PartialEq, Message)]
	pub struct Scope {
		#[prost(string, tag = "1")]
		pub name: String,
	}

	#[derive(Clone, PartialEq, Message)]
	pub struct KeyValue {
		#[prost(string, tag = "1")]
		pub key: String,
		#[prost(message, optional, tag = "2")]
		pub value: Option<AnyValue>,
	}

	#[derive(Clone, PartialEq, Message)]
	pub struct AnyValue {
		#[prost(oneof = "Any", tags = "1, 2, 3, 4, 5, 6, 7")]
		pub value: Option<Any>,
	}

	#[derive(Clone, PartialEq, prost::Oneof)]
	pub enum Any {
		#[prost(string, tag = "1")]
		String(String),
		#[prost(bool, tag = "2")]
		Bool(bool),
		#[prost(int64, tag = "3")]
		Int(i64),
		#[prost(double, tag = "4")]
		Double(f64),
		#[prost(message, tag = "5")]
		Array(List),
		#[prost(message, tag = "6")]
		Map(Map),
		#[prost(bytes, tag = "7")]
		Bytes(Vec<u8>),
	}

	#[derive(Clone, PartialEq, Message)]
	pub struct List {
		#[prost(message, repeated, tag = "1")]
		pub values: Vec<AnyValue>,
	}

	#[derive(Clone, PartialEq, Message)]
	pub struct Map {
		#[prost(message, repeated, tag = "1")]
		pub values: Vec<KeyValue>,
	}

	fn value(any: &AnyValue) -> Value {
		match &any.value {
			Some(Any::String(v)) => json!(v),
			Some(Any::Bool(v)) => json!(v),
			Some(Any::Int(v)) => json!(v),
			Some(Any::Double(v)) => json!(v),
			Some(Any::Array(v)) => Value::Array(v.values.iter().map(value).collect()),
			Some(Any::Map(v)) => Value::Object(attributes(&v.values).into_iter().collect()),
			Some(Any::Bytes(v)) => json!(hex(v)),
			None => Value::Null,
		}
	}

	fn attributes(list: &[KeyValue]) -> BTreeMap<String, Value> {
		list
			.iter()
			.map(|kv| {
				(
					kv.key.clone(),
					kv.value.as_ref().map(value).unwrap_or(Value::Null),
				)
			})
			.collect()
	}

	fn resource(resource: &Option<Resource>) -> BTreeMap<String, Value> {
		resource
			.as_ref()
			.map(|r| attributes(&r.attributes))
			.unwrap_or_default()
	}

	fn scope(scope: &Option<Scope>) -> String {
		scope.as_ref().map(|s| s.name.clone()).unwrap_or_default()
	}

	impl Traces {
		pub fn records(self) -> Vec<Record> {
			let mut records = Vec::new();
			for resource_spans in &self.resource_spans {
				let resource = resource(&resource_spans.resource);
				for scope_spans in &resource_spans.scope_spans {
					let scope = scope(&scope_spans.scope);
					for span in &scope_spans.spans {
						let status = span.status.clone().unwrap_or_default();
						records.push(Record {
							resource: resource.clone(),
							scope: scope.clone(),
							name: span.name.clone(),
							kind: span.kind,
							start_ns: span.start_time_unix_nano,
							end_ns: span.end_time_unix_nano,
							trace_id: hex(&span.trace_id),
							span_id: hex(&span.span_id),
							status: status.code,
							status_message: status.message,
							attributes: attributes(&span.attributes),
							body: None,
						});
					}
				}
			}
			records
		}
	}

	impl Logs {
		pub fn records(self) -> Vec<Record> {
			let mut records = Vec::new();
			for resource_logs in &self.resource_logs {
				let resource = resource(&resource_logs.resource);
				for scope_logs in &resource_logs.scope_logs {
					let scope = scope(&scope_logs.scope);
					for log in &scope_logs.log_records {
						let attributes = attributes(&log.attributes);
						let body = log.body.as_ref().map(value).filter(|v| !v.is_null());
						records.push(Record {
							resource: resource.clone(),
							scope: scope.clone(),
							name: log_name(&log.event_name, &attributes, body.as_ref()),
							start_ns: if log.time_unix_nano > 0 {
								log.time_unix_nano
							} else {
								log.observed_time_unix_nano
							},
							trace_id: hex(&log.trace_id),
							span_id: hex(&log.span_id),
							attributes,
							body,
							..Record::default()
						});
					}
				}
			}
			records
		}
	}
}

#[cfg(test)]
mod tests;
