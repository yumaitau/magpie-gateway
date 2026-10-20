use super::proto::*;
use super::*;
use std::io::Write;

const TOKEN: &str = "otlp-test-token";

fn state(token: &str) -> AppState {
	let mut config = crate::pools::tests::test_config();
	config.otlp_token = token.into();
	crate::state(config).unwrap()
}

fn headers(token: &str, content_type: &str) -> HeaderMap {
	let mut headers = HeaderMap::new();
	headers.insert(
		header::AUTHORIZATION,
		format!("Bearer {token}").parse().unwrap(),
	);
	headers.insert(header::CONTENT_TYPE, content_type.parse().unwrap());
	headers
}

fn kv(key: &str, value: Any) -> KeyValue {
	KeyValue {
		key: key.into(),
		value: Some(AnyValue { value: Some(value) }),
	}
}

fn s(key: &str, value: &str) -> KeyValue {
	kv(key, Any::String(value.into()))
}

fn telemetry(state: &AppState) -> Vec<AuditEvent> {
	state
		.inner
		.store
		.lock()
		.unwrap()
		.search_audit(&AuditQuery {
			kind: Some("telemetry".into()),
			limit: 100,
			..AuditQuery::default()
		})
		.unwrap()
}

fn named<'a>(events: &'a [AuditEvent], name: &str) -> &'a AuditEvent {
	events
		.iter()
		.find(|event| event.detail["name"] == name)
		.unwrap_or_else(|| panic!("no {name} event"))
}

fn span(name: &str, kind: i32, attributes: Vec<KeyValue>) -> Span {
	Span {
		trace_id: (1..=16).collect(),
		span_id: (1..=8).collect(),
		name: name.into(),
		kind,
		start_time_unix_nano: 1_790_000_000_000_000_000,
		end_time_unix_nano: 1_790_000_001_500_000_000,
		attributes,
		status: None,
	}
}

fn claude_platform() -> Vec<u8> {
	let usage = |input, output| {
		vec![
			s("gen_ai.provider.name", "anthropic"),
			s("anthropic.workspace.id", "wrkspc_01"),
			s("gen_ai.request.model", "claude-opus-5-5"),
			kv("gen_ai.usage.input_tokens", Any::Int(input)),
			kv("gen_ai.usage.output_tokens", Any::Int(output)),
		]
	};
	let mut create = usage(120, 30);
	create.push(s("gen_ai.operation.name", "chat"));
	create.push(s(
		"gen_ai.input.messages",
		r#"[{"role":"user","parts":[{"type":"text","content":"TFN 123 456 782"}]}]"#,
	));
	let mut failed = span(
		"anthropic.tool_use web_search",
		1,
		vec![
			s("gen_ai.tool.name", "web_search"),
			s("anthropic.workspace.id", "wrkspc_01"),
			s("gen_ai.tool.call.arguments", r#"{"query":"secret plans"}"#),
		],
	);
	failed.status = Some(Status {
		code: 2,
		message: "tool failed".into(),
	});
	Traces {
		resource_spans: vec![ResourceSpans {
			resource: Some(Resource {
				attributes: vec![s("service.name", "claude-platform")],
			}),
			scope_spans: vec![ScopeSpans {
				scope: Some(Scope {
					name: "com.anthropic.claude-platform".into(),
				}),
				spans: vec![
					span("anthropic.messages.create", 2, create),
					span("anthropic.model_request", 1, usage(120, 30)),
					failed,
				],
			}],
		}],
	}
	.encode_to_vec()
}

#[test]
fn claude_platform_traces_become_audit_events_without_content() {
	let state = state(TOKEN);
	let response = receive(
		&state,
		&headers(TOKEN, "application/x-protobuf"),
		&claude_platform(),
		Signal::Trace,
	);
	assert_eq!(response.status(), StatusCode::OK);
	assert_eq!(
		response.headers()[header::CONTENT_TYPE],
		"application/x-protobuf"
	);
	let events = telemetry(&state);
	assert_eq!(events.len(), 3);

	let create = named(&events, "anthropic.messages.create");
	assert_eq!(create.decision, "log");
	assert_eq!(create.model, "claude-opus-5-5");
	assert_eq!(create.actor, "workspace:wrkspc_01");
	assert_eq!(create.agent_id, "claude-platform");
	assert_eq!(create.reason, "Claude Platform anthropic.messages.create");
	assert_eq!(create.detail["spanKind"], "server");
	assert_eq!(create.detail["durationMs"], 1500);
	assert_eq!(create.telemetry.provider, "anthropic");
	assert_eq!(create.telemetry.operation, "chat");
	assert_eq!(
		create.telemetry.trace_id,
		"0102030405060708090a0b0c0d0e0f10"
	);
	assert_eq!(create.telemetry.span_id, "0102030405060708");
	assert_eq!(create.telemetry.input_tokens, Some(120));
	assert_eq!(create.telemetry.output_tokens, Some(30));
	assert_eq!(create.at.timestamp(), 1_790_000_000);
	assert_eq!(
		create.detail["contentOmitted"],
		json!(["gen_ai.input.messages"])
	);

	// The nested model request repeats the request's usage, so it is not counted twice.
	let model = named(&events, "anthropic.model_request");
	assert_eq!(model.telemetry.input_tokens, None);
	assert_eq!(model.detail["attributes"]["gen_ai.usage.input_tokens"], 120);

	let tool = named(&events, "anthropic.tool_use web_search");
	assert_eq!(tool.tool, "web_search");
	assert_eq!(tool.telemetry.operation, "execute_tool");
	assert_eq!(tool.detail["status"], "error");
	assert_eq!(tool.detail["statusMessage"], "tool failed");
	assert!(tool.reason.ends_with(" failed"));

	let stored = serde_json::to_string(&events).unwrap();
	assert!(!stored.contains("123 456 782"));
	assert!(!stored.contains("secret plans"));
}

#[test]
fn the_receiver_needs_its_token_and_a_known_encoding() {
	let body = claude_platform();
	let off = state("");
	assert_eq!(
		receive(
			&off,
			&headers("", "application/x-protobuf"),
			&body,
			Signal::Trace
		)
		.status(),
		StatusCode::NOT_FOUND
	);
	let on = state(TOKEN);
	for token in ["", "wrong"] {
		assert_eq!(
			receive(
				&on,
				&headers(token, "application/x-protobuf"),
				&body,
				Signal::Trace
			)
			.status(),
			StatusCode::UNAUTHORIZED
		);
	}
	assert_eq!(
		receive(&on, &headers(TOKEN, "text/plain"), &body, Signal::Trace).status(),
		StatusCode::UNSUPPORTED_MEDIA_TYPE
	);
	assert_eq!(
		receive(
			&on,
			&headers(TOKEN, "application/x-protobuf"),
			b"\xff\xff",
			Signal::Trace
		)
		.status(),
		StatusCode::BAD_REQUEST
	);
	assert!(telemetry(&on).is_empty());
}

fn json_logs(service: &str, records: Value) -> Value {
	json!({
		"resourceLogs": [{
			"resource": { "attributes": [
				{ "key": "service.name", "value": { "stringValue": service } }
			]},
			"scopeLogs": [{ "scope": { "name": service }, "logRecords": records }]
		}]
	})
}

fn gzip(body: &[u8]) -> Vec<u8> {
	let mut encoder = flate2::write::GzEncoder::new(Vec::new(), flate2::Compression::default());
	encoder.write_all(body).unwrap();
	encoder.finish().unwrap()
}

#[test]
fn claude_code_logs_count_cache_tokens_as_input() {
	let state = state(TOKEN);
	let body = json_logs(
		"claude-code",
		json!([{
			"timeUnixNano": "1790000000000000000",
			"body": { "stringValue": "claude_code.api_request" },
			"attributes": [
				{ "key": "event.name", "value": { "stringValue": "api_request" } },
				{ "key": "session.id", "value": { "stringValue": "session-7" } },
				{ "key": "user.email", "value": { "stringValue": "alex@example.gov.au" } },
				{ "key": "model", "value": { "stringValue": "claude-sonnet-5-5" } },
				{ "key": "input_tokens", "value": { "stringValue": "10" } },
				{ "key": "cache_read_tokens", "value": { "intValue": "200" } },
				{ "key": "cache_creation_tokens", "value": { "intValue": 5 } },
				{ "key": "output_tokens", "value": { "intValue": "40" } }
			]
		}]),
	);
	let response = receive(
		&state,
		&headers(TOKEN, "application/json"),
		body.to_string().as_bytes(),
		Signal::Log,
	);
	assert_eq!(response.status(), StatusCode::OK);
	let events = telemetry(&state);
	let event = named(&events, "claude_code.api_request");
	assert_eq!(event.actor, "alex@example.gov.au");
	assert_eq!(event.agent_id, "claude-code");
	assert_eq!(event.model, "claude-sonnet-5-5");
	assert_eq!(event.telemetry.provider, "anthropic");
	assert_eq!(event.telemetry.conversation_id, "session-7");
	assert_eq!(event.telemetry.input_tokens, Some(215));
	assert_eq!(event.telemetry.output_tokens, Some(40));
	assert_eq!(event.detail["signal"], "log");
	assert_eq!(event.detail["contentOmitted"], json!(["body"]));
}

#[test]
fn codex_logs_arrive_gzipped_and_drop_tool_payloads() {
	let state = state(TOKEN);
	let common = |name: &str| {
		vec![
			json!({ "key": "event.name", "value": { "stringValue": name } }),
			json!({ "key": "conversation.id", "value": { "stringValue": "thread-1" } }),
			json!({ "key": "user.email", "value": { "stringValue": "sam@example.gov.au" } }),
			json!({ "key": "model", "value": { "stringValue": "gpt-5.5-codex" } }),
		]
	};
	let mut usage = common("codex.sse_event");
	usage.extend([
		json!({ "key": "event.kind", "value": { "stringValue": "response.completed" } }),
		json!({ "key": "input_token_count", "value": { "intValue": "900" } }),
		json!({ "key": "output_token_count", "value": { "intValue": "80" } }),
	]);
	let mut tool = common("codex.tool_result");
	tool.extend([
		json!({ "key": "tool_name", "value": { "stringValue": "shell" } }),
		json!({ "key": "arguments", "value": { "stringValue": "cat ~/.ssh/id_ed25519" } }),
		json!({ "key": "output", "value": { "stringValue": "-----BEGIN OPENSSH PRIVATE KEY-----" } }),
		json!({ "key": "success", "value": { "stringValue": "false" } }),
	]);
	let body = json_logs(
		"codex_cli_rs",
		json!([
			{ "timeUnixNano": "1790000000000000000", "attributes": usage },
			{ "timeUnixNano": "1790000000000000000", "attributes": tool }
		]),
	);
	let mut headers = headers(TOKEN, "application/json");
	headers.insert(header::CONTENT_ENCODING, "gzip".parse().unwrap());
	let response = receive(
		&state,
		&headers,
		&gzip(body.to_string().as_bytes()),
		Signal::Log,
	);
	assert_eq!(response.status(), StatusCode::OK);
	let events = telemetry(&state);
	let sse = named(&events, "codex.sse_event");
	assert_eq!(sse.agent_id, "codex");
	assert_eq!(sse.actor, "sam@example.gov.au");
	assert_eq!(sse.telemetry.provider, "openai");
	assert_eq!(sse.telemetry.operation, "chat");
	assert_eq!(sse.telemetry.conversation_id, "thread-1");
	assert_eq!(sse.telemetry.input_tokens, Some(900));
	assert_eq!(sse.telemetry.output_tokens, Some(80));
	let result = named(&events, "codex.tool_result");
	assert_eq!(result.tool, "shell");
	assert_eq!(result.telemetry.operation, "execute_tool");
	assert_eq!(result.reason, "Codex codex.tool_result failed");
	assert_eq!(
		result.detail["contentOmitted"],
		json!(["arguments", "output"])
	);
	let stored = serde_json::to_string(&events).unwrap();
	assert!(!stored.contains("id_ed25519"));
	assert!(!stored.contains("PRIVATE KEY"));
}

#[test]
fn grok_build_logs_name_events_on_the_record() {
	let state = state(TOKEN);
	let log = |event_name: &str, attributes: Vec<KeyValue>| LogRecord {
		time_unix_nano: 1_790_000_000_000_000_000,
		event_name: event_name.into(),
		attributes,
		..LogRecord::default()
	};
	let body = Logs {
		resource_logs: vec![ResourceLogs {
			resource: Some(Resource {
				attributes: vec![
					s("service.name", "grok-cli"),
					s("grok_code.schema.version", "v1"),
				],
			}),
			scope_logs: vec![ScopeLogs {
				scope: None,
				log_records: vec![
					log(
						"grok_code.api_request",
						vec![
							s("session.id", "grok-session"),
							s("user.email", "kim@example.gov.au"),
							s("model", "grok-5-code"),
							kv("input_tokens", Any::Int(300)),
							kv("output_tokens", Any::Int(25)),
						],
					),
					log(
						"grok_code.tool_result",
						vec![
							s("tool_name", "run_terminal_cmd"),
							s("full_command", "curl https://example.invalid/upload"),
							kv("success", Any::Bool(true)),
						],
					),
				],
			}],
		}],
	}
	.encode_to_vec();
	let response = receive(
		&state,
		&headers(TOKEN, "application/x-protobuf"),
		&body,
		Signal::Log,
	);
	assert_eq!(response.status(), StatusCode::OK);
	let events = telemetry(&state);
	let request = named(&events, "grok_code.api_request");
	assert_eq!(request.agent_id, "grok");
	assert_eq!(request.actor, "kim@example.gov.au");
	assert_eq!(request.model, "grok-5-code");
	assert_eq!(request.telemetry.provider, "x_ai");
	assert_eq!(request.telemetry.conversation_id, "grok-session");
	assert_eq!(request.telemetry.input_tokens, Some(300));
	assert_eq!(request.telemetry.output_tokens, Some(25));
	assert_eq!(request.detail["resource"]["grok_code.schema.version"], "v1");
	let tool = named(&events, "grok_code.tool_result");
	assert_eq!(tool.tool, "run_terminal_cmd");
	assert_eq!(tool.actor, "grok");
	assert_eq!(tool.detail["status"], "ok");
	assert_eq!(tool.detail["contentOmitted"], json!(["full_command"]));
}

#[test]
fn metrics_are_accepted_and_discarded() {
	let state = state(TOKEN);
	let response = receive(
		&state,
		&headers(TOKEN, "application/x-protobuf"),
		b"\x0a\x00",
		Signal::Metric,
	);
	assert_eq!(response.status(), StatusCode::OK);
	assert!(telemetry(&state).is_empty());
}

#[test]
fn magpie_audit_export_is_not_ingested_again() {
	let state = state(TOKEN);
	let event = AuditEvent::new("dlp", "a", "b", "", "", "redact", "ABN", json!({}));
	let body = magpie::sinks::otlp_logs(&[event], false);
	let response = receive(
		&state,
		&headers(TOKEN, "application/json"),
		body.to_string().as_bytes(),
		Signal::Log,
	);
	assert_eq!(response.status(), StatusCode::OK);
	assert!(telemetry(&state).is_empty());
}

#[tokio::test]
async fn the_router_serves_the_otlp_paths() {
	let app = crate::router(state(TOKEN));
	let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
	let address = listener.local_addr().unwrap();
	tokio::spawn(async move { axum::serve(listener, app).await.unwrap() });
	let response = reqwest::Client::new()
		.post(format!("http://{address}/otlp/v1/traces"))
		.bearer_auth(TOKEN)
		.header(header::CONTENT_TYPE, "application/x-protobuf")
		.body(claude_platform())
		.send()
		.await
		.unwrap();
	assert_eq!(response.status(), reqwest::StatusCode::OK);
	let response = reqwest::Client::new()
		.post(format!("http://{address}/otlp/v1/logs"))
		.header(header::CONTENT_TYPE, "application/json")
		.body("{}")
		.send()
		.await
		.unwrap();
	assert_eq!(response.status(), reqwest::StatusCode::UNAUTHORIZED);
}

#[test]
fn a_free_text_body_never_names_the_event() {
	let none = BTreeMap::new();
	let name = |body: &str| log_name("", &none, Some(&json!(body)));
	assert_eq!(name("claude_code.api_request"), "claude_code.api_request");
	assert_eq!(name("hunter2"), "log");
	assert_eq!(name("TFN:123456782"), "log");
	assert_eq!(name("a..b"), "log");
	assert_eq!(name("rm -rf /tmp/x"), "log");
}
