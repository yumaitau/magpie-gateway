//! Audit export as OTLP/HTTP JSON logs, configured by the standard OpenTelemetry
//! exporter variables. Nothing is sent unless an endpoint is set.
use super::*;

const BATCH: usize = 512;

#[derive(Debug, PartialEq)]
pub struct Exporter {
	endpoint: String,
	headers: Vec<(String, String)>,
}

/// Read `OTEL_EXPORTER_OTLP_LOGS_*`, falling back to `OTEL_EXPORTER_OTLP_*`.
pub fn configure(env: impl Fn(&str) -> Option<String>) -> Result<Option<Exporter>, String> {
	let var = |name: &str| env(name).filter(|v| !v.trim().is_empty());
	if var("OTEL_SDK_DISABLED").is_some_and(|v| v.trim().eq_ignore_ascii_case("true")) {
		return Ok(None);
	}
	let endpoint = match (
		var("OTEL_EXPORTER_OTLP_LOGS_ENDPOINT"),
		var("OTEL_EXPORTER_OTLP_ENDPOINT"),
	) {
		(Some(logs), _) => logs.trim().to_string(),
		(None, Some(base)) => format!("{}/v1/logs", base.trim().trim_end_matches('/')),
		(None, None) => return Ok(None),
	};
	let protocol =
		var("OTEL_EXPORTER_OTLP_LOGS_PROTOCOL").or_else(|| var("OTEL_EXPORTER_OTLP_PROTOCOL"));
	if protocol.is_some_and(|p| p.trim() != "http/json") {
		return Err("Audit OTLP export supports only the http/json protocol.".into());
	}
	let headers = var("OTEL_EXPORTER_OTLP_LOGS_HEADERS")
		.or_else(|| var("OTEL_EXPORTER_OTLP_HEADERS"))
		.unwrap_or_default();
	let headers = headers
		.split(',')
		.filter(|pair| !pair.trim().is_empty())
		.map(|pair| {
			let (name, value) = pair
				.split_once('=')
				.ok_or("OTLP headers must be name=value pairs.")?;
			Ok((name.trim().to_string(), percent_decode(value.trim())?))
		})
		.collect::<Result<_, &str>>()?;
	Ok(Some(Exporter { endpoint, headers }))
}

fn percent_decode(value: &str) -> Result<String, &'static str> {
	let bytes = value.as_bytes();
	let mut out = Vec::with_capacity(bytes.len());
	let mut i = 0;
	while i < bytes.len() {
		if bytes[i] == b'%' {
			let hex = value
				.get(i + 1..i + 3)
				.and_then(|h| u8::from_str_radix(h, 16).ok())
				.ok_or("OTLP header values must be percent-encoded.")?;
			out.push(hex);
			i += 3;
		} else {
			out.push(bytes[i]);
			i += 1;
		}
	}
	String::from_utf8(out).map_err(|_| "OTLP header values must be UTF-8.")
}

pub fn start(state: AppState) {
	let exporter = match configure(|name| std::env::var(name).ok()) {
		Ok(Some(exporter)) => exporter,
		Ok(None) => return,
		Err(error) => {
			eprintln!("Audit OTLP export disabled: {error}");
			return;
		},
	};
	eprintln!("Audit OTLP export to {}.", exporter.endpoint);
	tokio::spawn(async move {
		loop {
			let delay = match export_once(&state, &exporter).await {
				Ok(n) if n == BATCH => 0,
				Ok(_) => 5,
				Err(error) => {
					eprintln!("Audit OTLP export: {error}");
					30
				},
			};
			tokio::time::sleep(Duration::from_secs(delay)).await;
		}
	});
}

/// Send one batch. Retryable failures keep the cursor; rejected batches are skipped.
pub async fn export_once(state: &AppState, exporter: &Exporter) -> Result<usize, String> {
	let (events, capture) = {
		let store = state.inner.store.lock().expect("db");
		let events = store.audit_export_batch(BATCH).map_err(|e| e.to_string())?;
		let capture = store
			.get_document("settings", "global")
			.ok()
			.flatten()
			.and_then(|doc| doc.body["capturePayloads"].as_bool())
			.unwrap_or(false);
		(events, capture)
	};
	let Some(&(last, _)) = events.last() else {
		return Ok(0);
	};
	let events: Vec<_> = events.into_iter().map(|(_, event)| event).collect();
	let mut request = state
		.inner
		.gateway_client
		.post(&exporter.endpoint)
		.timeout(Duration::from_secs(10))
		.json(&magpie::sinks::otlp_logs(&events, capture));
	for (name, value) in &exporter.headers {
		request = request.header(name, value);
	}
	let status = request
		.send()
		.await
		.map_err(|_| "collector unreachable".to_string())?
		.status();
	let retry = matches!(status.as_u16(), 429 | 502 | 503 | 504);
	if retry {
		return Err(format!("collector returned {status}; retrying"));
	}
	state
		.inner
		.store
		.lock()
		.expect("db")
		.advance_audit_export(last)
		.map_err(|e| e.to_string())?;
	if !status.is_success() {
		return Err(format!(
			"collector rejected {} events with {status}",
			events.len()
		));
	}
	Ok(events.len())
}

#[cfg(test)]
mod tests;
