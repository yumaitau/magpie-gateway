//! Administrator-managed destinations. Workers never hold the DB lock during I/O.
use super::*;
use lettre::{
	AsyncSmtpTransport, AsyncTransport, Message, Tokio1Executor,
	transport::smtp::authentication::Credentials,
};
use magpie::alerts::{Delivery, Destination};

pub fn start(state: AppState) {
	tokio::spawn(async move {
		loop {
			if let Err(error) = deliver_pending(&state).await {
				eprintln!("Alert queue unavailable: {error}");
			}
			tokio::time::sleep(Duration::from_secs(5)).await;
		}
	});
}
async fn deliver_pending(state: &AppState) -> magpie::Result<()> {
	let jobs = {
		let store = state.inner.store.lock().expect("db");
		store.collect_alerts()?;
		store.alert_deliveries(true)?
	};
	for job in jobs {
		let destination = {
			let store = state.inner.store.lock().expect("db");
			if !store.alert_is_pending(&job.id)? {
				continue;
			}
			store
				.alert_destinations()?
				.into_iter()
				.find(|d| d.id == job.destination && d.enabled)
		};
		if let Some(d) = destination {
			let result = send(&state.inner.gateway_client, &d, &job).await;
			state.inner.store.lock().expect("db").finish_alert(
				&job.id,
				job.attempts + 1,
				result.as_ref().err().map(String::as_str),
			)?;
		}
	}
	Ok(())
}
fn message(job: &Delivery) -> String {
	format!(
		"Magpie Gateway alert\nEvent: {}\nTime: {}\nKind: {}\nDecision: {}\nLog-only action: {}\nOpen the gateway audit log for details.",
		job.body["id"].as_str().unwrap_or("test"),
		job.body["at"].as_str().unwrap_or(""),
		job.body["kind"].as_str().unwrap_or("test"),
		job.body["decision"].as_str().unwrap_or("test"),
		job.body["wouldAction"].as_str().unwrap_or("none")
	)
}
fn webhook_body(d: &Destination, job: &Delivery) -> Value {
	let text = message(job);
	match d.kind.as_str() {
		"slack" => {
			json!({"text":text,"blocks":[{"type":"section","text":{"type":"plain_text","text":text}}]})
		},
		"teams" => {
			json!({"type":"message","attachments":[{"contentType":"application/vnd.microsoft.card.adaptive","contentUrl":null,"content":{"$schema":"http://adaptivecards.io/schemas/adaptive-card.json","type":"AdaptiveCard","version":"1.2","body":[{"type":"TextBlock","text":text,"wrap":true}]}}]})
		},
		_ => json!({"source":"magpie-gateway","deliveryId":job.id,"event":job.body}),
	}
}
async fn send(client: &reqwest::Client, d: &Destination, job: &Delivery) -> Result<(), String> {
	if d.kind == "email" {
		let email = Message::builder()
			.from(d.smtp_from.parse().map_err(|_| "Invalid sender")?)
			.to(d.recipient.parse().map_err(|_| "Invalid recipient")?)
			.subject("Magpie Gateway alert")
			.body(message(job))
			.map_err(|_| "Invalid email")?;
		let builder = if d.smtp_port == 465 {
			AsyncSmtpTransport::<Tokio1Executor>::relay(&d.smtp_host)
		} else {
			AsyncSmtpTransport::<Tokio1Executor>::starttls_relay(&d.smtp_host)
		}
		.map_err(|_| "Invalid SMTP host")?;
		let mut builder = builder
			.port(d.smtp_port)
			.timeout(Some(Duration::from_secs(15)));
		if !d.smtp_username.is_empty() {
			builder = builder.credentials(Credentials::new(
				d.smtp_username.clone(),
				d.smtp_password.clone(),
			));
		}
		tokio::time::timeout(Duration::from_secs(20), builder.build().send(email))
			.await
			.map_err(|_| "SMTP timeout")?
			.map_err(|_| "SMTP delivery failed; check TLS, credentials and recipient")?;
	} else {
		let mut request = client
			.post(&d.url)
			.timeout(Duration::from_secs(15))
			.header("X-Magpie-Delivery-Id", &job.id)
			.json(&webhook_body(d, job));
		if d.kind == "webhook" && !d.bearer.is_empty() {
			request = request.bearer_auth(&d.bearer);
		}
		let response = request
			.send()
			.await
			.map_err(|_| "Webhook connection failed; check URL and TLS")?;
		if !response.status().is_success() {
			return Err(format!("HTTP {}", response.status().as_u16()));
		}
	}
	Ok(())
}
fn validate(d: &Destination) -> Result<(), &'static str> {
	if d.name.trim().is_empty() || d.name.len() > 100 {
		return Err("Enter a destination name (up to 100 characters).");
	}
	if !matches!(d.trigger.as_str(), "security" | "blocked" | "changes") {
		return Err("Choose an alert trigger.");
	}
	if !matches!(d.kind.as_str(), "slack" | "teams" | "webhook" | "email") {
		return Err("Choose Slack, Teams, webhook or email.");
	}
	if d.kind == "email" {
		if d.recipient.parse::<lettre::message::Mailbox>().is_err()
			|| d.smtp_from.parse::<lettre::message::Mailbox>().is_err()
		{
			return Err("Enter valid sender and recipient email addresses.");
		}
		if d.smtp_host.is_empty()
			|| d.smtp_host.contains(['/', ':', ' ', '\n', '\r'])
			|| d.smtp_port == 0
		{
			return Err("Enter an SMTP hostname and port. TLS is required.");
		}
	} else {
		let url = reqwest::Url::parse(&d.url).map_err(|_| "Enter a valid HTTPS webhook URL.")?;
		if url.scheme() != "https"
			|| url.host_str().is_none()
			|| !url.username().is_empty()
			|| url.password().is_some()
			|| url.fragment().is_some()
		{
			return Err("Webhook URLs must use HTTPS, with no embedded login or fragment.");
		}
	}
	if d.bearer.contains(['\r', '\n']) {
		return Err("Bearer tokens cannot contain line breaks.");
	}
	Ok(())
}
pub(super) async fn page_get(State(state): State<AppState>, headers: HeaderMap) -> Response {
	if let Err(r) = require(&state, &headers, Permission::ManageSettings) {
		return r;
	}
	let (destinations, deliveries) = {
		let store = state.inner.store.lock().expect("db");
		match (store.alert_destinations(), store.alert_deliveries(false)) {
			(Ok(d), Ok(j)) => (d, j),
			_ => return (StatusCode::INTERNAL_SERVER_ERROR, "Could not load alerts.").into_response(),
		}
	};
	let mut body = String::from(
		"<p>Choose what should notify your team and where to send it. Alerts include the event ID and decision. Prompt text and personal details are not sent.</p><p>Delivery runs in the background and retries failures up to five times. Disabled destinations cancel queued alerts.</p><h2>Destinations</h2>",
	);
	if destinations.is_empty() {
		body.push_str("<p>No destinations yet. Add Slack, Teams, a webhook or email below.</p>");
	}
	for d in &destinations {
		body.push_str(&format!(
			"<details><summary>{} · {} · {}</summary>{}</details>",
			ui::esc(&d.name),
			ui::esc(&d.kind),
			if d.enabled { "On" } else { "Off" },
			destination_form(d)
		));
	}
	body.push_str("<h2>Add a destination</h2><p>For Slack or Teams, paste an incoming webhook URL. For Teams, use a Workflows webhook that accepts requests from anyone with its secret URL.</p>");
	body.push_str(&destination_form(&Destination {
		trigger: "security".into(),
		kind: "slack".into(),
		smtp_port: 587,
		..Default::default()
	}));
	body.push_str("<h2>Recent deliveries</h2><p>Refresh this page to see updated delivery status. A successful webhook status means the endpoint accepted it; SMTP success means the mail server accepted it.</p>");
	let rows = deliveries
		.iter()
		.map(|j| {
			vec![
				ui::esc(&j.created_at),
				ui::esc(
					destinations
						.iter()
						.find(|d| d.id == j.destination)
						.map(|d| d.name.as_str())
						.unwrap_or("Removed destination"),
				),
				ui::esc(&j.status),
				j.attempts.to_string(),
				ui::esc(&j.result),
			]
		})
		.collect::<Vec<_>>();
	body.push_str(&table(
		&["Time", "Destination", "Status", "Attempts", "Result"],
		&rows,
	));
	page(&state, &headers, "/alerts", "Alerts", body)
}
fn destination_form(d: &Destination) -> String {
	let id = if d.id.is_empty() { "new" } else { &d.id };
	let input = |name: &str, label: &str, value: &str, kind: &str| {
		format!(
			"<label for='{id}-{name}'>{label}</label><input id='{id}-{name}' name='{name}' type='{kind}' value='{}' autocomplete='off'>",
			ui::esc(value)
		)
	};
	let mut out = format!(
		"<form class='alert-destination' novalidate method='post' action='/alerts'><input type='hidden' name='id' value='{}'>",
		ui::esc(&d.id)
	);
	out.push_str(&input("name", "Destination name", &d.name, "text"));
	for (field, label, values, current) in [
		(
			"kind",
			"Send to",
			vec![
				("slack", "Slack"),
				("teams", "Teams"),
				("webhook", "Webhook"),
				("email", "Email"),
			],
			d.kind.as_str(),
		),
		(
			"trigger",
			"Notify me about",
			vec![
				("security", "Security findings (including log-only)"),
				("blocked", "Blocked requests (including log-only)"),
				("changes", "Policy, settings and access changes"),
			],
			d.trigger.as_str(),
		),
	] {
		out.push_str(&format!(
			"<label for='{id}-{field}'>{label}</label><select id='{id}-{field}' name='{field}'>"
		));
		for (value, text) in values {
			out.push_str(&format!(
				"<option value='{value}' {}>{text}</option>",
				selected(value == current)
			));
		}
		out.push_str("</select>");
	}
	out.push_str("<div class='alert-webhook'>");
	out.push_str(&input(
		"url",
		if d.url.is_empty() {
			"Webhook URL (Slack, Teams or webhook)"
		} else {
			"Replace webhook URL (leave blank to keep saved URL)"
		},
		"",
		"password",
	));
	out.push_str("</div><div class='alert-email'>");
	out.push_str(&input(
		"recipient",
		"Email recipient (email only)",
		&d.recipient,
		"email",
	));
	out.push_str("</div><details><summary>Email and advanced settings</summary><div class='alert-email'><p>Email requires an SMTP server. Use port 587 for STARTTLS or 465 for TLS.</p>");
	for (name, label, value, kind) in [
		("smtp_host", "SMTP host", d.smtp_host.as_str(), "text"),
		("smtp_from", "Sender email", d.smtp_from.as_str(), "email"),
		(
			"smtp_username",
			"SMTP username",
			d.smtp_username.as_str(),
			"text",
		),
		(
			"smtp_password",
			"SMTP password (blank keeps saved password)",
			"",
			"password",
		),
	] {
		out.push_str(&input(name, label, value, kind));
	}
	out.push_str(&input(
		"smtp_port",
		"SMTP port",
		&d.smtp_port.to_string(),
		"number",
	));
	out.push_str("</div><div class='alert-webhook'>");
	out.push_str(&input(
		"bearer",
		"Webhook bearer token (blank keeps saved token)",
		"",
		"password",
	));
	out.push_str("</div><label><input type='checkbox' name='clear_secrets' value='1'> Clear saved SMTP password and bearer token</label></details>");
	out.push_str(&format!("<label><input type='checkbox' name='enabled' value='1' {}> Enable alerts</label><button type='submit' name='action' value='save'>Save destination</button>",if d.enabled{"checked"}else{""}));
	if !d.id.is_empty() {
		out.push_str("<button type='submit' name='action' value='test'>Save and send test alert</button><button type='submit' name='action' value='delete'>Delete destination</button>");
	}
	out.push_str("</form>");
	out
}
#[derive(Deserialize)]
pub(super) struct DestinationForm {
	#[serde(flatten)]
	fields: HashMap<String, String>,
}
pub(super) async fn post(
	State(state): State<AppState>,
	headers: HeaderMap,
	Form(form): Form<DestinationForm>,
) -> Response {
	let session = match require(&state, &headers, Permission::ManageSettings) {
		Ok(s) => s,
		Err(r) => return r,
	};
	let f = &form.fields;
	let get = |key: &str| f.get(key).map(String::as_str).unwrap_or("");
	let store = state.inner.store.lock().expect("db");
	let destinations = match store.alert_destinations() {
		Ok(d) => d,
		Err(_) => {
			return (
				StatusCode::INTERNAL_SERVER_ERROR,
				"Could not load destinations.",
			)
				.into_response();
		},
	};
	let old = destinations.iter().find(|d| d.id == get("id"));
	if !get("id").is_empty() && old.is_none() {
		return (StatusCode::NOT_FOUND, "Destination no longer exists.").into_response();
	}
	if get("action") == "delete" {
		return match store.delete_alert_destination(get("id"), &session.email) {
			Ok(()) => Redirect::to("/alerts").into_response(),
			Err(_) => (
				StatusCode::INTERNAL_SERVER_ERROR,
				"Could not delete destination.",
			)
				.into_response(),
		};
	}
	if old.is_none() && destinations.len() >= 20 {
		return (StatusCode::BAD_REQUEST, "Limit of 20 destinations reached.").into_response();
	}
	let prior = old.cloned().unwrap_or_default();
	let keep = |key: &str, value: &str| {
		if get(key).is_empty() {
			value.to_owned()
		} else {
			get(key).to_owned()
		}
	};
	let d = Destination {
		after_event: 0,
		id: if prior.id.is_empty() {
			magpie::new_id()
		} else {
			prior.id.clone()
		},
		name: get("name").trim().into(),
		kind: get("kind").into(),
		trigger: get("trigger").into(),
		enabled: get("enabled") == "1",
		url: keep("url", &prior.url),
		bearer: if get("clear_secrets") == "1" {
			String::new()
		} else {
			keep("bearer", &prior.bearer)
		},
		recipient: get("recipient").trim().into(),
		smtp_host: get("smtp_host").trim().into(),
		smtp_port: if get("smtp_port").is_empty() {
			587
		} else {
			get("smtp_port").parse().unwrap_or(0)
		},
		smtp_username: get("smtp_username").into(),
		smtp_password: if get("clear_secrets") == "1" {
			String::new()
		} else {
			keep("smtp_password", &prior.smtp_password)
		},
		smtp_from: get("smtp_from").trim().into(),
	};
	if let Err(error) = validate(&d) {
		return (StatusCode::BAD_REQUEST, error).into_response();
	}
	if get("action") == "test" && !d.enabled {
		return (
			StatusCode::BAD_REQUEST,
			"Enable the destination before sending a test alert.",
		)
			.into_response();
	}
	let result = (|| -> magpie::Result<()> {
		store.save_alert_destination(&d, &session.email)?;
		if get("action") == "test" {
			store.enqueue_alert(
				&d.id,
				&AuditEvent::new("test", "", "", "", "", "test", "", json!({})),
			)?;
		}
		Ok(())
	})();
	match result {
		Ok(()) => Redirect::to("/alerts").into_response(),
		Err(_) => (
			StatusCode::INTERNAL_SERVER_ERROR,
			"Could not save destination or queue test.",
		)
			.into_response(),
	}
}
#[cfg(test)]
mod tests {
	use super::*;
	#[tokio::test]
	async fn webhook_transports_send_expected_bodies_and_headers() {
		let (tx, mut rx) = tokio::sync::mpsc::channel(4);
		let app = Router::new().route(
			"/sink",
			axum::routing::post(move |headers: HeaderMap, Json(body): Json<Value>| {
				let tx = tx.clone();
				async move {
					tx.send((headers, body)).await.unwrap();
					StatusCode::ACCEPTED
				}
			}),
		);
		let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
		let addr = listener.local_addr().unwrap();
		let task = tokio::spawn(async move { axum::serve(listener, app).await.unwrap() });
		let job = Delivery {
			id: "stable-id".into(),
			destination: "one".into(),
			body: json!({"id":"event","decision":"block"}),
			status: "pending".into(),
			attempts: 0,
			result: String::new(),
			created_at: String::new(),
		};
		for kind in ["webhook", "slack", "teams"] {
			let d = Destination {
				kind: kind.into(),
				url: format!("http://{addr}/sink"),
				bearer: "test-token".into(),
				..Default::default()
			};
			send(&reqwest::Client::new(), &d, &job).await.unwrap();
			let (headers, body) = rx.recv().await.unwrap();
			assert_eq!(headers["x-magpie-delivery-id"], "stable-id");
			assert_eq!(body, webhook_body(&d, &job));
			if kind == "webhook" {
				assert_eq!(headers["authorization"], "Bearer test-token");
			} else {
				assert!(!headers.contains_key("authorization"));
			}
		}
		task.abort();
	}
	#[tokio::test]
	async fn email_refuses_a_server_without_starttls() {
		use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
		let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
		let port = listener.local_addr().unwrap().port();
		let task = tokio::spawn(async move {
			let (socket, _) = listener.accept().await.unwrap();
			let mut socket = BufReader::new(socket);
			socket
				.get_mut()
				.write_all(b"220 localhost SMTP ready\r\n")
				.await
				.unwrap();
			let mut line = String::new();
			socket.read_line(&mut line).await.unwrap();
			assert!(line.starts_with("EHLO"));
			socket
				.get_mut()
				.write_all(b"250-localhost\r\n250 OK\r\n")
				.await
				.unwrap();
		});
		let d = Destination {
			kind: "email".into(),
			smtp_host: "127.0.0.1".into(),
			smtp_port: port,
			smtp_from: "gateway@example.com".into(),
			recipient: "ops@example.com".into(),
			smtp_username: "user".into(),
			smtp_password: "secret".into(),
			..Default::default()
		};
		let job = Delivery {
			id: "id".into(),
			destination: "one".into(),
			body: json!({}),
			status: "pending".into(),
			attempts: 0,
			result: String::new(),
			created_at: String::new(),
		};
		assert!(
			send(&reqwest::Client::new(), &d, &job)
				.await
				.unwrap_err()
				.contains("SMTP delivery failed")
		);
		task.await.unwrap();
	}

	#[test]
	fn formats_and_validation() {
		let mut d = Destination {
			name: "test".into(),
			trigger: "security".into(),
			kind: "slack".into(),
			url: "https://example.com/secret".into(),
			..Default::default()
		};
		assert!(validate(&d).is_ok());
		let j = Delivery {
			id: "delivery".into(),
			destination: "one".into(),
			body: json!({"id":"event","decision":"block"}),
			status: "pending".into(),
			attempts: 0,
			result: String::new(),
			created_at: String::new(),
		};
		assert!(
			webhook_body(&d, &j)["text"]
				.as_str()
				.unwrap()
				.contains("block")
		);
		d.kind = "teams".into();
		assert_eq!(
			webhook_body(&d, &j)["attachments"][0]["content"]["type"],
			"AdaptiveCard"
		);
		d.kind = "webhook".into();
		assert_eq!(webhook_body(&d, &j)["event"]["id"], "event");
		for url in [
			"http://example.com",
			"file:///etc/passwd",
			"https://user:pass@example.com",
		] {
			d.url = url.into();
			assert!(validate(&d).is_err());
		}
	}
	#[tokio::test]
	async fn delivery_reports_http_failure_without_leaking_url() {
		let app = Router::new().route(
			"/secret",
			axum::routing::post(|| async { StatusCode::TOO_MANY_REQUESTS }),
		);
		let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
		let addr = listener.local_addr().unwrap();
		let task = tokio::spawn(async move { axum::serve(listener, app).await.unwrap() });
		let d = Destination {
			kind: "webhook".into(),
			url: format!("http://{addr}/secret"),
			..Default::default()
		};
		let j = Delivery {
			id: "delivery".into(),
			destination: "one".into(),
			body: json!({}),
			status: "pending".into(),
			attempts: 0,
			result: String::new(),
			created_at: String::new(),
		};
		assert_eq!(
			send(&reqwest::Client::new(), &d, &j).await.unwrap_err(),
			"HTTP 429"
		);
		task.abort();
	}
}
