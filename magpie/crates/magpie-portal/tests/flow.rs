//! Portal flows against the in-process mock identity provider.
//!
//! Example names and identifiers are synthetic.

use std::sync::Arc;
use std::time::Duration;

use magpie::identity::{
	BACK_CHANNEL_EVENT, BreakGlassConfig, IdClaims, OidcProviderKind, hash_password, issue_hs256,
};
use magpie_portal::Config;
use reqwest::cookie::{CookieStore, Jar};
use reqwest::header::{HeaderMap, LOCATION, SET_COOKIE};
use reqwest::{Client, StatusCode};
use serde_json::json;

fn config(break_glass: BreakGlassConfig, secure: bool) -> Config {
	Config {
		guardrail_mode: Default::default(),
		database_path: ":memory:".into(),
		cookie_secure: secure,
		demo: false,
		seed_examples: true,
		mock_idp: true,
		oidc_kind: OidcProviderKind::Generic,
		issuer: "http://login.example/mock".into(),
		client_id: "magpie".into(),
		client_secret: String::new(),
		audience: "magpie".into(),
		public_url: String::new(),
		role_map: Default::default(),
		webhook_token: String::new(),
		bootstrap_models: vec![],
		bootstrap_protected: vec![],
		bootstrap_owner: String::new(),
		hmac_secret: "test-secret".into(),
		session_secret: "test-secret".into(),
		scim_token: "example-token".into(),
		break_glass,
		idle: Duration::from_secs(30 * 60),
		absolute: Duration::from_secs(12 * 60 * 60),
		listen: "127.0.0.1:0".into(),
	}
}

async fn spawn(secure: bool) -> String {
	spawn_with(config(BreakGlassConfig::default(), secure)).await
}

async fn spawn_with(config: Config) -> String {
	let state = magpie_portal::state(config).expect("state");
	let app = magpie_portal::router(state);
	let listener = tokio::net::TcpListener::bind("127.0.0.1:0")
		.await
		.expect("bind");
	let address = listener.local_addr().expect("addr");
	tokio::spawn(async move {
		axum::serve(listener, app).await.expect("serve");
	});
	format!("http://{address}")
}

fn client(base: &str) -> (Client, Arc<Jar>) {
	let jar = Arc::new(Jar::default());
	let client = Client::builder()
		.cookie_provider(jar.clone())
		.redirect(reqwest::redirect::Policy::none())
		.timeout(Duration::from_secs(5))
		.build()
		.expect("client");
	let _ = base;
	(client, jar)
}

async fn login_response(client: &Client, base: &str, who: &str) -> reqwest::Response {
	let start = client
		.get(format!("{base}/api/auth/start"))
		.send()
		.await
		.expect("start");
	assert!(start.status().is_redirection(), "{}", start.status());
	let next = start
		.headers()
		.get(LOCATION)
		.expect("location")
		.to_str()
		.expect("utf8")
		.to_string();
	let page = client.get(absolute(base, &next)).send().await.expect("idp");
	assert_eq!(page.status(), StatusCode::OK);
	let html = page.text().await.expect("html");
	let state = html
		.split("name=\"state\" value=\"")
		.nth(1)
		.expect("state")
		.split('"')
		.next()
		.expect("value");
	let continued = client
		.post(format!("{base}/mock-idp/authorize"))
		.form(&[("state", state), ("who", who)])
		.send()
		.await
		.expect("continue");
	assert!(continued.status().is_redirection());
	let callback = continued
		.headers()
		.get(LOCATION)
		.expect("callback")
		.to_str()
		.expect("utf8")
		.to_string();
	client
		.get(absolute(base, &callback))
		.send()
		.await
		.expect("callback")
}

async fn login(client: &Client, base: &str, who: &str) -> HeaderMap {
	let done = login_response(client, base, who).await;
	assert!(done.status().is_redirection(), "{}", done.status());
	assert!(
		done
			.headers()
			.get(LOCATION)
			.and_then(|value| value.to_str().ok())
			.unwrap_or("")
			.contains("/dashboard")
	);
	done.headers().clone()
}

fn absolute(base: &str, location: &str) -> String {
	if location.starts_with("http://") || location.starts_with("https://") {
		location.to_string()
	} else {
		format!("{base}{location}")
	}
}

fn mcp(
	tool: Option<&str>,
	model: &str,
	region: &str,
	destination: &str,
	text: &str,
) -> serde_json::Value {
	json!({
		"method": "tools/call",
		"tool": tool,
		"arguments": {"q": "example notes"},
		"resultText": text,
		"agentId": "example-briefing-agent",
		"model": model,
		"destinationRegion": region,
		"destinationName": destination,
		"phase": "request",
		"actor": "alex.nguyen@example.gov.au"
	})
}

#[tokio::test]
async fn oidc_login_sets_a_session_cookie_and_opens_the_dashboard() {
	let base = spawn(false).await;
	let (client, jar) = client(&base);
	let headers = login(&client, &base, "alex").await;
	let cookie = headers
		.get(SET_COOKIE)
		.expect("set-cookie")
		.to_str()
		.expect("cookie");
	assert!(cookie.contains("HttpOnly"));
	assert!(cookie.contains("SameSite=Lax"));
	assert!(!cookie.contains("Secure"));
	assert!(jar.cookies(&base.parse().expect("url")).is_some());
	let dashboard = client
		.get(format!("{base}/dashboard"))
		.send()
		.await
		.expect("dashboard");
	assert_eq!(dashboard.status(), StatusCode::OK);
	let html = dashboard.text().await.expect("html");
	assert!(html.contains("Alex Nguyen"));
	assert!(html.contains("alex.nguyen@example.gov.au"));
	assert!(!html.contains("Example data"));
}

#[tokio::test]
async fn secure_cookie_flag_is_set_when_configured() {
	let base = spawn(true).await;
	let (client, _) = client(&base);
	let headers = login(&client, &base, "alex").await;
	let cookie = headers
		.get(SET_COOKIE)
		.expect("set-cookie")
		.to_str()
		.expect("cookie");
	assert!(cookie.contains("Secure"));
	assert!(cookie.contains("HttpOnly"));
}

#[tokio::test]
async fn unapproved_tool_is_denied_until_alex_approves_it() {
	let base = spawn(false).await;
	let (client, _) = client(&base);
	let denied = client
		.post(format!("{base}/mcp/check"))
		.json(&mcp(
			Some("example_search"),
			"example-au-model",
			"ap-southeast-2",
			"example-au-model",
			"",
		))
		.send()
		.await
		.expect("check");
	let denied: serde_json::Value = denied.json().await.expect("json");
	assert_eq!(denied["decision"], "reject");
	assert!(
		denied["reason"]
			.as_str()
			.unwrap_or("")
			.contains("not approved")
	);

	login(&client, &base, "alex").await;
	let approved = client
		.post(format!("{base}/approvals/approval-example/approve"))
		.send()
		.await
		.expect("approve");
	assert!(approved.status().is_redirection());

	let allowed = client
		.post(format!("{base}/mcp/check"))
		.json(&mcp(
			Some("example_search"),
			"example-au-model",
			"ap-southeast-2",
			"example-au-model",
			"Summarise the approved notes.",
		))
		.send()
		.await
		.expect("check");
	let allowed: serde_json::Value = allowed.json().await.expect("json");
	assert_eq!(allowed["decision"], "pass", "{allowed}");

	let policies = client
		.get(format!("{base}/policies"))
		.send()
		.await
		.expect("policies");
	let html = policies.text().await.expect("html");
	assert!(html.contains("mcp.tool.name in ['example_search']"));
}

#[tokio::test]
async fn approver_cannot_approve_their_own_request() {
	let base = spawn(false).await;
	let (client, _) = client(&base);
	login(&client, &base, "alex").await;
	let queued = client
		.post(format!("{base}/approvals"))
		.form(&[
			("name", "example_own"),
			("kind", "tool"),
			("business_owner", "Alex Nguyen"),
			("purpose", "Example only."),
			("classification", "OFFICIAL"),
			("expires_on", "2027-06-01"),
		])
		.send()
		.await
		.expect("queue");
	assert!(queued.status().is_redirection());
	let page = client
		.get(format!("{base}/approvals"))
		.send()
		.await
		.expect("page");
	let html = page.text().await.expect("html");
	let id = html
		.split("example_own")
		.nth(1)
		.expect("row")
		.split("/approvals/")
		.nth(1)
		.expect("form")
		.split('/')
		.next()
		.expect("id");
	assert_ne!(id, "approval-example");
	let rejected = client
		.post(format!("{base}/approvals/{id}/approve"))
		.send()
		.await
		.expect("approve");
	assert_eq!(rejected.status(), StatusCode::FORBIDDEN);
}

#[tokio::test]
async fn dlp_webhook_blocks_a_prompt_with_an_example_abn() {
	let base = spawn(false).await;
	let (client, _) = client(&base);
	let response = client
		.post(format!("{base}/guardrails/request"))
		.header("x-magpie-region", "ap-southeast-2")
		.header("x-magpie-destination", "example-au-model")
		.header("x-magpie-model", "example-au-model")
		.json(&json!({
			"body": {"messages": [{"role": "user", "content": "Example only. ABN 12 000 000 041."}]}
		}))
		.send()
		.await
		.expect("webhook");
	assert_eq!(response.status(), StatusCode::OK);
	let body: serde_json::Value = response.json().await.expect("body");
	assert_eq!(body["action"]["status_code"], 400, "{body}");
	let text = body.to_string();
	assert!(!text.contains("12 000 000 041"), "{text}");
	assert!(!text.contains("12000000041"), "{text}");
}

#[tokio::test]
async fn sovereign_mode_and_protected_destinations_are_blocked() {
	let base = spawn(false).await;
	let (client, _) = client(&base);
	let offshore = client
		.post(format!("{base}/mcp/check"))
		.json(&mcp(
			None,
			"example-au-model",
			"us-east-1",
			"example-au-model",
			"hello",
		))
		.send()
		.await
		.expect("offshore");
	let offshore: serde_json::Value = offshore.json().await.expect("json");
	assert_eq!(offshore["decision"], "reject");
	assert!(
		offshore["reason"]
			.as_str()
			.unwrap_or("")
			.contains("Sovereign mode")
	);

	let protected = client
		.post(format!("{base}/mcp/check"))
		.json(&mcp(
			None,
			"example-au-model",
			"ap-southeast-2",
			"example-other",
			"PROTECTED example only",
		))
		.send()
		.await
		.expect("protected");
	let protected: serde_json::Value = protected.json().await.expect("json");
	assert_eq!(protected["decision"], "reject", "{protected}");
	assert!(
		protected["reason"]
			.as_str()
			.unwrap_or("")
			.contains("PROTECTED")
	);
}

#[tokio::test]
async fn scim_rejects_a_missing_token_and_creates_a_user() {
	let base = spawn(false).await;
	let (client, _) = client(&base);
	let denied = client
		.get(format!("{base}/scim/v2/Users"))
		.send()
		.await
		.expect("users");
	assert_eq!(denied.status(), StatusCode::UNAUTHORIZED);
	let created = client
		.post(format!("{base}/scim/v2/Users"))
		.bearer_auth("example-token")
		.json(&json!({"userName": "casey.morgan@example.gov.au", "displayName": "Casey Morgan"}))
		.send()
		.await
		.expect("create");
	assert_eq!(created.status(), StatusCode::CREATED);
	let listed = client
		.get(format!(
			"{base}/scim/v2/Users?filter=userName%20eq%20%22casey.morgan@example.gov.au%22"
		))
		.bearer_auth("example-token")
		.send()
		.await
		.expect("list");
	let listed: serde_json::Value = listed.json().await.expect("json");
	assert_eq!(listed["totalResults"], 1);
}

#[tokio::test]
async fn break_glass_is_disabled_and_saml_is_not_implemented() {
	let base = spawn(false).await;
	let (client, _) = client(&base);
	let glass = client
		.post(format!("{base}/api/auth/break-glass"))
		.form(&[("username", "break-glass"), ("password", "nope")])
		.send()
		.await
		.expect("glass");
	assert_eq!(glass.status(), StatusCode::OK);
	let html = glass.text().await.expect("html");
	assert!(html.contains("disabled"));
	let saml = client
		.post(format!("{base}/api/auth/saml"))
		.send()
		.await
		.expect("saml");
	assert_eq!(saml.status(), StatusCode::NOT_IMPLEMENTED);
}

#[tokio::test]
async fn backchannel_logout_ends_the_session() {
	let base = spawn(false).await;
	let (client, _) = client(&base);
	login(&client, &base, "alex").await;
	let claims = IdClaims {
		sub: "example-user".into(),
		email: Some("alex.nguyen@example.gov.au".into()),
		name: Some("Alex Nguyen".into()),
		preferred_username: None,
		groups: vec![],
		roles: vec![],
		nonce: None,
		iss: "mock".into(),
		aud: json!("magpie"),
		exp: 4_000_000_000,
		events: json!({ BACK_CHANNEL_EVENT: {} }),
	};
	let token = issue_hs256(&claims, b"test-secret").expect("token");
	let ended = client
		.post(format!("{base}/api/auth/backchannel"))
		.form(&[("logout_token", token)])
		.send()
		.await
		.expect("logout");
	assert_eq!(ended.status(), StatusCode::OK);
	let dashboard = client
		.get(format!("{base}/dashboard"))
		.send()
		.await
		.expect("dashboard");
	assert!(dashboard.status().is_redirection());
	assert!(
		dashboard
			.headers()
			.get(LOCATION)
			.and_then(|value| value.to_str().ok())
			.unwrap_or("")
			.contains("/login")
	);
}

#[tokio::test]
async fn policy_change_waits_for_a_second_person() {
	let base = spawn(false).await;
	let (alex, _) = client(&base);
	login(&alex, &base, "alex").await;
	let saved = alex
		.post(format!("{base}/policies"))
		.form(&[
			("mode", "raw"),
			("raw", "mcp.tool.name == 'example_search'"),
		])
		.send()
		.await
		.expect("save");
	assert_eq!(saved.status(), StatusCode::OK);
	let html = saved.text().await.expect("html");
	assert!(html.contains("pending revision"));
	let id = html
		.split("/revisions/")
		.nth(1)
		.expect("revision")
		.split('/')
		.next()
		.expect("id");
	let self_approval = alex
		.post(format!("{base}/revisions/{id}/approve"))
		.send()
		.await
		.expect("self");
	assert_eq!(self_approval.status(), StatusCode::FORBIDDEN);

	let (sam, _) = client(&base);
	login(&sam, &base, "sam").await;
	let approved = sam
		.post(format!("{base}/revisions/{id}/approve"))
		.send()
		.await
		.expect("approve");
	assert!(approved.status().is_redirection());
	let policies = sam
		.get(format!("{base}/policies"))
		.send()
		.await
		.expect("policies");
	let html = policies.text().await.expect("html");
	assert!(html.contains("No change is waiting"));
}

#[tokio::test]
async fn kill_switch_blocks_the_next_call() {
	let base = spawn(false).await;
	let (alex, _) = client(&base);
	login(&alex, &base, "alex").await;
	let approve = alex
		.post(format!("{base}/approvals/approval-example/approve"))
		.send()
		.await
		.expect("approve");
	assert!(approve.status().is_redirection());

	let (sam, _) = client(&base);
	login(&sam, &base, "sam").await;
	let saved = sam
		.post(format!("{base}/settings"))
		.form(&[
			("sovereign", "true"),
			("telemetry", "false"),
			("presidio", ""),
			("retention", "365"),
			("kill_global", "true"),
			("kill_agent", ""),
		])
		.send()
		.await
		.expect("settings");
	assert!(saved.status().is_redirection());

	let blocked = alex
		.post(format!("{base}/mcp/check"))
		.json(&mcp(
			Some("example_search"),
			"example-au-model",
			"ap-southeast-2",
			"example-au-model",
			"hello",
		))
		.send()
		.await
		.expect("check");
	let blocked: serde_json::Value = blocked.json().await.expect("json");
	assert_eq!(blocked["decision"], "reject");
	assert!(
		blocked["reason"]
			.as_str()
			.unwrap_or("")
			.contains("kill switch")
	);
}

#[tokio::test]
async fn demo_pages_are_labelled_as_example_data() {
	let mut demo = config(BreakGlassConfig::default(), false);
	demo.demo = true;
	let base = spawn_with(demo).await;
	let (client, _) = client(&base);
	for path in [
		"/dashboard",
		"/agents",
		"/catalogue",
		"/providers",
		"/policies",
		"/dlp",
		"/audit",
		"/approvals",
		"/settings",
	] {
		let response = client
			.get(format!("{base}{path}"))
			.send()
			.await
			.expect(path);
		assert_eq!(response.status(), StatusCode::OK, "{path}");
		let html = response.text().await.expect("html");
		assert!(html.contains("Example data"), "{path}");
		assert!(html.contains("<h1>"), "{path}");
	}
	let login_page = client
		.get(format!("{base}/login"))
		.send()
		.await
		.expect("login");
	assert!(login_page.text().await.expect("html").contains("Sign in"));
}

#[tokio::test]
async fn postgres_url_is_refused() {
	let mut config = config(BreakGlassConfig::default(), false);
	config.database_path = "postgres://example.invalid/magpie".into();
	let error = match magpie_portal::state(config) {
		Ok(_) => panic!("postgres URL was accepted"),
		Err(error) => error,
	};
	assert!(error.to_string().contains("SQLite"));
}

#[tokio::test]
async fn enabled_break_glass_login_is_audited() {
	let hash = hash_password("example-password").expect("hash");
	let glass = BreakGlassConfig {
		enabled: true,
		password_hash: hash,
		..BreakGlassConfig::default()
	};
	let base = spawn_with(config(glass, false)).await;
	let (client, _) = client(&base);
	let response = client
		.post(format!("{base}/api/auth/break-glass"))
		.form(&[
			("username", "break-glass"),
			("password", "example-password"),
		])
		.send()
		.await
		.expect("login");
	// 303, so the browser follows with GET. A 307 made it POST to /dashboard.
	assert_eq!(response.status(), StatusCode::SEE_OTHER);
	let dashboard = client
		.get(format!("{base}/dashboard"))
		.send()
		.await
		.expect("dashboard");
	let html = dashboard.text().await.expect("html");
	assert!(html.contains("Break-glass session"));
	let saved = client
		.post(format!("{base}/settings"))
		.form(&[
			("sovereign", "true"),
			("telemetry", "false"),
			("presidio", ""),
			("retention", "365"),
			("kill_global", "false"),
			("kill_agent", "example-agent"),
		])
		.send()
		.await
		.expect("settings");
	assert_eq!(saved.status(), StatusCode::SEE_OTHER);
	let cleared = client
		.post(format!("{base}/settings"))
		.form(&[
			("sovereign", "true"),
			("telemetry", "false"),
			("presidio", ""),
			("retention", "365"),
			("kill_global", "false"),
			("kill_agent", ""),
		])
		.send()
		.await
		.expect("settings");
	assert_eq!(cleared.status(), StatusCode::SEE_OTHER);
	let html = client
		.get(format!("{base}/settings"))
		.send()
		.await
		.expect("settings page")
		.text()
		.await
		.expect("html");
	assert!(
		!html.contains("value=\"example-agent\""),
		"kill switch not cleared"
	);
}

#[tokio::test]
async fn log_only_webhooks_pass_unchanged_audit_findings_and_keep_authentication() {
	let mut config = config(BreakGlassConfig::default(), false);
	config.guardrail_mode = magpie::guardrail::Mode::LogOnly;
	config.webhook_token = "test-webhook-secret".into();
	let base = spawn_with(config).await;
	let (client, _) = client(&base);
	let text = "Example only. ABN 12 000 000 041.";
	let requests = [
		(
			"/guardrails/request",
			json!({"body": {"messages": [{"role": "user", "content": text}]}}),
		),
		(
			"/guardrails/response",
			json!({"body": {"choices": [{"message": {"role": "assistant", "content": text}}]}}),
		),
		(
			"/mcp/check",
			mcp(
				Some("unapproved"),
				"example-au-model",
				"ap-southeast-2",
				"example-au-model",
				text,
			),
		),
	];
	for (path, payload) in requests {
		let denied = client
			.post(format!("{base}{path}"))
			.json(&payload)
			.send()
			.await
			.unwrap();
		assert_eq!(denied.status(), StatusCode::UNAUTHORIZED);
		let reply = client
			.post(format!("{base}{path}"))
			.header("x-magpie-webhook-token", "test-webhook-secret")
			.header("x-magpie-region", "ap-southeast-2")
			.header("x-magpie-destination", "example-au-model")
			.header("x-magpie-model", "example-au-model")
			.json(&payload)
			.send()
			.await
			.unwrap();
		assert_eq!(reply.status(), StatusCode::OK);
		let reply: serde_json::Value = reply.json().await.unwrap();
		if path == "/mcp/check" {
			assert_eq!(reply["decision"], "pass");
			assert_eq!(reply["text"], text);
		} else {
			assert!(reply["action"].get("body").is_none());
			assert!(reply["action"].get("status_code").is_none());
			assert!(
				reply["action"]["reason"]
					.as_str()
					.unwrap()
					.contains("would redact")
			);
		}
	}
	login(&client, &base, "alex").await;
	let settings = client
		.get(format!("{base}/settings"))
		.send()
		.await
		.unwrap()
		.text()
		.await
		.unwrap();
	assert!(settings.contains("Log-only trial"));
	assert!(settings.contains("<strong>log-only</strong>"));
	let csv = client
		.get(format!("{base}/audit.csv"))
		.send()
		.await
		.unwrap()
		.text()
		.await
		.unwrap();
	assert!(csv.contains("Log-only: would redact"));
	assert!(csv.contains("Log-only: would block"));
	assert!(csv.contains("wouldAction"));
	assert!(!csv.contains("12 000 000 041"));
}

#[tokio::test]
async fn administrator_can_switch_live_mode_but_policy_editors_cannot() {
	let base = spawn(false).await;
	let (admin, _) = client(&base);
	let (editor, _) = client(&base);
	let anonymous = admin
		.post(format!("{base}/settings/guardrail-mode"))
		.form(&[("mode", "log-only")])
		.send()
		.await
		.unwrap();
	assert!(anonymous.status().is_redirection());
	login(&editor, &base, "alex").await;
	let denied = editor
		.post(format!("{base}/settings/guardrail-mode"))
		.form(&[("mode", "log-only")])
		.send()
		.await
		.unwrap();
	assert_eq!(denied.status(), StatusCode::FORBIDDEN);
	login(&admin, &base, "sam").await;
	let invalid = admin
		.post(format!("{base}/settings/guardrail-mode"))
		.form(&[("mode", "disabled")])
		.send()
		.await
		.unwrap();
	assert_eq!(invalid.status(), StatusCode::BAD_REQUEST);
	let current: serde_json::Value = admin
		.get(format!("{base}/settings/guardrail-mode"))
		.send()
		.await
		.unwrap()
		.json()
		.await
		.unwrap();
	assert_eq!(current["mode"], "enforce");
	for mode in ["log-only", "enforce", "log-only"] {
		let saved = admin
			.post(format!("{base}/settings/guardrail-mode"))
			.form(&[("mode", mode)])
			.send()
			.await
			.unwrap();
		assert_eq!(saved.status(), StatusCode::SEE_OTHER);
		let html = admin
			.get(format!("{base}/settings"))
			.send()
			.await
			.unwrap()
			.text()
			.await
			.unwrap();
		assert!(html.contains(&format!("<strong>{mode}</strong>")));
		assert_eq!(html.contains("Log-only trial"), mode == "log-only");
		for phase in ["request", "response"] {
			let message = json!({"role": "user", "content": "ABN 12 000 000 041"});
			let body = if phase == "request" {
				json!({"body": {"messages": [message]}})
			} else {
				json!({"body": {"choices": [{"message": message}]}})
			};
			let response: serde_json::Value = admin
				.post(format!("{base}/guardrails/{phase}"))
				.header("x-magpie-region", "ap-southeast-2")
				.header("x-magpie-destination", "example-au-model")
				.header("x-magpie-model", "example-au-model")
				.json(&body)
				.send()
				.await
				.unwrap()
				.json()
				.await
				.unwrap();
			assert_eq!(response["action"].get("body").is_some(), mode == "enforce");
		}
		let response: serde_json::Value = admin
			.post(format!("{base}/mcp/check"))
			.json(&mcp(
				Some("unapproved"),
				"example-au-model",
				"ap-southeast-2",
				"example-au-model",
				"hello",
			))
			.send()
			.await
			.unwrap()
			.json()
			.await
			.unwrap();
		assert_eq!(
			response["decision"],
			if mode == "log-only" { "pass" } else { "reject" }
		);
	}
	let audit = admin
		.get(format!("{base}/audit.csv"))
		.send()
		.await
		.unwrap()
		.text()
		.await
		.unwrap();
	assert!(audit.contains("Guardrail mode changed from enforce to log-only."));
	assert!(audit.contains("Guardrail mode changed from log-only to enforce."));
	assert!(audit.contains("sam.patel@example.gov.au"));
	// General settings still use their approval flow and cannot replace the runtime choice.
	let saved = admin
		.post(format!("{base}/settings"))
		.form(&[
			("sovereign", "true"),
			("telemetry", "false"),
			("presidio", ""),
			("retention", "365"),
		])
		.send()
		.await
		.unwrap();
	assert_eq!(saved.status(), StatusCode::SEE_OTHER);
	let current: serde_json::Value = admin
		.get(format!("{base}/settings/guardrail-mode"))
		.send()
		.await
		.unwrap()
		.json()
		.await
		.unwrap();
	assert_eq!(current["mode"], "log-only");
}

#[test]
fn runtime_mode_survives_restart_and_a_changed_deployment_default() {
	let path = std::env::temp_dir().join(format!("magpie-mode-{}.sqlite", magpie::new_id()));
	let mut initial = config(BreakGlassConfig::default(), false);
	initial.database_path = path.to_str().unwrap().into();
	initial.guardrail_mode = magpie::guardrail::Mode::LogOnly;
	drop(magpie_portal::state(initial).unwrap());
	{
		let store = magpie::store::Store::open_path(path.to_str().unwrap()).unwrap();
		assert_eq!(
			store.guardrail_mode().unwrap(),
			Some(magpie::guardrail::Mode::LogOnly)
		);
		store
			.set_guardrail_mode(magpie::guardrail::Mode::Enforce, "administrator")
			.unwrap();
		let mut access = store.access_control().unwrap().unwrap();
		access.groups.push(magpie::rbac::UserGroup {
			id: "persisted".into(),
			name: "Persisted".into(),
			roles: vec!["viewer".into()],
			members: vec!["saved-user".into()],
			identity_groups: vec![],
		});
		store.set_access_control(&access, "administrator").unwrap();
	}
	let mut restarted = config(BreakGlassConfig::default(), false);
	restarted.database_path = path.to_str().unwrap().into();
	restarted.guardrail_mode = magpie::guardrail::Mode::LogOnly;
	drop(magpie_portal::state(restarted).unwrap());
	{
		let store = magpie::store::Store::open_path(path.to_str().unwrap()).unwrap();
		assert_eq!(
			store.guardrail_mode().unwrap(),
			Some(magpie::guardrail::Mode::Enforce)
		);
	}
	{
		let store = magpie::store::Store::open_path(path.to_str().unwrap()).unwrap();
		assert!(
			store
				.access_control()
				.unwrap()
				.unwrap()
				.groups
				.iter()
				.any(|group| group.id == "persisted")
		);
	}
	std::fs::remove_file(path).unwrap();
}

#[tokio::test]
async fn custom_group_roles_grant_individual_actions_and_revoke_active_sessions() {
	let base = spawn(false).await;
	let (admin, _) = client(&base);
	let (user, _) = client(&base);
	login(&admin, &base, "sam").await;
	login(&user, &base, "alex").await;
	let set_mode = || {
		user
			.post(format!("{base}/settings/guardrail-mode"))
			.form(&[("mode", "log-only")])
	};
	assert_eq!(
		set_mode().send().await.unwrap().status(),
		StatusCode::FORBIDDEN
	);
	assert_eq!(
		user
			.get(format!("{base}/access"))
			.send()
			.await
			.unwrap()
			.status(),
		StatusCode::FORBIDDEN
	);
	assert_eq!(
		user
			.post(format!("{base}/access/roles"))
			.form(&[("id", "owner")])
			.send()
			.await
			.unwrap()
			.status(),
		StatusCode::FORBIDDEN
	);
	let role = [
		("id", "trial-operator"),
		("name", "Trial operator"),
		("permission.readSettings", "1"),
		("permission.changeEnforcement", "1"),
	];
	assert_eq!(
		admin
			.post(format!("{base}/access/roles"))
			.form(&role)
			.send()
			.await
			.unwrap()
			.status(),
		StatusCode::SEE_OTHER
	);
	let group = [
		("id", "trial-team"),
		("name", "Trial team"),
		("role.trial-operator", "1"),
		("member.example-user", "1"),
	];
	assert_eq!(
		admin
			.post(format!("{base}/access/groups"))
			.form(&group)
			.send()
			.await
			.unwrap()
			.status(),
		StatusCode::SEE_OTHER
	);
	// No new login: a new group grants only the selected action immediately.
	assert_eq!(
		set_mode().send().await.unwrap().status(),
		StatusCode::SEE_OTHER
	);
	assert_eq!(
		user
			.post(format!("{base}/settings"))
			.form(&[
				("sovereign", "true"),
				("telemetry", "false"),
				("presidio", ""),
				("retention", "365")
			])
			.send()
			.await
			.unwrap()
			.status(),
		StatusCode::FORBIDDEN
	);
	// A referenced role cannot disappear silently.
	assert_eq!(
		admin
			.post(format!("{base}/access/roles"))
			.form(&[("id", "trial-operator"), ("operation", "delete")])
			.send()
			.await
			.unwrap()
			.status(),
		StatusCode::BAD_REQUEST
	);
	// Editing the role revokes access from the existing session.
	assert_eq!(
		admin
			.post(format!("{base}/access/roles"))
			.form(&[
				("id", "trial-operator"),
				("name", "Trial operator"),
				("permission.readSettings", "1")
			])
			.send()
			.await
			.unwrap()
			.status(),
		StatusCode::SEE_OTHER
	);
	assert_eq!(
		set_mode().send().await.unwrap().status(),
		StatusCode::FORBIDDEN
	);
	// Identity-provider groups can grant the same role without manual membership.
	admin
		.post(format!("{base}/access/roles"))
		.form(&role)
		.send()
		.await
		.unwrap();
	assert_eq!(
		admin
			.post(format!("{base}/access/groups"))
			.form(&[
				("id", "trial-team"),
				("name", "Trial team"),
				("role.trial-operator", "1"),
				("identity_groups", "magpie-policy-editors")
			])
			.send()
			.await
			.unwrap()
			.status(),
		StatusCode::SEE_OTHER
	);
	assert_eq!(
		set_mode().send().await.unwrap().status(),
		StatusCode::SEE_OTHER
	);
	// Deleting the group revokes that grant, then its role can be removed.
	assert_eq!(
		admin
			.post(format!("{base}/access/groups"))
			.form(&[("id", "trial-team"), ("operation", "delete")])
			.send()
			.await
			.unwrap()
			.status(),
		StatusCode::SEE_OTHER
	);
	assert_eq!(
		set_mode().send().await.unwrap().status(),
		StatusCode::FORBIDDEN
	);
	assert_eq!(
		admin
			.post(format!("{base}/access/roles"))
			.form(&[("id", "trial-operator"), ("operation", "delete")])
			.send()
			.await
			.unwrap()
			.status(),
		StatusCode::SEE_OTHER
	);
	// Protect recovery and the current access administrator from self-lockout.
	assert_eq!(
		admin
			.post(format!("{base}/access/roles"))
			.form(&[("id", "owner"), ("operation", "delete")])
			.send()
			.await
			.unwrap()
			.status(),
		StatusCode::BAD_REQUEST
	);
	assert_eq!(
		admin
			.post(format!("{base}/access/roles"))
			.form(&[("id", "admin"), ("name", "Admin")])
			.send()
			.await
			.unwrap()
			.status(),
		StatusCode::BAD_REQUEST
	);
	assert_eq!(
		admin
			.post(format!("{base}/access/users"))
			.form(&[("subject", "example-admin"), ("disabled", "true")])
			.send()
			.await
			.unwrap()
			.status(),
		StatusCode::BAD_REQUEST
	);
	assert_eq!(
		admin
			.post(format!("{base}/access/users"))
			.form(&[("subject", "example-user"), ("disabled", "true")])
			.send()
			.await
			.unwrap()
			.status(),
		StatusCode::SEE_OTHER
	);
	assert_eq!(
		user
			.get(format!("{base}/dashboard"))
			.send()
			.await
			.unwrap()
			.status(),
		StatusCode::FORBIDDEN
	);
	let denied_login = login_response(&user, &base, "alex").await;
	assert_eq!(denied_login.status(), StatusCode::OK);
	assert!(denied_login.headers().get(SET_COOKIE).is_none());
	assert!(
		denied_login
			.text()
			.await
			.unwrap()
			.contains("portal account is disabled")
	);
	admin
		.post(format!("{base}/access/users"))
		.form(&[("subject", "example-user"), ("disabled", "false")])
		.send()
		.await
		.unwrap();
	assert!(
		user
			.get(format!("{base}/dashboard"))
			.send()
			.await
			.unwrap()
			.status()
			.is_redirection()
	);
	login(&user, &base, "alex").await;
	assert_eq!(
		user
			.get(format!("{base}/dashboard"))
			.send()
			.await
			.unwrap()
			.status(),
		StatusCode::OK
	);
	let audit = admin
		.get(format!("{base}/audit.csv"))
		.send()
		.await
		.unwrap()
		.text()
		.await
		.unwrap();
	assert!(audit.contains("Roles or user groups updated."));
	assert!(audit.contains("Portal user disabled."));
}

#[tokio::test]
async fn unassigned_users_are_registered_but_have_no_default_access() {
	let mut cfg = config(BreakGlassConfig::default(), false);
	cfg.role_map.policy_editor_groups.clear();
	cfg.role_map.approver_groups.clear();
	cfg.role_map.auditor_groups.clear();
	let base = spawn_with(cfg).await;
	let (user, _) = client(&base);
	login(&user, &base, "alex").await;
	for path in [
		"/dashboard",
		"/catalogue",
		"/policies",
		"/settings",
		"/access",
		"/audit",
		"/api/export",
	] {
		assert_eq!(
			user
				.get(format!("{base}{path}"))
				.send()
				.await
				.unwrap()
				.status(),
			StatusCode::FORBIDDEN,
			"{path}"
		);
	}
	let (admin, _) = client(&base);
	login(&admin, &base, "sam").await;
	let access = admin
		.get(format!("{base}/access"))
		.send()
		.await
		.unwrap()
		.text()
		.await
		.unwrap();
	assert!(access.contains("alex.nguyen@example.gov.au"));
}
