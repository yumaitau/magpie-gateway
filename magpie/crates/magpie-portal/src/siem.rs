//! UI-configured SIEM sinks (Microsoft Sentinel Logs Ingestion API, OTLP, Splunk HEC, Syslog)
//! plus downloadable Microsoft Sentinel DCR schema & KQL analytics pack.
use super::*;
use magpie::sinks::{AuditSink, SinkStatus};

const BATCH: usize = 512;

fn json_error(status: StatusCode, message: impl Into<String>) -> Response {
	(status, Json(json!({ "error": message.into() }))).into_response()
}

pub(super) fn routes() -> Router<AppState> {
	Router::new()
		.route("/api/magpie/sinks", get(list_sinks).post(save_sink))
		.route("/api/magpie/sinks/{id}/delete", post(delete_sink))
		.route("/api/magpie/sinks/{id}/test", post(test_sink))
		.route("/api/magpie/sinks/sentinel-pack", get(sentinel_pack))
}

pub fn start(state: AppState) {
	tokio::spawn(async move {
		loop {
			if let Err(err) = deliver_all(&state).await {
				eprintln!("SIEM sink worker: {err}");
			}
			tokio::time::sleep(Duration::from_secs(5)).await;
		}
	});
}

async fn deliver_all(state: &AppState) -> Result<(), String> {
	let sinks = {
		let store = state.inner.store.lock().expect("db");
		store.audit_sinks().map_err(|e| e.to_string())?
	};
	for sink in sinks.into_iter().filter(|s| s.enabled) {
		let batch = {
			let store = state.inner.store.lock().expect("db");
			store
				.audit_sink_batch(&sink.id, BATCH)
				.map_err(|e| e.to_string())?
		};
		let Some(&(last_pos, _)) = batch.last() else {
			continue;
		};
		let events: Vec<AuditEvent> = batch.into_iter().map(|(_, ev)| ev).collect();
		match send_batch(&state.inner.gateway_client, &sink, &events).await {
			Ok(()) => {
				let _ = state
					.inner
					.store
					.lock()
					.expect("db")
					.audit_sink_record_result(&sink.id, Some(last_pos), events.len(), None);
			},
			Err((retryable, msg)) => {
				let advance = if retryable { None } else { Some(last_pos) };
				let _ = state
					.inner
					.store
					.lock()
					.expect("db")
					.audit_sink_record_result(&sink.id, advance, 0, Some(&msg));
			},
		}
	}
	Ok(())
}

async fn acquire_sentinel_token(
	client: &reqwest::Client,
	sink: &AuditSink,
) -> Result<String, (bool, String)> {
	if sink.auth_mode == "managed-identity" {
		let mut url = "http://169.254.169.254/metadata/identity/oauth2/token?api-version=2018-02-01&resource=https://monitor.azure.com/".to_string();
		if !sink.client_id.trim().is_empty() {
			url.push_str("&client_id=");
			url.push_str(sink.client_id.trim());
		}
		let resp = client
			.get(&url)
			.header("Metadata", "true")
			.timeout(Duration::from_secs(8))
			.send()
			.await
			.map_err(|_| {
				(
					true,
					"Azure Managed Identity endpoint (IMDS) unreachable.".to_string(),
				)
			})?;
		if !resp.status().is_success() {
			return Err((
				true,
				format!(
					"Managed Identity token returned HTTP {}",
					resp.status().as_u16()
				),
			));
		}
		let body: Value = resp
			.json()
			.await
			.map_err(|_| (true, "Invalid Managed Identity token JSON.".to_string()))?;
		return body["access_token"]
			.as_str()
			.map(str::to_string)
			.ok_or_else(|| {
				(
					true,
					"Managed Identity response missing access_token.".to_string(),
				)
			});
	}

	if sink.tenant_id.trim().is_empty()
		|| sink.client_id.trim().is_empty()
		|| sink.secret.trim().is_empty()
	{
		return Err((
			false,
			"Sentinel client-credentials auth requires tenant ID, client ID, and client secret."
				.to_string(),
		));
	}
	let token_url = format!(
		"https://login.microsoftonline.com/{}/oauth2/v2.0/token",
		sink.tenant_id.trim()
	);
	let resp = client
		.post(&token_url)
		.timeout(Duration::from_secs(10))
		.form(&[
			("grant_type", "client_credentials"),
			("client_id", sink.client_id.trim()),
			("client_secret", sink.secret.trim()),
			("scope", "https://monitor.azure.com/.default"),
		])
		.send()
		.await
		.map_err(|_| {
			(
				true,
				"Microsoft Entra token endpoint unreachable.".to_string(),
			)
		})?;
	if !resp.status().is_success() {
		return Err((
			matches!(resp.status().as_u16(), 429 | 502 | 503 | 504),
			format!(
				"Entra token endpoint returned HTTP {}",
				resp.status().as_u16()
			),
		));
	}
	let body: Value = resp
		.json()
		.await
		.map_err(|_| (true, "Invalid Entra token response.".to_string()))?;
	body["access_token"]
		.as_str()
		.map(str::to_string)
		.ok_or_else(|| {
			(
				true,
				"Entra token response missing access_token.".to_string(),
			)
		})
}

async fn send_batch(
	client: &reqwest::Client,
	sink: &AuditSink,
	events: &[AuditEvent],
) -> Result<(), (bool, String)> {
	match sink.kind.as_str() {
		"sentinel" => {
			let token = acquire_sentinel_token(client, sink).await?;
			let stream = if sink.stream_name.trim().is_empty() {
				"Custom-MagpieAudit_CL"
			} else {
				sink.stream_name.trim()
			};
			let url = format!(
				"{}/dataCollectionRules/{}/streams/{}?api-version=2023-01-01",
				sink.endpoint.trim().trim_end_matches('/'),
				sink.dcr_immutable_id.trim(),
				stream
			);
			let records: Vec<Value> = events
				.iter()
				.map(|e| magpie::sinks::sentinel_record(e, sink.capture_payloads))
				.collect();
			let resp = client
				.post(&url)
				.bearer_auth(&token)
				.timeout(Duration::from_secs(15))
				.json(&records)
				.send()
				.await
				.map_err(|_| {
					(
						true,
						"Microsoft Sentinel DCE endpoint unreachable.".to_string(),
					)
				})?;
			let code = resp.status().as_u16();
			if resp.status().is_success() {
				Ok(())
			} else {
				Err((
					matches!(code, 429 | 502 | 503 | 504),
					format!("Sentinel Logs Ingestion API returned HTTP {code}"),
				))
			}
		},
		"otlp" => {
			let mut req = client
				.post(sink.endpoint.trim())
				.timeout(Duration::from_secs(15))
				.json(&magpie::sinks::otlp_logs(events, sink.capture_payloads));
			if !sink.secret.trim().is_empty() {
				req = req.bearer_auth(sink.secret.trim());
			}
			let resp = req
				.send()
				.await
				.map_err(|_| (true, "OTLP collector unreachable.".to_string()))?;
			let code = resp.status().as_u16();
			if resp.status().is_success() {
				Ok(())
			} else {
				Err((
					matches!(code, 429 | 502 | 503 | 504),
					format!("OTLP endpoint returned HTTP {code}"),
				))
			}
		},
		"splunk" => {
			let payload = events
				.iter()
				.map(|e| magpie::sinks::splunk_hec(e, sink.capture_payloads).to_string())
				.collect::<Vec<_>>()
				.join("\n");
			let mut req = client
				.post(sink.endpoint.trim())
				.timeout(Duration::from_secs(15))
				.header(header::CONTENT_TYPE, "application/json")
				.body(payload);
			if !sink.secret.trim().is_empty() {
				req = req.header(
					header::AUTHORIZATION,
					format!("Splunk {}", sink.secret.trim()),
				);
			}
			let resp = req
				.send()
				.await
				.map_err(|_| (true, "Splunk HEC endpoint unreachable.".to_string()))?;
			let code = resp.status().as_u16();
			if resp.status().is_success() {
				Ok(())
			} else {
				Err((
					matches!(code, 429 | 502 | 503 | 504),
					format!("Splunk HEC returned HTTP {code}"),
				))
			}
		},
		"syslog" => {
			let payload = events
				.iter()
				.map(magpie::sinks::syslog_line)
				.collect::<Vec<_>>()
				.join("\n");
			let mut req = client
				.post(sink.endpoint.trim())
				.timeout(Duration::from_secs(15))
				.header(header::CONTENT_TYPE, "text/plain; charset=utf-8")
				.body(payload);
			if !sink.secret.trim().is_empty() {
				req = req.bearer_auth(sink.secret.trim());
			}
			let resp = req
				.send()
				.await
				.map_err(|_| (true, "Syslog ingest endpoint unreachable.".to_string()))?;
			let code = resp.status().as_u16();
			if resp.status().is_success() {
				Ok(())
			} else {
				Err((
					matches!(code, 429 | 502 | 503 | 504),
					format!("Syslog ingest returned HTTP {code}"),
				))
			}
		},
		_ => Err((false, "Unsupported sink kind.".to_string())),
	}
}

fn public_sink(sink: &AuditSink, status: Option<&SinkStatus>) -> Value {
	json!({
		"id": sink.id,
		"name": sink.name,
		"kind": sink.kind,
		"enabled": sink.enabled,
		"capturePayloads": sink.capture_payloads,
		"endpoint": sink.endpoint,
		"dcrImmutableId": sink.dcr_immutable_id,
		"streamName": sink.stream_name,
		"authMode": sink.auth_mode,
		"tenantId": sink.tenant_id,
		"clientId": sink.client_id,
		"hasSecret": !sink.secret.is_empty(),
		"status": status.map(|s| json!({
			"position": s.position,
			"lastOkAt": s.last_ok_at,
			"lastError": s.last_error,
			"sentCount": s.sent_count,
		})).unwrap_or_else(|| json!({
			"position": 0,
			"lastOkAt": "",
			"lastError": "",
			"sentCount": 0,
		})),
	})
}

async fn list_sinks(State(state): State<AppState>, headers: HeaderMap) -> Response {
	let session = match require(&state, &headers, Permission::ReadSettings) {
		Ok(s) => s,
		Err(r) => return r,
	};
	let can_manage = access::allows(&state, &session, Permission::ManageSettings);
	let store = state.inner.store.lock().expect("db");
	let sinks = store.audit_sinks().unwrap_or_default();
	let statuses = store.audit_sink_statuses().unwrap_or_default();
	drop(store);
	let list: Vec<Value> = sinks
		.iter()
		.map(|s| {
			let st = statuses.iter().find(|item| item.sink_id == s.id);
			public_sink(s, st)
		})
		.collect();
	let env_otlp = std::env::var("OTEL_EXPORTER_OTLP_LOGS_ENDPOINT")
		.or_else(|_| std::env::var("OTEL_EXPORTER_OTLP_ENDPOINT"))
		.ok()
		.filter(|v| !v.trim().is_empty())
		.is_some();
	Json(json!({
		"sinks": list,
		"envOtlpConfigured": env_otlp,
		"canManage": can_manage,
	}))
	.into_response()
}

fn validate_sink(sink: &AuditSink) -> Result<(), &'static str> {
	if sink.name.trim().is_empty() || sink.name.len() > 100 {
		return Err("Enter a sink name (up to 100 characters).");
	}
	if !matches!(
		sink.kind.as_str(),
		"sentinel" | "otlp" | "splunk" | "syslog"
	) {
		return Err("Choose sentinel, otlp, splunk, or syslog.");
	}
	let ep = sink.endpoint.trim();
	if ep.is_empty() || (!ep.starts_with("https://") && !ep.starts_with("http://")) {
		return Err("Endpoint URL must start with https:// (or http:// for internal collectors).");
	}
	if sink.kind == "sentinel" {
		if sink.dcr_immutable_id.trim().is_empty() {
			return Err("Microsoft Sentinel requires a Data Collection Rule (DCR) immutable ID.");
		}
		if !matches!(
			sink.auth_mode.as_str(),
			"managed-identity" | "client-credentials"
		) {
			return Err("Choose managed-identity or client-credentials authentication for Sentinel.");
		}
		if sink.auth_mode == "client-credentials"
			&& (sink.tenant_id.trim().is_empty() || sink.client_id.trim().is_empty())
		{
			return Err("Client credentials authentication requires Entra tenant ID and client ID.");
		}
	}
	Ok(())
}

async fn save_sink(
	State(state): State<AppState>,
	headers: HeaderMap,
	Json(mut sink): Json<AuditSink>,
) -> Response {
	let session = match require(&state, &headers, Permission::ManageSettings) {
		Ok(s) => s,
		Err(r) => return r,
	};
	if sink.id.trim().is_empty() {
		sink.id = magpie::new_id();
	}
	if sink.kind == "sentinel" && sink.stream_name.trim().is_empty() {
		sink.stream_name = "Custom-MagpieAudit_CL".into();
	}
	if sink.kind == "sentinel" && sink.auth_mode.trim().is_empty() {
		sink.auth_mode = "managed-identity".into();
	}
	if let Err(msg) = validate_sink(&sink) {
		return json_error(StatusCode::BAD_REQUEST, msg);
	}
	let store = state.inner.store.lock().expect("db");
	if let Err(err) = store.save_audit_sink(&sink, &session.email) {
		return json_error(StatusCode::INTERNAL_SERVER_ERROR, err.to_string());
	}
	let saved = store
		.audit_sinks()
		.unwrap_or_default()
		.into_iter()
		.find(|item| item.id == sink.id)
		.unwrap_or(sink);
	let statuses = store.audit_sink_statuses().unwrap_or_default();
	let st = statuses.iter().find(|item| item.sink_id == saved.id);
	Json(public_sink(&saved, st)).into_response()
}

async fn delete_sink(
	State(state): State<AppState>,
	headers: HeaderMap,
	Path(id): Path<String>,
) -> Response {
	let session = match require(&state, &headers, Permission::ManageSettings) {
		Ok(s) => s,
		Err(r) => return r,
	};
	if let Err(err) = state
		.inner
		.store
		.lock()
		.expect("db")
		.delete_audit_sink(&id, &session.email)
	{
		return json_error(StatusCode::INTERNAL_SERVER_ERROR, err.to_string());
	}
	Json(json!({ "deleted": id })).into_response()
}

async fn test_sink(
	State(state): State<AppState>,
	headers: HeaderMap,
	Path(id): Path<String>,
) -> Response {
	let session = match require(&state, &headers, Permission::ManageSettings) {
		Ok(s) => s,
		Err(r) => return r,
	};
	let sink = {
		let store = state.inner.store.lock().expect("db");
		store
			.audit_sinks()
			.unwrap_or_default()
			.into_iter()
			.find(|item| item.id == id)
	};
	let Some(sink) = sink else {
		return json_error(StatusCode::NOT_FOUND, "Sink not found.");
	};
	let sample = AuditEvent::new(
		"sinks",
		&session.email,
		"magpie-test",
		"synthetic_test",
		"none",
		"allow",
		"Synthetic SIEM connectivity test from Magpie Gateway.",
		json!({"synthetic": true}),
	);
	match send_batch(&state.inner.gateway_client, &sink, &[sample]).await {
		Ok(()) => Json(json!({
			"ok": true,
			"message": format!("Delivered synthetic test event to '{}'.", sink.name)
		}))
		.into_response(),
		Err((_, err)) => Json(json!({
			"ok": false,
			"error": err,
		}))
		.into_response(),
	}
}

async fn sentinel_pack(State(state): State<AppState>, headers: HeaderMap) -> Response {
	if let Err(r) = require(&state, &headers, Permission::ReadSettings) {
		return r;
	}
	let pack = json!({
		"schema": "magpie.sentinel-pack.v1",
		"tableName": "MagpieAudit_CL",
		"streamName": "Custom-MagpieAudit_CL",
		"dataCollectionRuleStreamDeclaration": {
			"Custom-MagpieAudit_CL": {
				"columns": [
					{ "name": "TimeGenerated", "type": "datetime" },
					{ "name": "EventId", "type": "string" },
					{ "name": "Kind", "type": "string" },
					{ "name": "Actor", "type": "string" },
					{ "name": "AgentId", "type": "string" },
					{ "name": "Tool", "type": "string" },
					{ "name": "Model", "type": "string" },
					{ "name": "Decision", "type": "string" },
					{ "name": "Reason", "type": "string" },
					{ "name": "Detail", "type": "dynamic" }
				]
			}
		},
		"kqlAnalyticsRules": [
			{
				"id": "after-hours",
				"name": "Magpie — After-hours AI gateway activity",
				"query": include_str!("../../../../detections/sentinel/after-hours.kql")
			},
			{
				"id": "blocked-destination",
				"name": "Magpie — Blocked non-sovereign or unapproved destination",
				"query": include_str!("../../../../detections/sentinel/blocked-destination.kql")
			},
			{
				"id": "credential-in-arguments",
				"name": "Magpie — Credential detected in prompt or tool arguments",
				"query": include_str!("../../../../detections/sentinel/credential-in-arguments.kql")
			},
			{
				"id": "dlp-block-spike",
				"name": "Magpie — Spike in Australian DLP blocks",
				"query": include_str!("../../../../detections/sentinel/dlp-block-spike.kql")
			},
			{
				"id": "tool-call-spree",
				"name": "Magpie — Agent tool call rate-limit spree",
				"query": include_str!("../../../../detections/sentinel/tool-call-spree.kql")
			},
			{
				"id": "unapproved-tool",
				"name": "Magpie — Attempt to invoke unapproved MCP tool",
				"query": include_str!("../../../../detections/sentinel/unapproved-tool.kql")
			}
		]
	});
	(
		[
			(header::CONTENT_TYPE, "application/json; charset=utf-8"),
			(
				header::CONTENT_DISPOSITION,
				"attachment; filename=\"magpie-sentinel-pack.json\"",
			),
		],
		serde_json::to_string_pretty(&pack).unwrap_or_default(),
	)
		.into_response()
}
