use super::*;
use std::collections::HashMap;

fn env(vars: &[(&str, &str)]) -> impl Fn(&str) -> Option<String> {
	let vars: HashMap<String, String> = vars
		.iter()
		.map(|(k, v)| (k.to_string(), v.to_string()))
		.collect();
	move |name| vars.get(name).cloned()
}

#[test]
fn configuration_follows_the_standard_exporter_variables() {
	assert_eq!(configure(env(&[])).unwrap(), None);
	let base = configure(env(&[
		("OTEL_EXPORTER_OTLP_ENDPOINT", "https://collector.example/"),
		(
			"OTEL_EXPORTER_OTLP_HEADERS",
			"authorization=Bearer%20abc, x-tenant=au",
		),
	]))
	.unwrap()
	.unwrap();
	assert_eq!(base.endpoint, "https://collector.example/v1/logs");
	assert_eq!(
		base.headers,
		vec![
			("authorization".into(), "Bearer abc".into()),
			("x-tenant".into(), "au".into())
		]
	);
	let logs = configure(env(&[
		("OTEL_EXPORTER_OTLP_ENDPOINT", "https://collector.example"),
		(
			"OTEL_EXPORTER_OTLP_LOGS_ENDPOINT",
			"https://logs.example/ingest",
		),
		("OTEL_EXPORTER_OTLP_LOGS_PROTOCOL", "http/json"),
	]))
	.unwrap()
	.unwrap();
	assert_eq!(logs.endpoint, "https://logs.example/ingest");
	assert!(
		configure(env(&[
			("OTEL_EXPORTER_OTLP_ENDPOINT", "https://collector.example"),
			("OTEL_EXPORTER_OTLP_PROTOCOL", "grpc"),
		]))
		.is_err()
	);
	assert!(
		configure(env(&[
			("OTEL_EXPORTER_OTLP_ENDPOINT", "https://collector.example"),
			("OTEL_EXPORTER_OTLP_HEADERS", "broken"),
		]))
		.is_err()
	);
	assert_eq!(
		configure(env(&[
			("OTEL_EXPORTER_OTLP_ENDPOINT", "https://collector.example"),
			("OTEL_SDK_DISABLED", "true"),
		]))
		.unwrap(),
		None
	);
}

#[test]
fn correlation_comes_from_trace_context_and_agent_session_headers() {
	let mut headers = HeaderMap::new();
	headers.insert(
		"traceparent",
		"00-4bf92f3577b34da6a3ce929d0e0e4736-00f067aa0ba902b7-01"
			.parse()
			.unwrap(),
	);
	headers.insert("x-session-id", "generic".parse().unwrap());
	headers.insert("x-claude-code-session-id", "claude-1".parse().unwrap());
	headers.insert("x-magpie-destination", "claude".parse().unwrap());
	let telemetry = telemetry_from(&headers);
	assert_eq!(telemetry.trace_id, "4bf92f3577b34da6a3ce929d0e0e4736");
	assert_eq!(telemetry.span_id, "00f067aa0ba902b7");
	assert_eq!(telemetry.conversation_id, "claude-1");
	assert_eq!(telemetry.provider, "anthropic");
	assert_eq!(telemetry.operation, "chat");
	let mut copied = HeaderMap::new();
	copy_correlation(&headers, &mut copied);
	assert_eq!(copied.len(), 2);
	headers.insert(
		"traceparent",
		"00-00000000000000000000000000000000-00f067aa0ba902b7-01"
			.parse()
			.unwrap(),
	);
	headers.remove("x-claude-code-session-id");
	headers.insert("x-magpie-tool", "search".parse().unwrap());
	let telemetry = telemetry_from(&headers);
	assert!(telemetry.trace_id.is_empty());
	assert_eq!(telemetry.conversation_id, "generic");
	assert_eq!(telemetry.operation, "execute_tool");
}

#[tokio::test]
async fn exports_new_events_once_and_retries_unavailable_collectors() {
	let received = Arc::new(Mutex::new(Vec::<(Option<String>, Value)>::new()));
	let status = Arc::new(Mutex::new(StatusCode::OK));
	let router = Router::new().route(
		"/v1/logs",
		post({
			let received = received.clone();
			let status = status.clone();
			move |headers: HeaderMap, Json(body): Json<Value>| async move {
				let auth = header_str(&headers, "authorization").map(str::to_string);
				received.lock().unwrap().push((auth, body));
				*status.lock().unwrap()
			}
		}),
	);
	let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
	let url = format!("http://{}", listener.local_addr().unwrap());
	tokio::spawn(async move { axum::serve(listener, router).await.unwrap() });
	let exporter = configure(env(&[
		("OTEL_EXPORTER_OTLP_ENDPOINT", &url),
		(
			"OTEL_EXPORTER_OTLP_HEADERS",
			"authorization=Bearer%20collector",
		),
	]))
	.unwrap()
	.unwrap();
	let state = crate::pools::tests::test_state();
	// History before export starts is not replayed.
	assert_eq!(export_once(&state, &exporter).await.unwrap(), 0);
	let mut event = AuditEvent::new(
		"subscription-pool",
		"owner",
		"key",
		"",
		"model",
		"complete",
		"done",
		json!({"payload": "secret text"}),
	);
	event.telemetry.provider = "anthropic".into();
	event.telemetry.input_tokens = Some(7);
	state
		.inner
		.store
		.lock()
		.unwrap()
		.append_audit(&event)
		.unwrap();
	*status.lock().unwrap() = StatusCode::SERVICE_UNAVAILABLE;
	assert!(export_once(&state, &exporter).await.is_err());
	*status.lock().unwrap() = StatusCode::OK;
	assert_eq!(export_once(&state, &exporter).await.unwrap(), 1);
	assert_eq!(export_once(&state, &exporter).await.unwrap(), 0);
	let received = received.lock().unwrap();
	assert_eq!(received.len(), 2);
	let (auth, body) = &received[1];
	assert_eq!(auth.as_deref(), Some("Bearer collector"));
	let text = body.to_string();
	assert!(text.contains("gen_ai.provider.name"));
	assert!(text.contains("gen_ai.usage.input_tokens"));
	assert!(!text.contains("secret text"));
}
