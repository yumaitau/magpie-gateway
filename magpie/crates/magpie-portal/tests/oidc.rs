//! Single sign-on against a stand-in OIDC provider that signs RS256 ID tokens.
//!
//! The provider checks the PKCE verifier and the client secret the way Entra
//! ID does for a confidential web client. Names are synthetic.

use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use axum::extract::{Form, State};
use axum::http::StatusCode;
use axum::routing::{get, post};
use axum::{Json, Router};
use base64::Engine;
use base64::engine::general_purpose::URL_SAFE_NO_PAD;
use magpie::identity::{BreakGlassConfig, OidcProviderKind, RoleMap};
use magpie_portal::Config;
use reqwest::header::LOCATION;
use reqwest::{Client, Url};
use serde_json::{Value, json};
use sha2::{Digest, Sha256};

const KEY_PEM: &str = include_str!("../../magpie/tests/fixtures/oidc-test-key.pem");
const KEY_N: &str = include_str!("../../magpie/tests/fixtures/oidc-test-key.n");

#[derive(Clone, Default)]
struct Idp {
	base: Arc<Mutex<String>>,
	codes: Arc<Mutex<HashMap<String, (String, String)>>>,
}

async fn discovery(State(idp): State<Idp>) -> Json<Value> {
	let base = idp.base.lock().unwrap().clone();
	Json(json!({
		"issuer": base,
		"authorization_endpoint": format!("{base}/authorize"),
		"token_endpoint": format!("{base}/token"),
		"jwks_uri": format!("{base}/keys"),
	}))
}

async fn keys() -> Json<Value> {
	Json(json!({"keys": [{"kty": "RSA", "kid": "k1", "use": "sig", "n": KEY_N.trim(), "e": "AQAB"}]}))
}

async fn token(
	State(idp): State<Idp>,
	Form(form): Form<HashMap<String, String>>,
) -> Result<Json<Value>, (StatusCode, Json<Value>)> {
	let refuse = |error: &str| (StatusCode::BAD_REQUEST, Json(json!({"error": error})));
	if form.get("client_secret").map(String::as_str) != Some("example-client-secret") {
		return Err(refuse("invalid_client"));
	}
	let code = form.get("code").cloned().unwrap_or_default();
	let Some((challenge, nonce)) = idp.codes.lock().unwrap().remove(&code) else {
		return Err(refuse("invalid_grant"));
	};
	let verifier = form.get("code_verifier").cloned().unwrap_or_default();
	if URL_SAFE_NO_PAD.encode(Sha256::digest(verifier.as_bytes())) != challenge {
		return Err(refuse("invalid_grant"));
	}
	let base = idp.base.lock().unwrap().clone();
	let mut header = jsonwebtoken::Header::new(jsonwebtoken::Algorithm::RS256);
	header.kid = Some("k1".into());
	let claims = json!({
		"sub": "entra-object-1",
		"name": "Jordan Lee",
		"preferred_username": "jordan.lee@example.com.au",
		"roles": ["Magpie.Owner"],
		"nonce": nonce,
		"iss": base,
		"aud": "client-1",
		"exp": chrono_like_exp(),
	});
	let id_token = jsonwebtoken::encode(
		&header,
		&claims,
		&jsonwebtoken::EncodingKey::from_rsa_pem(KEY_PEM.as_bytes()).unwrap(),
	)
	.unwrap();
	Ok(Json(json!({"id_token": id_token, "token_type": "Bearer"})))
}

fn chrono_like_exp() -> u64 {
	std::time::SystemTime::now()
		.duration_since(std::time::UNIX_EPOCH)
		.unwrap()
		.as_secs()
		+ 300
}

async fn serve(app: Router) -> String {
	let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
	let address = listener.local_addr().unwrap();
	tokio::spawn(async move { axum::serve(listener, app).await.unwrap() });
	format!("http://{address}")
}

async fn spawn_idp() -> (String, Idp) {
	let idp = Idp::default();
	let app = Router::new()
		.route("/.well-known/openid-configuration", get(discovery))
		.route("/keys", get(keys))
		.route("/token", post(token))
		.with_state(idp.clone());
	let base = serve(app).await;
	*idp.base.lock().unwrap() = base.clone();
	(base, idp)
}

fn portal_config(issuer: String, webhook_token: &str) -> Config {
	let mut role_map = RoleMap::default();
	role_map.extend_from(|name| (name == "MAGPIE_ROLE_OWNERS").then(|| "Magpie.Owner".into()));
	Config {
		database_path: ":memory:".into(),
		cookie_secure: false,
		demo: false,
		seed_examples: false,
		mock_idp: false,
		oidc_kind: OidcProviderKind::Generic,
		issuer,
		client_id: "client-1".into(),
		client_secret: "example-client-secret".into(),
		audience: "client-1".into(),
		public_url: String::new(),
		role_map,
		webhook_token: webhook_token.into(),
		bootstrap_models: vec![("claude-sonnet-5-5".into(), "ap-southeast-2".into())],
		bootstrap_owner: "jordan.lee@example.com.au".into(),
		hmac_secret: "test-secret".into(),
		session_secret: "test-secret".into(),
		scim_token: String::new(),
		break_glass: BreakGlassConfig::default(),
		idle: Duration::from_secs(1800),
		absolute: Duration::from_secs(43200),
		listen: "127.0.0.1:0".into(),
	}
}

async fn spawn_portal(config: Config) -> String {
	let state = magpie_portal::state(config).unwrap();
	serve(magpie_portal::router(state)).await
}

fn http() -> Client {
	Client::builder()
		.cookie_store(true)
		.redirect(reqwest::redirect::Policy::none())
		.timeout(Duration::from_secs(5))
		.build()
		.unwrap()
}

#[tokio::test]
async fn oidc_code_flow_with_pkce_signs_in_an_owner() {
	let (issuer, idp) = spawn_idp().await;
	let portal = spawn_portal(portal_config(issuer.clone(), "")).await;
	let client = http();

	let start = client
		.get(format!("{portal}/api/auth/start"))
		.send()
		.await
		.unwrap();
	assert!(start.status().is_redirection(), "{}", start.status());
	let location = Url::parse(start.headers()[LOCATION].to_str().unwrap()).unwrap();
	assert!(
		location
			.as_str()
			.starts_with(&format!("{issuer}/authorize"))
	);
	let query: HashMap<_, _> = location.query_pairs().into_owned().collect();
	assert_eq!(query["redirect_uri"], format!("{portal}/api/auth/callback"));
	assert_eq!(query["code_challenge_method"], "S256");

	idp.codes.lock().unwrap().insert(
		"code-1".into(),
		(query["code_challenge"].clone(), query["nonce"].clone()),
	);
	let callback = client
		.get(format!(
			"{portal}/api/auth/callback?code=code-1&state={}",
			query["state"]
		))
		.send()
		.await
		.unwrap();
	assert!(callback.status().is_redirection(), "{}", callback.status());
	assert_eq!(callback.headers()[LOCATION], "/dashboard");

	let settings = client
		.get(format!("{portal}/settings"))
		.send()
		.await
		.unwrap();
	assert_eq!(settings.status(), reqwest::StatusCode::OK);
	let html = client
		.get(format!("{portal}/dashboard"))
		.send()
		.await
		.unwrap()
		.text()
		.await
		.unwrap();
	assert!(html.contains("jordan.lee@example.com.au"), "{html}");
}

#[tokio::test]
async fn oidc_callback_reports_a_refused_login_and_a_replayed_state() {
	let (issuer, _) = spawn_idp().await;
	let portal = spawn_portal(portal_config(issuer, "")).await;
	let client = http();
	let refused = client
		.get(format!(
			"{portal}/api/auth/callback?error=access_denied&error_description=User+cancelled"
		))
		.send()
		.await
		.unwrap()
		.text()
		.await
		.unwrap();
	assert!(refused.contains("User cancelled"), "{refused}");
	let unknown = client
		.get(format!(
			"{portal}/api/auth/callback?code=x&state=never-issued"
		))
		.send()
		.await
		.unwrap()
		.text()
		.await
		.unwrap();
	assert!(unknown.contains("expired"), "{unknown}");
}

#[tokio::test]
async fn guardrail_webhook_requires_the_configured_token() {
	let portal = spawn_portal(portal_config(
		"http://127.0.0.1:9".into(),
		"example-webhook-token",
	))
	.await;
	let client = http();
	let body = json!({"body": {"messages": [{"role": "user", "content": "hello"}]}});
	let missing = client
		.post(format!("{portal}/guardrails/request"))
		.json(&body)
		.send()
		.await
		.unwrap();
	assert_eq!(missing.status(), reqwest::StatusCode::UNAUTHORIZED);
	let wrong = client
		.post(format!("{portal}/mcp/check"))
		.header("x-magpie-webhook-token", "nope")
		.json(&json!({}))
		.send()
		.await
		.unwrap();
	assert_eq!(wrong.status(), reqwest::StatusCode::UNAUTHORIZED);
	let ok = client
		.post(format!("{portal}/guardrails/request"))
		.header("x-magpie-webhook-token", "example-webhook-token")
		.json(&body)
		.send()
		.await
		.unwrap();
	assert_eq!(ok.status(), reqwest::StatusCode::OK);
}

#[tokio::test]
async fn deployment_models_are_approved_on_first_start() {
	let portal = spawn_portal(portal_config("http://127.0.0.1:9".into(), "")).await;
	let client = http();
	let ask = |model: &'static str| {
		client
			.post(format!("{portal}/guardrails/request"))
			.header("x-magpie-model", model)
			.header("x-magpie-region", "ap-southeast-2")
			.header("x-magpie-destination", model)
			.header("x-magpie-actor", "jordan")
			.json(&json!({"body": {"messages": [{"role": "user", "content": "Write a unit test."}]}}))
			.send()
	};
	let approved: Value = ask("claude-sonnet-5-5")
		.await
		.unwrap()
		.json()
		.await
		.unwrap();
	assert!(
		approved["action"].get("status_code").is_none(),
		"{approved}"
	);
	let other: Value = ask("claude-fable-5-1").await.unwrap().json().await.unwrap();
	assert_eq!(other["action"]["status_code"], 403, "{other}");
}
