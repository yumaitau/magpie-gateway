//! Admin portal, SCIM and the guardrail webhook.
//!
//! Telemetry, Presidio and audit sinks stay off unless settings name an endpoint.

mod access;
mod accounts;
pub mod alerts;
mod australia;
mod controls;
mod native;
mod ui;

use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use axum::extract::{Form, Path, Query, State};
use axum::http::{HeaderMap, StatusCode, header};
use axum::response::{Html, IntoResponse, Redirect, Response};
use axum::routing::{get, post};
use axum::{Json, Router};
use chrono::Utc;
use magpie::approvals::{Approval, ApprovalStatus, ItemKind};
use magpie::audit::{AuditEvent, AuditQuery};
use magpie::dlp::{self, DlpRule};
use magpie::guardrail::{
	self, Inspect, McpCheckRequest, Phase, PolicySet, PromptWebhook, ResponseWebhook, WebhookMessage,
};
use magpie::identity::{
	self, BreakGlassConfig, IdClaims, OidcProviderKind, Permission, RoleMap, SessionCodec,
	SessionRecord, issuer_for, roles_from_groups, start_pkce, verify_hs256,
};
use magpie::presidio::{self, PresidioEntity};
use magpie::register::{self, UseCase};
use magpie::safety::{ArgumentRule, KillSwitch, RateLimiter, ToolLimit};
use magpie::scim::{self, Directory};
use magpie::store::{Document, Store};
use serde::Deserialize;
use serde_json::{Value, json};

pub struct Config {
	/// Private, trusted Agentgateway admin endpoint. Empty disables the native console.
	pub gateway_admin_url: String,
	pub gateway_ui_dir: String,
	/// Initial mode only; the portal persists subsequent administrator choices.
	pub guardrail_mode: guardrail::Mode,
	pub database_path: String,
	pub cookie_secure: bool,
	pub demo: bool,
	pub seed_examples: bool,
	pub mock_idp: bool,
	pub oidc_kind: OidcProviderKind,
	/// Empty with an empty `client_id` turns single sign-on off.
	pub issuer: String,
	pub client_id: String,
	/// Confidential clients send this to the token endpoint. Empty for PKCE-only public clients.
	pub client_secret: String,
	pub audience: String,
	/// External base URL, for example `https://magpie.example.com.au`. Empty means use the Host header.
	pub public_url: String,
	pub role_map: RoleMap,
	/// When set, the proxy must send it as `x-magpie-webhook-token` on guardrail and MCP check calls.
	pub webhook_token: String,
	/// Models the deployment routes to, as (name, region). Each gets a provider
	/// record and an approval on first start, so day-one traffic is not denied.
	pub bootstrap_models: Vec<(String, String)>,
	/// Destinations allowed for PROTECTED and above. The settings form does not
	/// edit this list, so the deployment supplies it.
	pub bootstrap_protected: Vec<String>,
	/// Business owner recorded on those approvals.
	pub bootstrap_owner: String,
	pub hmac_secret: String,
	pub session_secret: String,
	pub scim_token: String,
	pub break_glass: BreakGlassConfig,
	pub idle: Duration,
	pub absolute: Duration,
	pub listen: String,
}

struct PendingLogin {
	challenge: String,
	verifier: String,
	nonce: String,
}

#[derive(Clone, Deserialize)]
struct Discovery {
	issuer: String,
	authorization_endpoint: String,
	token_endpoint: String,
	jwks_uri: String,
}

#[derive(Clone)]
struct Provider {
	discovery: Discovery,
	jwks: identity::JwkSet,
}

struct IssuedCode {
	challenge: String,
	claims: IdClaims,
}

struct Inner {
	login_dummy_hash: String,
	gateway_client: reqwest::Client,
	gateway_write: tokio::sync::Mutex<()>,
	store: Mutex<Store>,
	codec: SessionCodec,
	pending: Mutex<HashMap<String, PendingLogin>>,
	codes: Mutex<HashMap<String, IssuedCode>>,
	provider: Mutex<Option<Provider>>,
	directory: Mutex<Directory>,
	limiter: RateLimiter,
	config: Config,
}

#[derive(Clone)]
pub struct AppState {
	inner: Arc<Inner>,
}

pub fn state(config: Config) -> magpie::Result<AppState> {
	if config.database_path.starts_with("postgres://")
		|| config.database_path.starts_with("postgresql://")
	{
		return Err(magpie::Error::Msg(
			magpie::store::postgres_placeholder_note().into(),
		));
	}
	let store = if config.database_path == ":memory:" {
		Store::open_memory()?
	} else {
		Store::open_path(&config.database_path)?
	};
	if config.seed_examples {
		seed(&store)?;
	}
	bootstrap(
		&store,
		&config.bootstrap_models,
		&config.bootstrap_owner,
		&config.bootstrap_protected,
	)?;
	if store.access_control()?.is_none() {
		store.set_access_control(
			&magpie::rbac::AccessControl::defaults(&config.role_map),
			"deployment",
		)?;
	}
	if store.guardrail_mode()?.is_none() {
		store.set_guardrail_mode(config.guardrail_mode, "deployment")?;
	}
	// New releases can add actions; the recovery Owner retains every action.
	if let Some(mut access) = store.access_control()?
		&& let Some(owner) = access.roles.iter_mut().find(|r| r.id == "owner")
	{
		let mut changed = false;
		for (permission, _, _) in magpie::rbac::ACTIONS {
			if !owner.permissions.contains(permission) {
				owner.permissions.push(*permission);
				changed = true;
			}
		}
		if changed {
			store.set_access_control(&access, "upgrade")?;
		}
	}
	accounts::bootstrap(&store, &config)?;
	let login_dummy_hash = identity::hash_password(&identity::start_pkce().verifier)?;
	let codec = SessionCodec::new(config.session_secret.clone(), config.idle, config.absolute);
	Ok(AppState {
		inner: Arc::new(Inner {
			login_dummy_hash,
			gateway_client: reqwest::Client::builder()
				.redirect(reqwest::redirect::Policy::none())
				.connect_timeout(Duration::from_secs(5))
				.build()
				.expect("gateway client"),
			gateway_write: tokio::sync::Mutex::new(()),
			store: Mutex::new(store),
			codec,
			pending: Mutex::new(HashMap::new()),
			codes: Mutex::new(HashMap::new()),
			provider: Mutex::new(None),
			directory: Mutex::new(Directory::default()),
			limiter: RateLimiter::new(vec![
				ToolLimit {
					tool: "example_search".into(),
					max_calls: 30,
					window_seconds: 60,
				},
				ToolLimit {
					tool: "account-login".into(),
					max_calls: 12,
					window_seconds: 60,
				},
				ToolLimit {
					tool: "account-login-global".into(),
					max_calls: 120,
					window_seconds: 60,
				},
				ToolLimit {
					tool: "account-password".into(),
					max_calls: 10,
					window_seconds: 60,
				},
				ToolLimit {
					tool: "account-accept".into(),
					max_calls: 30,
					window_seconds: 60,
				},
			]),
			config,
		}),
	})
}

pub fn router(state: AppState) -> Router {
	let webhooks = Router::new()
		.route("/guardrails/request", post(guard_request))
		.route("/guardrails/response", post(guard_response))
		.route("/mcp/check", post(mcp_check))
		.route_layer(axum::middleware::from_fn_with_state(
			state.clone(),
			webhook_auth,
		));
	Router::new()
		.merge(webhooks)
		.merge(native::routes())
		.merge(accounts::routes())
		.route("/", get(native::home))
		.route("/healthz", get(health))
		.route("/login", get(login))
		.route("/api/auth/start", get(auth_start))
		.route("/api/auth/callback", get(auth_callback))
		.route("/api/auth/logout", post(logout))
		.route("/api/auth/backchannel", post(backchannel))
		.route("/api/auth/break-glass", post(break_glass_login))
		.route("/api/auth/saml", post(saml))
		.route(
			"/mock-idp/authorize",
			get(mock_authorize).post(mock_continue),
		)
		.route("/mock-idp/token", post(mock_token))
		.route(
			"/mock-idp/.well-known/openid-configuration",
			get(mock_discovery),
		)
		.route("/access", get(access::access_page))
		.route("/access/roles", post(access::role_post))
		.route("/access/groups", post(access::group_post))
		.route("/access/users", post(access::user_post))
		.route("/dashboard", get(dashboard))
		.route("/agents", get(agents_page))
		.route("/catalogue", get(catalogue_page))
		.route("/providers", get(providers_page))
		.route("/policies", get(policies_page).post(policies_post))
		.route("/dlp", get(dlp_page).post(dlp_post))
		.route("/dlp/rules", post(dlp_rule_post))
		.route("/australia", get(australia::overview))
		.route("/controls", get(controls::overview))
		.route("/alerts", get(alerts::page_get).post(alerts::post))
		.route("/audit", get(audit_page))
		.route("/audit.csv", get(audit_csv))
		.route("/approvals", get(approvals_page).post(approvals_post))
		.route("/approvals/{id}/approve", post(approve))
		.route("/revisions/{id}/approve", post(approve_revision))
		.route("/revisions/{id}/rollback", post(rollback_revision))
		.route("/settings", get(settings_page).post(settings_post))
		.route(
			"/settings/guardrail-mode",
			get(guardrail_mode_get).post(guardrail_mode_post),
		)
		.route("/use-cases.csv", get(use_cases_csv).post(use_cases_import))
		.route("/api/export", get(export_docs))
		.route("/api/import", post(import_docs))
		.route("/scim/v2/ServiceProviderConfig", get(scim_config))
		.route(
			"/scim/v2/Users",
			get(scim_list_users).post(scim_create_user),
		)
		.route(
			"/scim/v2/Users/{id}",
			get(scim_get_user)
				.patch(scim_patch_user)
				.delete(scim_delete_user),
		)
		.route(
			"/scim/v2/Groups",
			get(scim_list_groups).post(scim_create_group),
		)
		.route(
			"/scim/v2/Groups/{id}",
			get(scim_get_group)
				.patch(scim_patch_group)
				.delete(scim_delete_group),
		)
		.layer(axum::middleware::from_fn_with_state(
			state.clone(),
			browser_security,
		))
		.with_state(state)
}

async fn browser_security(
	State(state): State<AppState>,
	request: axum::extract::Request,
	next: axum::middleware::Next,
) -> Response {
	let unsafe_method = !matches!(
		*request.method(),
		axum::http::Method::GET | axum::http::Method::HEAD | axum::http::Method::OPTIONS
	);
	if unsafe_method
		&& (read_cookie(request.headers(), "magpie_session").is_some()
			|| request.uri().path() == "/api/auth/break-glass")
		&& !accounts::same_origin(&state, request.headers())
	{
		return (
			StatusCode::FORBIDDEN,
			"Cross-origin account and governance changes are refused.",
		)
			.into_response();
	}
	let mut response = next.run(request).await;
	// Keep the Origin header on same-origin form posts for CSRF validation,
	// while suppressing referrers when navigating to another origin.
	response
		.headers_mut()
		.insert(header::REFERRER_POLICY, "same-origin".parse().unwrap());
	response
		.headers_mut()
		.insert(header::X_CONTENT_TYPE_OPTIONS, "nosniff".parse().unwrap());
	response
}

async fn health() -> &'static str {
	"ok"
}

impl Config {
	pub fn oidc_enabled(&self) -> bool {
		self.mock_idp || (!self.issuer.is_empty() && !self.client_id.is_empty())
	}
}

fn login_error(state: &AppState, error: &str) -> Response {
	Html(ui::login_page(
		state.inner.config.oidc_enabled(),
		state.inner.config.break_glass.enabled,
		Some(error),
	))
	.into_response()
}

async fn login(State(state): State<AppState>) -> Response {
	if !state.inner.config.gateway_ui_dir.is_empty() {
		return Redirect::to("/ui/login").into_response();
	}
	Html(ui::login_page(
		state.inner.config.oidc_enabled(),
		state.inner.config.break_glass.enabled,
		None,
	))
	.into_response()
}

/// Discovery and signing keys, fetched once and again when a key is unknown.
async fn load_provider(state: &AppState, refresh: bool) -> Result<Provider, String> {
	if !refresh && let Some(provider) = state.inner.provider.lock().expect("provider").clone() {
		return Ok(provider);
	}
	let config = &state.inner.config;
	let issuer = issuer_for(config.oidc_kind, &config.issuer);
	let client = reqwest::Client::builder()
		.timeout(Duration::from_secs(10))
		.build()
		.map_err(|error| error.to_string())?;
	let discovery: Discovery = client
		.get(format!(
			"{}/.well-known/openid-configuration",
			issuer.trim_end_matches('/')
		))
		.send()
		.await
		.and_then(|response| response.error_for_status())
		.map_err(|error| format!("OIDC discovery failed: {error}"))?
		.json()
		.await
		.map_err(|error| format!("OIDC discovery failed: {error}"))?;
	let jwks = client
		.get(&discovery.jwks_uri)
		.send()
		.await
		.and_then(|response| response.error_for_status())
		.map_err(|error| format!("OIDC signing keys failed: {error}"))?
		.json()
		.await
		.map_err(|error| format!("OIDC signing keys failed: {error}"))?;
	let provider = Provider { discovery, jwks };
	*state.inner.provider.lock().expect("provider") = Some(provider.clone());
	Ok(provider)
}

fn redirect_uri(state: &AppState, headers: &HeaderMap) -> String {
	let base = if state.inner.config.public_url.is_empty() {
		public_base(headers)
	} else {
		state
			.inner
			.config
			.public_url
			.trim_end_matches('/')
			.to_string()
	};
	format!("{base}/api/auth/callback")
}

async fn auth_start(State(state): State<AppState>, headers: HeaderMap) -> Response {
	if !state.inner.config.oidc_enabled() {
		return login_error(&state, "Single sign-on is not set up on this portal.");
	}
	let pkce = start_pkce();
	let redirect = redirect_uri(&state, &headers);
	let endpoint = if state.inner.config.mock_idp {
		format!("{}/mock-idp/authorize", public_base(&headers))
	} else {
		match load_provider(&state, false).await {
			Ok(provider) => provider.discovery.authorization_endpoint,
			Err(error) => return login_error(&state, &error),
		}
	};
	let url = identity::authorisation_url(
		&endpoint,
		&state.inner.config.client_id,
		&redirect,
		&pkce,
		"openid profile email",
	);
	state.inner.pending.lock().expect("pending").insert(
		pkce.state.clone(),
		PendingLogin {
			challenge: pkce.challenge,
			verifier: pkce.verifier,
			nonce: pkce.nonce,
		},
	);
	Redirect::to(&url).into_response()
}

async fn auth_callback(
	State(state): State<AppState>,
	headers: HeaderMap,
	axum::extract::Query(query): axum::extract::Query<CallbackQuery>,
) -> Response {
	if let Some(error) = &query.error {
		let detail = query.error_description.as_deref().unwrap_or(error);
		return login_error(
			&state,
			&format!("The identity provider refused the login: {detail}"),
		);
	}
	let (Some(code), Some(login_state)) = (query.code.as_deref(), query.state.as_deref()) else {
		return login_error(&state, "The identity provider did not return a code.");
	};
	let pending = state
		.inner
		.pending
		.lock()
		.expect("pending")
		.remove(login_state);
	let Some(pending) = pending else {
		return login_error(&state, "This login attempt has expired.");
	};
	if !state.inner.config.mock_idp {
		return match oidc_exchange(&state, &headers, code, &pending).await {
			Ok(claims) => finish_login(&state, &headers, &claims, false, None),
			Err(error) => login_error(&state, &error),
		};
	}
	let issued = state.inner.codes.lock().expect("codes").remove(code);
	let Some(issued) = issued else {
		return login_error(&state, "The identity provider did not return a known code.");
	};
	if issued.challenge != pending.challenge {
		return login_error(&state, "PKCE check failed.");
	}
	let claims = match verify_hs256(
		&issued_token(&state, &issued),
		state.inner.config.hmac_secret.as_bytes(),
		&state.inner.config.audience,
		Some(&pending.nonce),
	) {
		Ok(claims) => claims,
		Err(error) => {
			return login_error(&state, &error.to_string());
		},
	};
	finish_login(&state, &headers, &claims, false, None)
}

#[derive(Deserialize)]
struct TokenResponse {
	id_token: String,
}

/// Authorisation-code exchange with the PKCE verifier, then ID token checks.
async fn oidc_exchange(
	state: &AppState,
	headers: &HeaderMap,
	code: &str,
	pending: &PendingLogin,
) -> Result<IdClaims, String> {
	let config = &state.inner.config;
	let provider = load_provider(state, false).await?;
	let redirect = redirect_uri(state, headers);
	let mut form = vec![
		("grant_type", "authorization_code"),
		("code", code),
		("redirect_uri", redirect.as_str()),
		("client_id", config.client_id.as_str()),
		("code_verifier", pending.verifier.as_str()),
	];
	if !config.client_secret.is_empty() {
		form.push(("client_secret", config.client_secret.as_str()));
	}
	let client = reqwest::Client::builder()
		.timeout(Duration::from_secs(10))
		.build()
		.map_err(|error| error.to_string())?;
	let response = client
		.post(&provider.discovery.token_endpoint)
		.form(&form)
		.send()
		.await
		.map_err(|error| format!("Token exchange failed: {error}"))?;
	if !response.status().is_success() {
		let status = response.status();
		let body = response.text().await.unwrap_or_default();
		let reason = serde_json::from_str::<Value>(&body)
			.ok()
			.and_then(|value| {
				value
					.get("error")
					.and_then(Value::as_str)
					.map(str::to_string)
			})
			.unwrap_or_else(|| status.to_string());
		return Err(format!("Token exchange failed: {reason}"));
	}
	let token: TokenResponse = response
		.json()
		.await
		.map_err(|error| format!("Token exchange failed: {error}"))?;
	let verify = |provider: &Provider| {
		identity::verify_with_jwks(
			&token.id_token,
			&provider.jwks,
			&provider.discovery.issuer,
			&config.client_id,
			Some(&pending.nonce),
		)
	};
	match verify(&provider) {
		Ok(claims) => Ok(claims),
		Err(error) if error.to_string().contains("No identity provider key") => {
			verify(&load_provider(state, true).await?).map_err(|error| error.to_string())
		},
		Err(error) => Err(error.to_string()),
	}
}

fn issued_token(state: &AppState, issued: &IssuedCode) -> String {
	identity::issue_hs256(&issued.claims, state.inner.config.hmac_secret.as_bytes())
		.unwrap_or_default()
}

fn finish_login(
	state: &AppState,
	headers: &HeaderMap,
	claims: &IdClaims,
	break_glass: bool,
	expected_password_hash: Option<&str>,
) -> Response {
	let mut groups = claims.groups.clone();
	groups.extend(claims.roles.clone());
	let roles = roles_from_groups(&groups, &state.inner.config.role_map);
	let now = Utc::now();
	let email = claims
		.email
		.as_deref()
		.or(claims.preferred_username.as_deref())
		.unwrap_or("");
	let (mut record, _) = match state.inner.codec.issue(
		&claims.sub,
		email,
		claims.name.as_deref().unwrap_or(&claims.sub),
		roles,
		now,
		break_glass,
	) {
		Ok(pair) => pair,
		Err(error) => {
			return login_error(state, &error.to_string());
		},
	};
	record.identity_groups = groups;
	let (record, cookie) = match state.inner.codec.touch(record, now) {
		Ok(pair) => pair,
		Err(error) => return login_error(state, &error.to_string()),
	};
	let user = magpie::rbac::PortalUser {
		subject: record.subject.clone(),
		name: record.name.clone(),
		email: record.email.clone(),
		last_login: now.to_rfc3339(),
		disabled: false,
	};
	{
		let store = state.inner.store.lock().expect("db");
		if let Some(expected) = expected_password_hash
			&& store
				.local_account(&record.subject)
				.ok()
				.flatten()
				.is_none_or(|a| a.password_hash != expected)
		{
			return login_error(state, "Password changed. Please sign in again.");
		}

		if store.remember_portal_user(&user).is_err() {
			return login_error(state, "Could not save the signed-in user.");
		}
		if !break_glass
			&& !matches!(store.portal_user(&record.subject), Ok(Some(ref user)) if !user.disabled)
		{
			return login_error(
				state,
				"Your portal account is disabled. Contact an access administrator.",
			);
		}
		if store
			.put_session(
				&record.session_id,
				&record.subject,
				&serde_json::to_string(&record).unwrap_or_default(),
				&record.absolute_expiry.to_rfc3339(),
			)
			.is_err()
		{
			return login_error(state, "Could not create the session.");
		}
	}
	let _ = record_audit(
		state,
		"login",
		&record.email,
		"",
		"",
		"",
		"allow",
		if break_glass {
			"break-glass login"
		} else {
			if claims.iss == "local" {
				"local account login"
			} else {
				"oidc login"
			}
		},
	);
	let mut response = Redirect::to(if state.inner.config.gateway_admin_url.is_empty() {
		"/dashboard"
	} else {
		"/"
	})
	.into_response();
	let _ = headers;
	response.headers_mut().insert(
		header::SET_COOKIE,
		session_cookie(&cookie, state.inner.config.cookie_secure)
			.parse()
			.expect("cookie"),
	);
	response
}

async fn logout(State(state): State<AppState>, headers: HeaderMap) -> Response {
	if let Ok((session, _)) = session_from(&state, &headers) {
		let _ = state
			.inner
			.store
			.lock()
			.expect("db")
			.delete_session(&session.session_id);
		let _ = record_audit(
			&state,
			"logout",
			&session.email,
			"",
			"",
			"",
			"allow",
			"logout",
		);
	}
	let mut response = Redirect::to("/login").into_response();
	response.headers_mut().insert(
		header::SET_COOKIE,
		"magpie_session=; HttpOnly; SameSite=Lax; Path=/; Max-Age=0"
			.parse()
			.expect("cookie"),
	);
	response
}

#[derive(Deserialize)]
struct LogoutForm {
	logout_token: String,
}

async fn backchannel(State(state): State<AppState>, Form(form): Form<LogoutForm>) -> Response {
	let claims = match verify_hs256(
		&form.logout_token,
		state.inner.config.hmac_secret.as_bytes(),
		&state.inner.config.audience,
		None,
	) {
		Ok(claims) => claims,
		Err(error) => return (StatusCode::BAD_REQUEST, error.to_string()).into_response(),
	};
	let subject = match identity::logout_subject(&claims) {
		Ok(subject) => subject,
		Err(error) => return (StatusCode::BAD_REQUEST, error.to_string()).into_response(),
	};
	let removed = state
		.inner
		.store
		.lock()
		.expect("db")
		.delete_sessions_for_subject(&subject)
		.unwrap_or(0);
	let _ = record_audit(
		&state,
		"logout",
		&subject,
		"",
		"",
		"",
		"allow",
		"back-channel logout",
	);
	(StatusCode::OK, format!("ended {removed} session(s)")).into_response()
}

#[derive(Deserialize)]
struct GlassForm {
	username: String,
	password: String,
}

async fn break_glass_login(
	State(state): State<AppState>,
	headers: HeaderMap,
	Form(form): Form<GlassForm>,
) -> Response {
	accounts::local_login(state, headers, form).await
}

async fn saml() -> Response {
	(
		StatusCode::NOT_IMPLEMENTED,
		"SAML 2.0 is not a supported login method. Unsigned assertions are rejected. Use OIDC.",
	)
		.into_response()
}

#[derive(Deserialize)]
struct MockQuery {
	state: String,
}

async fn mock_authorize(
	State(state): State<AppState>,
	headers: HeaderMap,
	axum::extract::Query(query): axum::extract::Query<MockQuery>,
) -> Response {
	if !state.inner.config.mock_idp {
		return StatusCode::NOT_FOUND.into_response();
	}
	let base = public_base(&headers);
	Html(ui::mock_idp_page(
		&query.state,
		&format!("{base}/mock-idp/authorize"),
	))
	.into_response()
}

#[derive(Deserialize)]
struct MockForm {
	state: String,
	who: String,
}

async fn mock_continue(
	State(state): State<AppState>,
	headers: HeaderMap,
	Form(form): Form<MockForm>,
) -> Response {
	if !state.inner.config.mock_idp {
		return StatusCode::NOT_FOUND.into_response();
	}
	let pending = state
		.inner
		.pending
		.lock()
		.expect("pending")
		.get(&form.state)
		.map(|item| item.nonce.clone());
	let Some(nonce) = pending else {
		return (StatusCode::BAD_REQUEST, "unknown login state").into_response();
	};
	let challenge = state
		.inner
		.pending
		.lock()
		.expect("pending")
		.get(&form.state)
		.map(|item| item.challenge.clone())
		.unwrap_or_default();
	let code = magpie::new_id();
	let claims = mock_claims(&form.who, &nonce, &state.inner.config.audience);
	let Some(claims) = claims else {
		return (StatusCode::BAD_REQUEST, "unknown example user").into_response();
	};
	state
		.inner
		.codes
		.lock()
		.expect("codes")
		.insert(code.clone(), IssuedCode { challenge, claims });
	let base = public_base(&headers);
	Redirect::to(&format!(
		"{base}/api/auth/callback?code={code}&state={}",
		form.state
	))
	.into_response()
}

async fn mock_token() -> StatusCode {
	StatusCode::NO_CONTENT
}

async fn mock_discovery(headers: HeaderMap) -> Json<Value> {
	let base = public_base(&headers);
	Json(json!({
		"issuer": format!("{base}/mock-idp"),
		"authorization_endpoint": format!("{base}/mock-idp/authorize"),
		"token_endpoint": format!("{base}/mock-idp/token"),
		"response_types_supported": ["code"],
		"subject_types_supported": ["public"],
		"id_token_signing_alg_values_supported": ["HS256"],
		"code_challenge_methods_supported": ["S256"]
	}))
}

#[allow(clippy::result_large_err)]
fn session_from(
	state: &AppState,
	headers: &HeaderMap,
) -> Result<(SessionRecord, String), Response> {
	let cookie =
		read_cookie(headers, "magpie_session").ok_or_else(|| Redirect::to("/login").into_response())?;
	let opened = state
		.inner
		.codec
		.open(&cookie, Utc::now())
		.map_err(|_| Redirect::to("/login").into_response())?;
	// Keep validation and renewal under one lock so disabling a user cannot
	// race with renewal and recreate a revoked session.
	let store = state.inner.store.lock().expect("db");
	let stored = store.get_session(&opened.session_id).ok().flatten();
	if !opened.break_glass {
		let user = store.portal_user(&opened.subject);
		if !matches!(user, Ok(Some(ref user)) if !user.disabled) {
			return Err(
				(
					StatusCode::FORBIDDEN,
					"Your portal account is disabled or requires a new sign-in.",
				)
					.into_response(),
			);
		}
	}
	if stored.is_none() {
		return Err(Redirect::to("/login").into_response());
	}
	let (touched, resealed) = state
		.inner
		.codec
		.touch(opened, Utc::now())
		.map_err(|_| Redirect::to("/login").into_response())?;
	let _ = store.put_session(
		&touched.session_id,
		&touched.subject,
		&serde_json::to_string(&touched).unwrap_or_default(),
		&touched.absolute_expiry.to_rfc3339(),
	);
	Ok((touched, resealed))
}

fn viewer_of(state: &AppState, session: &SessionRecord) -> ui::Viewer {
	ui::Viewer {
		name: session.name.clone(),
		email: session.email.clone(),
		roles: access::role_names(state, session),
		permissions: access::permissions(state, session),
		break_glass: session.break_glass,
	}
}

fn page(state: &AppState, headers: &HeaderMap, path: &str, title: &str, body: String) -> Response {
	let body = if load_settings(state).guardrail_mode == guardrail::Mode::LogOnly {
		format!(
			r#"<p class="banner warn" role="status">Log-only trial: policies are evaluated and logged, but traffic is not blocked or changed. Kill switches are also log-only.</p>{body}"#
		)
	} else {
		body
	};
	match session_from(state, headers) {
		Ok((session, cookie)) => {
			if !access::allows(state, &session, access::page_permission(path)) {
				return (StatusCode::FORBIDDEN, "Your role cannot view this page.").into_response();
			}
			if headers.get("x-magpie-view").is_some_and(|v| v == "1") {
				return (
					[
						(header::CACHE_CONTROL, "no-store".to_string()),
						(
							header::SET_COOKIE,
							session_cookie(&cookie, state.inner.config.cookie_secure),
						),
					],
					Json(json!({"title":title,"body":body})),
				)
					.into_response();
			}
			if !state.inner.config.gateway_ui_dir.is_empty() {
				return Redirect::to(&format!("/ui/governance{path}")).into_response();
			}
			let mut response = Html(ui::layout(
				title,
				path,
				Some(&viewer_of(state, &session)),
				state.inner.config.demo,
				&body,
			))
			.into_response();
			if let Ok(value) = session_cookie(&cookie, state.inner.config.cookie_secure).parse() {
				response.headers_mut().insert(header::SET_COOKIE, value);
			}
			response
		},
		Err(response) if state.inner.config.demo => {
			let viewer = ui::Viewer {
				name: "Alex Nguyen".into(),
				email: "alex.nguyen@example.gov.au".into(),
				roles: vec!["Auditor".into()],
				permissions: magpie::rbac::ACTIONS.iter().map(|(p, _, _)| *p).collect(),
				break_glass: false,
			};
			let _ = response;
			Html(ui::layout(title, path, Some(&viewer), true, &body)).into_response()
		},
		Err(response) => response,
	}
}

#[allow(clippy::result_large_err)]
fn require(
	state: &AppState,
	headers: &HeaderMap,
	permission: Permission,
) -> Result<SessionRecord, Response> {
	if state.inner.config.demo && read_cookie(headers, "magpie_session").is_none() {
		return Err(
			(
				StatusCode::FORBIDDEN,
				"Example mode can read pages. Sign in to change anything.",
			)
				.into_response(),
		);
	}
	let (session, _) = session_from(state, headers)?;
	if !access::allows(state, &session, permission) {
		return Err((StatusCode::FORBIDDEN, "Your role cannot do that.").into_response());
	}
	Ok(session)
}

async fn dashboard(State(state): State<AppState>, headers: HeaderMap) -> Response {
	let events = state
		.inner
		.store
		.lock()
		.expect("db")
		.search_audit(&AuditQuery {
			limit: 500,
			..AuditQuery::default()
		})
		.unwrap_or_default();
	let count = |kind: &str| {
		events
			.iter()
			.filter(|event| event.kind == kind || event.decision == kind)
			.count()
	};
	let body = format!(
		r#"
<div class="cards">
<div class="card"><strong>{}</strong>Decisions</div>
<div class="card"><strong>{}</strong>Blocks</div>
<div class="card"><strong>{}</strong>DLP hits</div>
<div class="card"><strong>{}</strong>Logins</div>
</div>
<h2>Top tools</h2>
{}
<h2>Top agents</h2>
{}
<h2>Recent decisions</h2>
{}
<p class="muted">Counts come from the audit log. Full payloads are off unless a setting turns them on.</p>"#,
		events.len(),
		events
			.iter()
			.filter(|event| event.decision == "block")
			.count(),
		events
			.iter()
			.filter(|event| event.detail.to_string().contains("\"hits\""))
			.count(),
		count("login"),
		tally_table(&events, |event| event.tool.as_str(), "Tool"),
		tally_table(&events, |event| event.agent_id.as_str(), "Agent"),
		table(
			&[
				"When", "Actor", "Agent", "Model", "Tool", "Decision", "Reason"
			],
			&rows_audit(&events[..events.len().min(8)])
		)
	);
	page(&state, &headers, "/dashboard", "Dashboard", body)
}

async fn agents_page(State(state): State<AppState>, headers: HeaderMap) -> Response {
	let docs = state
		.inner
		.store
		.lock()
		.expect("db")
		.list_documents("agent")
		.unwrap_or_default();
	let rows = docs
		.iter()
		.map(|doc| {
			vec![
				doc.body["name"].as_str().unwrap_or(&doc.id).to_string(),
				doc.body["owner"].as_str().unwrap_or("").to_string(),
				doc.body["tools"].to_string(),
				doc.body["models"].to_string(),
			]
		})
		.collect::<Vec<_>>();
	page(
		&state,
		&headers,
		"/agents",
		"Agents and clients",
		table(
			&["Agent", "Owner", "Approved tools", "Approved models"],
			&rows,
		),
	)
}

async fn catalogue_page(State(state): State<AppState>, headers: HeaderMap) -> Response {
	let tools = state
		.inner
		.store
		.lock()
		.expect("db")
		.list_documents("tool")
		.unwrap_or_default();
	let rows = tools
		.iter()
		.map(|doc| {
			vec![
				doc.body["server"].as_str().unwrap_or("").to_string(),
				doc.body["name"].as_str().unwrap_or(&doc.id).to_string(),
				doc.body["status"].as_str().unwrap_or("").to_string(),
				doc.body["classification"]
					.as_str()
					.unwrap_or("")
					.to_string(),
			]
		})
		.collect::<Vec<_>>();
	page(
		&state,
		&headers,
		"/catalogue",
		"MCP servers and tools",
		format!(
			"<p>Anything that is not approved is denied when a tool call reaches the guardrail.</p>{}",
			table(&["Server", "Tool", "Status", "Classification"], &rows)
		),
	)
}

async fn providers_page(State(state): State<AppState>, headers: HeaderMap) -> Response {
	let docs = state
		.inner
		.store
		.lock()
		.expect("db")
		.list_documents("provider")
		.unwrap_or_default();
	let rows = docs
		.iter()
		.map(|doc| {
			let region = doc.body["region"].as_str().unwrap_or("");
			vec![
				doc.body["name"].as_str().unwrap_or(&doc.id).to_string(),
				region.to_string(),
				magpie::sovereignty::residency_label(region).to_string(),
				doc.body["kind"].as_str().unwrap_or("model").to_string(),
			]
		})
		.collect::<Vec<_>>();
	let settings = load_settings(&state);
	let mode = if settings.sovereign_mode { "on" } else { "off" };
	page(
		&state,
		&headers,
		"/providers",
		"Providers and routes",
		format!(
			"<p>Sovereign mode is {mode}. A provider outside Australia is blocked while it is on, and OFFICIAL: Sensitive and above stays on Australian regions even when it is off.</p>{}",
			table(&["Name", "Region", "Residency", "Kind"], &rows)
		),
	)
}

async fn policies_page(State(state): State<AppState>, headers: HeaderMap) -> Response {
	let sample = magpie::policy::VisualPolicy {
		name: "Example tool allow list".into(),
		effect: magpie::policy::Effect::Allow,
		all: vec![magpie::policy::Clause {
			field: "mcp.tool.name".into(),
			op: "in".into(),
			value: json!(["example_search"]),
		}],
	};
	let generated = magpie::policy::generate(&sample)
		.map(|item| item.expression)
		.unwrap_or_else(|error| error.to_string());
	let tested = magpie::policy::test_expression(
		&generated,
		&json!({"mcp": {"tool": {"name": "example_search"}}}),
	);
	let stored = state
		.inner
		.store
		.lock()
		.expect("db")
		.get_document("policy", "approved-tools")
		.ok()
		.flatten()
		.and_then(|doc| {
			doc
				.body
				.get("cel")
				.and_then(|value| value.as_str())
				.map(str::to_string)
		})
		.unwrap_or_else(|| "false".into());
	let body = format!(
		r#"<p>The visual editor writes CEL. Tool and JWT fields can be attached to the proxy as an allow rule. Marking and region rules are enforced by this portal's guardrail. An empty approved-tool list is <code>false</code>, which denies every tool.</p>
<h2>Approved tools</h2>
<pre>{}</pre>
<h2>Example</h2>
<pre>{}</pre>
<p>Sample request result: {}</p>
<form method="post" action="/policies">
<input type="hidden" name="mode" value="visual">
<label for="field">Field</label>
<select id="field" name="field">
<option>mcp.tool.name</option>
<option>jwt.groups</option>
<option>destination.region</option>
<option>marking</option>
</select>
<label for="op">Operator</label>
<select id="op" name="op"><option>eq</option><option selected>in</option><option>not_in</option></select>
<label for="value">Value</label>
<input id="value" name="value" value="example_search" required>
<button type="submit">Generate CEL</button>
</form>
<h2>Raw CEL</h2>
<form method="post" action="/policies">
<input type="hidden" name="mode" value="raw">
<label for="raw">Expression</label>
<textarea id="raw" name="raw" rows="4" required>{}</textarea>
<button type="submit">Check and save</button>
</form>
{}"#,
		ui::esc(&stored),
		ui::esc(&generated),
		ui::esc(
			&tested
				.value
				.map(|value| value.to_string())
				.unwrap_or_else(|| tested.error.unwrap_or_default())
		),
		ui::esc(&generated),
		pending_html(&state)
	);
	page(&state, &headers, "/policies", "Policies", body)
}

#[derive(Deserialize)]
struct PolicyForm {
	#[serde(default)]
	mode: String,
	#[serde(default)]
	field: String,
	#[serde(default)]
	op: String,
	#[serde(default)]
	value: String,
	#[serde(default)]
	raw: String,
}

async fn policies_post(
	State(state): State<AppState>,
	headers: HeaderMap,
	Form(form): Form<PolicyForm>,
) -> Response {
	let session = match require(&state, &headers, Permission::EditPolicy) {
		Ok(session) => session,
		Err(response) => return response,
	};
	let (expression, note) = if form.mode == "raw" {
		let tested = magpie::policy::test_expression(
			&form.raw,
			&json!({"mcp": {"tool": {"name": "example_search"}}}),
		);
		if !tested.compiled {
			return page(
				&state,
				&headers,
				"/policies",
				"Policies",
				format!(
					"<p role=\"alert\">{}</p>",
					ui::esc(
						&tested
							.error
							.unwrap_or_else(|| "CEL did not compile.".into())
					)
				),
			);
		}
		(
			form.raw,
			"Raw CEL compiled against the sample request.".to_string(),
		)
	} else {
		let value = if form.op == "eq" {
			json!(form.value)
		} else {
			json!(
				form
					.value
					.split(',')
					.map(|item| item.trim())
					.filter(|item| !item.is_empty())
					.collect::<Vec<_>>()
			)
		};
		let policy = magpie::policy::VisualPolicy {
			name: "portal".into(),
			effect: magpie::policy::Effect::Allow,
			all: vec![magpie::policy::Clause {
				field: form.field,
				op: form.op,
				value,
			}],
		};
		match magpie::policy::generate(&policy) {
			Ok(item) => (item.expression, item.note),
			Err(error) => {
				return page(
					&state,
					&headers,
					"/policies",
					"Policies",
					format!("<p role=\"alert\">{}</p>", ui::esc(&error.to_string())),
				);
			},
		}
	};
	let tested = magpie::policy::test_expression(
		&expression,
		&json!({"mcp": {"tool": {"name": "example_search"}}, "destination": {"region": "ap-southeast-2"}}),
	);
	let status = save_revision(
		&state,
		"policy",
		"portal",
		&session.email,
		json!({"cel": expression}),
		second_person(&state),
	);
	let waiting = match status {
		Ok(magpie::change::RevisionStatus::Pending) => {
			"Saved as a pending revision. A different person has to approve it before it is active."
		},
		Ok(_) => "Saved and active.",
		Err(_) => "The revision could not be stored.",
	};
	page(
		&state,
		&headers,
		"/policies",
		"Policies",
		format!(
			"<pre>{}</pre><p>{}</p><p>Test: {}</p><p>{}</p>{}",
			ui::esc(&expression),
			ui::esc(&note),
			ui::esc(&format!("{tested:?}")),
			ui::esc(waiting),
			pending_html(&state)
		),
	)
}

#[derive(Deserialize)]
struct DlpPageQuery {
	saved: Option<String>,
}

async fn dlp_page(
	State(state): State<AppState>,
	headers: HeaderMap,
	Query(query): Query<DlpPageQuery>,
) -> Response {
	let disabled = if require(&state, &headers, Permission::EditDlp).is_ok() {
		""
	} else {
		"disabled"
	};
	let rules = load_rules(&state);
	let mode = load_settings(&state).guardrail_mode;
	let mut rows = vec![];
	for rule in &rules {
		let kind = serde_json::to_value(rule.kind)
			.unwrap()
			.as_str()
			.unwrap()
			.to_string();
		let mut options = String::new();
		for action in [
			dlp::Action::Allow,
			dlp::Action::Log,
			dlp::Action::Redact,
			dlp::Action::Mask,
			dlp::Action::Tokenise,
			dlp::Action::Block,
			dlp::Action::RequireApproval,
		] {
			let value = serde_json::to_value(action)
				.unwrap()
				.as_str()
				.unwrap()
				.to_string();
			options.push_str(&format!(
				"<option value=\"{}\" {}>{}</option>",
				value,
				if rule.action == action {
					"selected"
				} else {
					""
				},
				action.as_str()
			));
		}
		let effective = if !rule.enabled {
			"Off — no findings or actions".to_string()
		} else if mode == guardrail::Mode::LogOnly {
			format!("Observe only · would {}", rule.action.as_str())
		} else {
			format!("On · {}", rule.action.as_str())
		};
		let operation = if rule.enabled { "disable" } else { "enable" };
		let operation_label = if rule.enabled {
			"Disable detector"
		} else {
			"Enable detector"
		};
		rows.push(vec![dlp::kind_name(rule.kind).into(), effective, format!(r#"<form method="post" action="/dlp/rules"><input type="hidden" name="kind" value="{kind}"><label class="small" for="action-{kind}">Action for {kind}</label><select id="action-{kind}" name="action" {disabled}>{options}</select> <button type="submit" name="operation" value="save" {disabled}>Save action</button> <button type="submit" name="operation" value="{operation}" {disabled}>{operation_label}</button><details><summary>More options</summary><button type="submit" name="operation" value="restore" {disabled}>Restore default</button></details></form>"#)]);
	}
	let body = format!(
		r#"<p>Australian identifiers are inspected locally in prompts and responses passing through Magpie guardrails. Each detector has its own action. Saved changes follow the approval setting and apply without a redeploy.</p>
<p><a href="/settings">Manage log-only or enforcement mode</a> · <a href="/australia">Australian assurance and detector coverage</a></p>
<p class="banner">Log-only records what the policy would do and sends traffic unchanged. Redact, mask, tokenise and block take effect in enforcement mode. This test always shows a sanitised preview.</p>
<p><a href="/controls">View all controls and framework coverage</a></p>
<p>Disable removes this detector’s findings and actions. Allow still detects and records findings. Restore default enables the detector and resets its action. Changes may need a different person’s approval; the table always shows the active revision. Marking and caveat detection settings do not disable built-in classification routing. Disabling Presidio also stops requests to its analyser.</p>
<h2>Detector actions</h2>{}
<p>Mask reveals the final two characters. Tokenise creates a keyed, non-reversible pseudonym. Public ABNs and ACNs can be set to allow where your organisation permits it.</p>
<section id="pending-changes" aria-label="Change approvals">{}</section>
<h2>Test Australian privacy controls</h2>
<p>Use synthetic examples. Test text is not saved to the audit log or sent to an AI model. TFNs detected by context or a candidate checksum are not verified with the ATO.</p>
<form method="post" action="/dlp"><label for="text">Text to check</label><textarea id="text" name="text" rows="4" required {disabled}></textarea><button type="submit" {disabled}>Run DLP</button></form>"#,
		table(&["Detector", "Effective state", "Change or disable"], &rows),
		pending_html(&state)
	);
	let notice = match query.saved.as_deref() {
		Some("pending") => {
			"<p role=\"status\"><strong>Change saved for approval.</strong> The active rule has not changed. A different authorised person must approve it under Pending changes below.</p>"
		},
		Some("active") => {
			"<p role=\"status\"><strong>Change saved and active.</strong> The table shows the current rule.</p>"
		},
		_ => "",
	};
	page(
		&state,
		&headers,
		"/dlp",
		"Australian privacy",
		format!("{notice}{body}"),
	)
}

#[derive(Deserialize)]
struct DlpRuleForm {
	kind: dlp::HitKind,
	action: dlp::Action,
	enabled: Option<bool>,
	operation: Option<String>,
}

async fn dlp_rule_post(
	State(state): State<AppState>,
	headers: HeaderMap,
	Form(form): Form<DlpRuleForm>,
) -> Response {
	let session = match require(&state, &headers, Permission::EditDlp) {
		Ok(s) => s,
		Err(r) => return r,
	};
	let Some(mut rule) = load_rules(&state).into_iter().find(|r| r.kind == form.kind) else {
		return StatusCode::BAD_REQUEST.into_response();
	};
	match form.operation.as_deref() {
		Some("restore") => {
			let default = dlp::default_rules()
				.into_iter()
				.find(|r| r.kind == form.kind)
				.unwrap();
			rule.action = default.action;
			rule.enabled = default.enabled;
		},
		Some("disable") => rule.enabled = false,
		Some("enable") => rule.enabled = true,
		None | Some("save") => {
			rule.action = form.action;
			if let Some(enabled) = form.enabled {
				rule.enabled = enabled;
			}
		},
		_ => return StatusCode::BAD_REQUEST.into_response(),
	}
	match save_revision(
		&state,
		"dlp_rule",
		&rule.id,
		&session.email,
		json!(rule),
		second_person(&state),
	) {
		Ok(status) => {
			let pending = status == magpie::change::RevisionStatus::Pending;
			let _ = record_audit(
				&state,
				"policy",
				&session.email,
				"",
				"",
				"",
				"log",
				&format!(
					"DLP {} change {}",
					dlp::kind_name(rule.kind),
					if pending {
						"pending approval"
					} else {
						"active"
					}
				),
			);
			Redirect::to(if pending {
				"/dlp?saved=pending"
			} else {
				"/dlp?saved=active"
			})
			.into_response()
		},
		Err(_) => (
			StatusCode::INTERNAL_SERVER_ERROR,
			"The rule could not be saved.",
		)
			.into_response(),
	}
}

#[derive(Deserialize)]
struct DlpForm {
	text: String,
}

async fn dlp_post(
	State(state): State<AppState>,
	headers: HeaderMap,
	Form(form): Form<DlpForm>,
) -> Response {
	if let Err(response) = require(&state, &headers, Permission::EditDlp) {
		return response;
	}
	let outcome = dlp::inspect(
		&form.text,
		&Default::default(),
		&load_rules(&state),
		&[],
		state.inner.config.session_secret.as_bytes(),
	);
	let preview = dlp::inspect(
		&form.text,
		&Default::default(),
		&dlp::default_rules(),
		&[],
		state.inner.config.session_secret.as_bytes(),
	);
	page(
		&state,
		&headers,
		"/dlp",
		"Australian privacy",
		format!(
			"<p>Configured action: {}</p><h2>Sanitised preview</h2><pre>{}</pre><p>{} hit(s). Original values are withheld, including for allow and log rules.</p><p><a href=\"/dlp\">Back to Australian privacy controls</a></p>",
			ui::esc(outcome.action.as_str()),
			ui::esc(&preview.output),
			outcome.hits.len()
		),
	)
}

#[derive(Deserialize)]
struct AuditFilter {
	#[serde(default)]
	decision: String,
	#[serde(default)]
	actor: String,
	#[serde(default)]
	kind: String,
}

async fn audit_page(
	State(state): State<AppState>,
	headers: HeaderMap,
	Query(filter): Query<AuditFilter>,
) -> Response {
	if let Err(response) = require(&state, &headers, Permission::ReadAudit)
		&& !state.inner.config.demo
	{
		return response;
	}
	let query = AuditQuery {
		limit: 100,
		decision: blank_to_none(&filter.decision),
		actor: blank_to_none(&filter.actor),
		kind: blank_to_none(&filter.kind),
		..AuditQuery::default()
	};
	let events = state
		.inner
		.store
		.lock()
		.expect("db")
		.search_audit(&query)
		.unwrap_or_default();
	let body = format!(
		r#"<form method="get" action="/audit">
<label for="decision">Decision</label>
<input id="decision" name="decision" value="{}">
<label for="actor">Actor</label>
<input id="actor" name="actor" value="{}">
<label for="kind">Kind</label>
<input id="kind" name="kind" value="{}">
<button type="submit">Filter</button>
</form>
<p><a href="/audit.csv">Export CSV</a></p>
{}"#,
		ui::esc(&filter.decision),
		ui::esc(&filter.actor),
		ui::esc(&filter.kind),
		table(
			&[
				"When", "Kind", "Actor", "Agent", "Model", "Tool", "Decision", "Reason"
			],
			&rows_audit_full(&events)
		)
	);
	page(&state, &headers, "/audit", "Audit log", body)
}

async fn audit_csv(State(state): State<AppState>, headers: HeaderMap) -> Response {
	if let Err(response) = require(&state, &headers, Permission::ExportAudit) {
		return response;
	}
	let events = state
		.inner
		.store
		.lock()
		.expect("db")
		.search_audit(&AuditQuery {
			limit: 1000,
			..AuditQuery::default()
		})
		.unwrap_or_default();
	(
		[(header::CONTENT_TYPE, "text/csv; charset=utf-8")],
		magpie::audit::to_csv(&events),
	)
		.into_response()
}

async fn approvals_page(State(state): State<AppState>, headers: HeaderMap) -> Response {
	let docs = state
		.inner
		.store
		.lock()
		.expect("db")
		.list_documents("approval")
		.unwrap_or_default();
	let mut rows = Vec::new();
	for doc in &docs {
		let approval: Approval =
			serde_json::from_value(doc.body.clone()).unwrap_or_else(|_| placeholder_approval(&doc.id));
		rows.push(vec![
			approval.name,
			format!("{:?}", approval.kind),
			approval.business_owner,
			approval.purpose,
			approval.classification,
			approval.expires_on,
			format!("{:?}", approval.status),
			format!("<form method=\"post\" action=\"/approvals/{}/approve\"><button type=\"submit\">Approve</button></form>", ui::esc(&doc.id)),
		]);
	}
	let body = format!(
		r#"<p>A request needs a business owner, a purpose, a data classification and an expiry. Approving it writes the tool into the allow list. The approver cannot be the person who submitted it. Anything that is still pending is denied.</p>
{}
<h2>New request</h2>
<form method="post" action="/approvals">
<label for="name">Name</label>
<input id="name" name="name" required>
<label for="kind">Kind</label>
<select id="kind" name="kind"><option value="tool">Tool</option><option value="model">Model</option><option value="mcpServer">MCP server</option></select>
<label for="business_owner">Business owner</label>
<input id="business_owner" name="business_owner" required>
<label for="purpose">Purpose</label>
<input id="purpose" name="purpose" required>
<label for="classification">Data classification</label>
<input id="classification" name="classification" value="OFFICIAL" required>
<label for="expires_on">Expiry</label>
<input id="expires_on" name="expires_on" value="2027-06-01" required>
<button type="submit">Queue request</button>
</form>
<p><a href="/use-cases.csv">Export use-case register (CSV)</a></p>"#,
		table(
			&[
				"Name",
				"Kind",
				"Owner",
				"Purpose",
				"Classification",
				"Expiry",
				"Status",
				"Action"
			],
			&rows
		)
	);
	page(&state, &headers, "/approvals", "Approvals", body)
}

async fn approve(
	State(state): State<AppState>,
	headers: HeaderMap,
	Path(id): Path<String>,
) -> Response {
	let session = match require(&state, &headers, Permission::Approve) {
		Ok(session) => session,
		Err(response) => return response,
	};
	let store = state.inner.store.lock().expect("db");
	let Some(doc) = store.get_document("approval", &id).ok().flatten() else {
		return (StatusCode::NOT_FOUND, "No such approval").into_response();
	};
	let mut approval: Approval = serde_json::from_value(doc.body.clone())
		.map_err(|error| error.to_string())
		.unwrap_or_else(|_| placeholder_approval(&id));
	if approval.business_owner.eq_ignore_ascii_case(&session.email)
		|| approval.business_owner.eq_ignore_ascii_case(&session.name)
	{
		return (
			StatusCode::FORBIDDEN,
			"You cannot approve your own request.",
		)
			.into_response();
	}
	approval.status = ApprovalStatus::Approved;
	let mut updated = doc;
	updated.body = serde_json::to_value(&approval).unwrap_or(json!({}));
	updated.revision += 1;
	updated.updated_by = session.email.clone();
	updated.updated_at = Utc::now().to_rfc3339();
	store.put_document(&updated).ok();
	if approval.kind == ItemKind::Tool {
		let _ = store.put_document(&Document {
			kind: "tool".into(),
			id: approval.name.clone(),
			body: json!({"server": "approved", "name": approval.name, "status": "approved", "classification": approval.classification}),
			revision: 1,
			updated_at: Utc::now().to_rfc3339(),
			updated_by: session.email.clone(),
			active: true,
		});
	}
	drop(store);
	let approvals = load_approvals(&state);
	let cel = magpie::approvals::cel_for_approved_tools(&approvals, Utc::now().date_naive());
	let _ = save_revision(
		&state,
		"policy",
		"approved-tools",
		&session.email,
		json!({"cel": cel}),
		false,
	);
	let _ = record_audit(
		&state,
		"approval",
		&session.email,
		"",
		&approval.name,
		"",
		"allow",
		"approved",
	);
	Redirect::to("/approvals").into_response()
}

#[derive(Deserialize)]
struct ApprovalForm {
	name: String,
	kind: String,
	business_owner: String,
	purpose: String,
	classification: String,
	expires_on: String,
}

async fn approvals_post(
	State(state): State<AppState>,
	headers: HeaderMap,
	Form(form): Form<ApprovalForm>,
) -> Response {
	let session = match require(&state, &headers, Permission::SubmitApproval) {
		Ok(session) => session,
		Err(response) => return response,
	};
	let kind = match form.kind.as_str() {
		"model" => ItemKind::Model,
		"mcpServer" => ItemKind::McpServer,
		_ => ItemKind::Tool,
	};
	let mut use_case = magpie::register::UseCase::example();
	use_case.name = form.name.clone();
	use_case.description = form.purpose.clone();
	use_case.owner_name = form.business_owner.clone();
	use_case.reference = format!("EXAMPLE-{}", magpie::new_id());
	let approval = Approval {
		id: magpie::new_id(),
		kind,
		name: form.name,
		business_owner: form.business_owner,
		purpose: form.purpose,
		classification: form.classification,
		expires_on: form.expires_on,
		status: ApprovalStatus::Pending,
		use_case,
	};
	if let Err(error) = magpie::approvals::validate(&approval) {
		return page(
			&state,
			&headers,
			"/approvals",
			"Approvals",
			format!("<p role=\"alert\">{}</p>", ui::esc(&error.to_string())),
		);
	}
	let id = approval.id.clone();
	let _ = save_revision(
		&state,
		"approval",
		&id,
		&session.email,
		serde_json::to_value(&approval).unwrap_or(json!({})),
		false,
	);
	let _ = record_audit(
		&state,
		"approval",
		&session.email,
		"",
		&approval.name,
		"",
		"allow",
		"queued",
	);
	Redirect::to("/approvals").into_response()
}

async fn approve_revision(
	State(state): State<AppState>,
	headers: HeaderMap,
	Path(id): Path<String>,
) -> Response {
	let session = match require(&state, &headers, Permission::Approve) {
		Ok(session) => session,
		Err(response) => return response,
	};
	let store = state.inner.store.lock().expect("db");
	let Some(mut revision) = store.get_revision(&id).ok().flatten() else {
		return (StatusCode::NOT_FOUND, "No such revision").into_response();
	};
	if let Err(error) = magpie::change::approve(&mut revision, &session.email) {
		return (StatusCode::FORBIDDEN, error.to_string()).into_response();
	}
	store.update_revision(&revision).ok();
	let current = store
		.get_document(&revision.kind, &revision.doc_id)
		.ok()
		.flatten();
	let next_revision = current.as_ref().map(|doc| doc.revision + 1).unwrap_or(1);
	store
		.put_document(&Document {
			kind: revision.kind.clone(),
			id: revision.doc_id.clone(),
			body: revision.after.clone(),
			revision: next_revision,
			updated_at: Utc::now().to_rfc3339(),
			updated_by: session.email.clone(),
			active: true,
		})
		.ok();
	drop(store);
	let _ = record_audit(
		&state,
		"policy",
		&session.email,
		"",
		&revision.doc_id,
		"",
		"allow",
		"revision approved",
	);
	Redirect::to("/policies").into_response()
}

async fn rollback_revision(
	State(state): State<AppState>,
	headers: HeaderMap,
	Path(id): Path<String>,
) -> Response {
	let session = match require(&state, &headers, Permission::EditPolicy) {
		Ok(session) => session,
		Err(response) => return response,
	};
	let store = state.inner.store.lock().expect("db");
	let Some(revision) = store.get_revision(&id).ok().flatten() else {
		return (StatusCode::NOT_FOUND, "No such revision").into_response();
	};
	let permission = match revision.kind.as_str() {
		"settings" | "kill" => Permission::ManageSettings,
		"dlp_rule" => Permission::EditDlp,
		"approval" => Permission::Approve,
		_ => Permission::EditPolicy,
	};
	if !store
		.access_control()
		.ok()
		.flatten()
		.is_some_and(|a| a.allows(&session, permission))
	{
		return (
			StatusCode::FORBIDDEN,
			"Your role cannot roll back this type of change.",
		)
			.into_response();
	}
	if revision.status != magpie::change::RevisionStatus::Active {
		return (
			StatusCode::CONFLICT,
			"Only an active revision can be rolled back.",
		)
			.into_response();
	}
	let rolled = magpie::change::rollback_of(&revision, &session.email, Utc::now());
	store.append_revision(&rolled).ok();
	store
		.put_document(&Document {
			kind: rolled.kind.clone(),
			id: rolled.doc_id.clone(),
			body: rolled.after.clone(),
			revision: 1,
			updated_at: Utc::now().to_rfc3339(),
			updated_by: session.email.clone(),
			active: true,
		})
		.ok();
	drop(store);
	let _ = record_audit(
		&state,
		"policy",
		&session.email,
		"",
		&revision.doc_id,
		"",
		"allow",
		"revision rolled back",
	);
	Redirect::to("/policies").into_response()
}

async fn settings_page(State(state): State<AppState>, headers: HeaderMap) -> Response {
	let settings_disabled = if require(&state, &headers, Permission::ManageSettings).is_ok() {
		""
	} else {
		"disabled"
	};
	let mode_disabled = if require(&state, &headers, Permission::ChangeEnforcement).is_ok() {
		""
	} else {
		"disabled"
	};
	let settings = load_settings(&state);
	let body = format!(
		r#"<h2>Enforcement</h2>
<p>Current mode: <strong>{}</strong></p>
<form method="post" action="/settings/guardrail-mode">
<label for="guardrail_mode">Guardrail mode</label>
<select id="guardrail_mode" name="mode" aria-describedby="guardrail-mode-help" {}>
<option value="log-only" {}>Log-only — record findings and allow traffic unchanged</option>
<option value="enforce" {}>Enforcement — apply policy actions</option>
</select>
<p id="guardrail-mode-help">Applies to new requests immediately and stays saved across restarts. Log-only also makes kill switches log-only. Authentication stays required.</p>
<button type="submit" {}>Apply mode</button>
</form>
<p>Changing this mode requires the Change enforcement mode permission. Changes take effect immediately and are audited; policy edits below retain their existing approval process.</p>
<h2>Policy settings</h2>
<form method="post" action="/settings"><fieldset {}><legend>Policy settings and kill switches</legend>
<label for="sovereign">Sovereign mode</label>
<select id="sovereign" name="sovereign"><option value="true" {}>On</option><option value="false" {}>Off</option></select>
<label for="telemetry">Telemetry and phone-home</label>
<select id="telemetry" name="telemetry"><option value="false" {}>Off</option><option value="true" {}>On</option></select>
<label for="presidio">Presidio analyzer URL</label>
<input id="presidio" name="presidio" value="{}" placeholder="Leave empty to stay local">
<label for="retention">Audit retention (days)</label>
<input id="retention" name="retention" value="{}">
<label for="kill_global">Global kill switch</label>
<select id="kill_global" name="kill_global"><option value="false" {}>Off</option><option value="true" {}>On</option></select>
<label for="kill_agent">Per-agent kill switch</label>
<input id="kill_agent" name="kill_agent" value="{}" placeholder="agent id, or leave empty">
<button type="submit">Save settings</button>
</fieldset></form>
<p>The kill switch is written immediately. Other settings wait for a second person when that control is on.</p>
<p>Break-glass is {}.</p>
<p>SAML 2.0 is not enabled.</p>
<p>Presidio, when configured, uses ghcr.io/data-privacy-stack images. Microsoft's registry is not used.</p>
<p>Prompt injection can call a webhook or a model you host. Llama Prompt Guard 2 weights are not included. They are under the Llama 4 Community License.</p>"#,
		settings.guardrail_mode.as_str(),
		mode_disabled,
		selected(settings.guardrail_mode == guardrail::Mode::LogOnly),
		selected(settings.guardrail_mode == guardrail::Mode::Enforce),
		mode_disabled,
		settings_disabled,
		selected(settings.sovereign_mode),
		selected(!settings.sovereign_mode),
		selected(!settings.telemetry_enabled),
		selected(settings.telemetry_enabled),
		ui::esc(&settings.presidio_analyzer_url),
		settings.retention_days,
		selected(!settings.kill_global),
		selected(settings.kill_global),
		ui::esc(&settings.kill_agents),
		if state.inner.config.break_glass.enabled {
			"enabled and audited"
		} else {
			"disabled"
		}
	);
	let body = format!("{body}{}", pending_html(&state));
	page(&state, &headers, "/settings", "Settings", body)
}

async fn guardrail_mode_get(State(state): State<AppState>, headers: HeaderMap) -> Response {
	if let Err(response) = require(&state, &headers, Permission::ReadSettings) {
		return response;
	}
	Json(json!({"mode": load_settings(&state).guardrail_mode})).into_response()
}

#[derive(Deserialize)]
struct GuardrailModeForm {
	mode: String,
}

async fn guardrail_mode_post(
	State(state): State<AppState>,
	headers: HeaderMap,
	Form(form): Form<GuardrailModeForm>,
) -> Response {
	let session = match require(&state, &headers, Permission::ChangeEnforcement) {
		Ok(session) => session,
		Err(response) => return response,
	};
	let mode = match form.mode.parse::<guardrail::Mode>() {
		Ok(mode) => mode,
		Err(_) => return (StatusCode::BAD_REQUEST, "Choose log-only or enforce.").into_response(),
	};
	let result = state
		.inner
		.store
		.lock()
		.expect("db")
		.set_guardrail_mode(mode, &session.email);
	match result {
		Ok(()) => Redirect::to("/settings").into_response(),
		Err(_) => (
			StatusCode::INTERNAL_SERVER_ERROR,
			"Could not save the mode. No change was applied.",
		)
			.into_response(),
	}
}

#[derive(Deserialize)]
struct SettingsForm {
	sovereign: String,
	telemetry: String,
	presidio: String,
	retention: u32,
	#[serde(default)]
	kill_global: String,
	#[serde(default)]
	kill_agent: String,
}

async fn settings_post(
	State(state): State<AppState>,
	headers: HeaderMap,
	Form(form): Form<SettingsForm>,
) -> Response {
	let session = match require(&state, &headers, Permission::ManageSettings) {
		Ok(session) => session,
		Err(response) => return response,
	};
	let agents = form
		.kill_agent
		.split(',')
		.map(str::trim)
		.filter(|item| !item.is_empty())
		.map(str::to_string)
		.collect::<Vec<_>>();
	let kill = KillSwitch {
		global: form.kill_global == "true",
		agents,
	};
	let _ = save_revision(
		&state,
		"kill",
		"global",
		&session.email,
		serde_json::to_value(&kill).unwrap_or(json!({})),
		false,
	);
	let current = load_settings(&state);
	let body = json!({
		"sovereignMode": form.sovereign == "true",
		"telemetryEnabled": form.telemetry == "true",
		"presidioAnalyzerUrl": form.presidio,
		"capturePayloads": false,
		"retentionDays": form.retention,
		"requireApproval": current.require_approval,
		"approvedProtected": current.approved_protected
	});
	let _ = save_revision(
		&state,
		"settings",
		"global",
		&session.email,
		body,
		current.require_approval,
	);
	let _ = record_audit(
		&state,
		"policy",
		&session.email,
		"",
		"",
		"",
		"allow",
		"settings updated",
	);
	Redirect::to("/settings").into_response()
}

async fn use_cases_csv(State(state): State<AppState>, headers: HeaderMap) -> Response {
	if let Err(response) = require(&state, &headers, Permission::ExportPolicy) {
		return response;
	}
	let docs = state
		.inner
		.store
		.lock()
		.expect("db")
		.list_documents("approval")
		.unwrap_or_default();
	let cases = docs
		.iter()
		.filter_map(|doc| serde_json::from_value::<Approval>(doc.body.clone()).ok())
		.map(|approval| approval.use_case)
		.collect::<Vec<_>>();
	(
		[(header::CONTENT_TYPE, "text/csv; charset=utf-8")],
		register::to_csv(&cases),
	)
		.into_response()
}

async fn use_cases_import(
	State(state): State<AppState>,
	headers: HeaderMap,
	body: String,
) -> Response {
	let session = match require(&state, &headers, Permission::ImportPolicy) {
		Ok(session) => session,
		Err(response) => return response,
	};
	let cases = match register::from_csv(&body) {
		Ok(cases) => cases,
		Err(error) => return (StatusCode::BAD_REQUEST, error.to_string()).into_response(),
	};
	for case in cases {
		let approval = Approval {
			id: magpie::new_id(),
			kind: ItemKind::Tool,
			name: case.name.clone(),
			business_owner: case.owner_name.clone(),
			purpose: case.description.clone(),
			classification: "OFFICIAL".into(),
			expires_on: if case.next_review.is_empty() {
				"2027-06-01".into()
			} else {
				case.next_review.clone()
			},
			status: ApprovalStatus::Pending,
			use_case: case,
		};
		if magpie::approvals::validate(&approval).is_ok() {
			let id = approval.id.clone();
			let _ = save_revision(
				&state,
				"approval",
				&id,
				&session.email,
				serde_json::to_value(&approval).unwrap_or(json!({})),
				false,
			);
		}
	}
	let _ = record_audit(
		&state,
		"approval",
		&session.email,
		"",
		"",
		"",
		"allow",
		"use-case register imported",
	);
	StatusCode::NO_CONTENT.into_response()
}

async fn export_docs(State(state): State<AppState>, headers: HeaderMap) -> Response {
	if let Err(response) = require(&state, &headers, Permission::ExportPolicy) {
		return response;
	}
	let mut docs = state
		.inner
		.store
		.lock()
		.expect("db")
		.export_documents()
		.unwrap_or_default();
	docs.retain(|doc| {
		matches!(
			doc.kind.as_str(),
			"policy" | "dlp_rule" | "approval" | "provider" | "agent" | "argument_rule"
		)
	});
	Json(json!({"example": state.inner.config.demo, "documents": docs})).into_response()
}

async fn import_docs(
	State(state): State<AppState>,
	headers: HeaderMap,
	Json(body): Json<Value>,
) -> Response {
	let session = match require(&state, &headers, Permission::ImportPolicy) {
		Ok(session) => session,
		Err(response) => return response,
	};
	let Some(items) = body.get("documents").and_then(|value| value.as_array()) else {
		return (StatusCode::BAD_REQUEST, "expected documents").into_response();
	};
	if items.iter().any(|item| {
		!matches!(
			item.get("kind").and_then(Value::as_str),
			Some("policy" | "dlp_rule" | "approval" | "provider" | "agent" | "argument_rule")
		)
	}) {
		return (
			StatusCode::BAD_REQUEST,
			"Only policy, DLP, approval, provider, agent and argument-rule documents can be imported.",
		)
			.into_response();
	}
	let store = state.inner.store.lock().expect("db");
	for item in items {
		if let Ok(document) = serde_json::from_value::<Document>(item.clone()) {
			store.put_document(&document).ok();
		}
	}
	drop(store);
	let _ = record_audit(
		&state,
		"policy",
		&session.email,
		"",
		"",
		"",
		"allow",
		"gitops import",
	);
	StatusCode::NO_CONTENT.into_response()
}

async fn guard_request(
	State(state): State<AppState>,
	headers: HeaderMap,
	Json(body): Json<PromptWebhook>,
) -> Response {
	let text = guardrail::join_messages(&body.body.messages);
	let outcome = run_decision(&state, &headers, &text, Phase::Request, None).await;
	Json(guardrail::webhook_response(
		&outcome,
		&body.body.messages,
		false,
	))
	.into_response()
}

async fn guard_response(
	State(state): State<AppState>,
	headers: HeaderMap,
	Json(body): Json<ResponseWebhook>,
) -> Response {
	let messages: Vec<WebhookMessage> = body
		.body
		.choices
		.into_iter()
		.map(|choice| choice.message)
		.collect();
	let text = guardrail::join_messages(&messages);
	let outcome = run_decision(&state, &headers, &text, Phase::Response, None).await;
	Json(guardrail::webhook_response(&outcome, &messages, true)).into_response()
}

async fn mcp_check(
	State(state): State<AppState>,
	headers: HeaderMap,
	Json(body): Json<McpCheckRequest>,
) -> Response {
	let text = body
		.result_text
		.clone()
		.unwrap_or_else(|| body.arguments.clone().unwrap_or(json!({})).to_string());
	let phase = if body.phase == "response" {
		Phase::Response
	} else {
		Phase::Request
	};
	let mut extra = HeaderMap::new();
	extra.insert(
		"x-magpie-agent",
		body
			.agent_id
			.parse()
			.unwrap_or(header::HeaderValue::from_static("agent")),
	);
	if let Some(tool) = &body.tool
		&& let Ok(value) = tool.parse()
	{
		extra.insert("x-magpie-tool", value);
	}
	if let Some(model) = &body.model
		&& let Ok(value) = model.parse()
	{
		extra.insert("x-magpie-model", value);
	}
	if let Ok(value) = body.destination_region.parse() {
		extra.insert("x-magpie-region", value);
	}
	if let Ok(value) = body.destination_name.parse() {
		extra.insert("x-magpie-destination", value);
	}
	if let Ok(value) = body.actor.parse() {
		extra.insert("x-magpie-actor", value);
	}
	let _ = headers;
	let outcome = run_decision(&state, &extra, &text, phase, body.arguments.as_ref()).await;
	Json(guardrail::mcp_response(&outcome)).into_response()
}

async fn run_decision(
	state: &AppState,
	headers: &HeaderMap,
	text: &str,
	phase: Phase,
	arguments: Option<&Value>,
) -> magpie::guardrail::Outcome {
	let settings = load_settings(state);
	let rules = load_rules(state);
	let presidio = if settings.presidio_analyzer_url.is_empty()
		|| !dlp::detector_enabled(&rules, dlp::HitKind::Presidio)
	{
		Vec::new()
	} else {
		match fetch_presidio(&settings.presidio_analyzer_url, text).await {
			Ok(entities) => entities,
			Err(error) => {
				let actor = header_str(headers, "x-magpie-actor").unwrap_or("gateway");
				let _ = record_audit(
					state,
					"dlp",
					actor,
					"",
					"",
					"",
					"log",
					&format!("Presidio did not respond ({error}). In-process recognisers still ran."),
				);
				Vec::new()
			},
		}
	};
	let approvals = load_approvals(state);
	let kill = load_kill(state);
	let metadata = metadata_from(headers);
	let tool = header_str(headers, "x-magpie-tool");
	let model = header_str(headers, "x-magpie-model");
	let input = Inspect {
		text,
		metadata: &metadata,
		agent_id: header_str(headers, "x-magpie-agent").unwrap_or("unknown"),
		tool,
		model,
		destination_region: header_str(headers, "x-magpie-region").unwrap_or(""),
		destination_name: header_str(headers, "x-magpie-destination").unwrap_or("unknown"),
		arguments,
		phase,
		now: Utc::now(),
		actor: header_str(headers, "x-magpie-actor").unwrap_or("gateway"),
	};
	let argument_rules = load_argument_rules(state);
	let policy = PolicySet {
		mode: settings.guardrail_mode,
		rules: &rules,
		approvals: &approvals,
		approved_protected: &settings.approved_protected,
		sovereign_mode: settings.sovereign_mode,
		kill: &kill,
		limiter: Some(&state.inner.limiter),
		argument_rules: &argument_rules,
		presidio: &presidio,
		token_key: state.inner.config.session_secret.as_bytes(),
		scan_injection: phase == Phase::Response || tool.is_some(),
		extra_injection: &[],
	};
	let outcome = guardrail::decide(&input, &policy);
	let _ = state
		.inner
		.store
		.lock()
		.expect("db")
		.append_audit(&outcome.audit);
	outcome
}

async fn fetch_presidio(url: &str, text: &str) -> Result<Vec<PresidioEntity>, String> {
	let client = reqwest::Client::builder()
		.timeout(Duration::from_secs(2))
		.build()
		.map_err(|error| error.to_string())?;
	let response = client
		.post(url)
		.json(&presidio::analyzer_request(text))
		.send()
		.await
		.map_err(|error| error.to_string())?;
	if !response.status().is_success() {
		return Err(format!("HTTP {}", response.status()));
	}
	let body = response.text().await.map_err(|error| error.to_string())?;
	presidio::parse_analyzer_response(&body).map_err(|error| error.to_string())
}

/// Guardrail and MCP check calls come from the proxy, which sends the shared token.
async fn webhook_auth(
	State(state): State<AppState>,
	request: axum::extract::Request,
	next: axum::middleware::Next,
) -> Response {
	let expected = &state.inner.config.webhook_token;
	let presented = header_str(request.headers(), "x-magpie-webhook-token").unwrap_or("");
	if !expected.is_empty() && !identity::constant_time_eq(presented.as_bytes(), expected.as_bytes())
	{
		return (StatusCode::UNAUTHORIZED, "webhook token required").into_response();
	}
	next.run(request).await
}

#[allow(clippy::result_large_err)]
fn scim_auth(state: &AppState, headers: &HeaderMap) -> Result<(), Response> {
	let presented = headers
		.get(header::AUTHORIZATION)
		.and_then(|value| value.to_str().ok())
		.and_then(|value| value.strip_prefix("Bearer "));
	if state.inner.config.scim_token.is_empty()
		|| !identity::constant_time_eq(
			presented.unwrap_or("").as_bytes(),
			state.inner.config.scim_token.as_bytes(),
		) {
		return Err((StatusCode::UNAUTHORIZED, "SCIM bearer token required").into_response());
	}
	Ok(())
}

async fn scim_config(State(state): State<AppState>, headers: HeaderMap) -> Response {
	if let Err(response) = scim_auth(&state, &headers) {
		return response;
	}
	Json(scim::service_provider_config()).into_response()
}

#[derive(Deserialize)]
struct ScimQuery {
	#[serde(default)]
	filter: String,
	#[serde(default, rename = "startIndex")]
	start_index: usize,
	#[serde(default)]
	count: usize,
}

async fn scim_list_users(
	State(state): State<AppState>,
	headers: HeaderMap,
	Query(query): Query<ScimQuery>,
) -> Response {
	if let Err(response) = scim_auth(&state, &headers) {
		return response;
	}
	let filter = if query.filter.is_empty() {
		headers
			.get("x-scim-filter")
			.and_then(|value| value.to_str().ok())
			.map(str::to_string)
	} else {
		Some(query.filter)
	};
	let start = if query.start_index == 0 {
		1
	} else {
		query.start_index
	};
	let count = if query.count == 0 { 100 } else { query.count };
	Json(
		state
			.inner
			.directory
			.lock()
			.expect("scim")
			.list_users(filter.as_deref(), start, count),
	)
	.into_response()
}

async fn scim_create_user(
	State(state): State<AppState>,
	headers: HeaderMap,
	Json(body): Json<Value>,
) -> Response {
	if let Err(response) = scim_auth(&state, &headers) {
		return response;
	}
	match state
		.inner
		.directory
		.lock()
		.expect("scim")
		.create_user(body)
	{
		Ok(user) => (StatusCode::CREATED, Json(user)).into_response(),
		Err(error) => (StatusCode::BAD_REQUEST, error.to_string()).into_response(),
	}
}

async fn scim_get_user(
	State(state): State<AppState>,
	headers: HeaderMap,
	Path(id): Path<String>,
) -> Response {
	if let Err(response) = scim_auth(&state, &headers) {
		return response;
	}
	match state
		.inner
		.directory
		.lock()
		.expect("scim")
		.get_user(&id)
		.cloned()
	{
		Some(user) => Json(user).into_response(),
		None => StatusCode::NOT_FOUND.into_response(),
	}
}

async fn scim_patch_user(
	State(state): State<AppState>,
	headers: HeaderMap,
	Path(id): Path<String>,
	Json(body): Json<Value>,
) -> Response {
	if let Err(response) = scim_auth(&state, &headers) {
		return response;
	}
	match state
		.inner
		.directory
		.lock()
		.expect("scim")
		.patch_user(&id, &body)
	{
		Ok(user) => Json(user).into_response(),
		Err(error) => (StatusCode::BAD_REQUEST, error.to_string()).into_response(),
	}
}

async fn scim_delete_user(
	State(state): State<AppState>,
	headers: HeaderMap,
	Path(id): Path<String>,
) -> Response {
	if let Err(response) = scim_auth(&state, &headers) {
		return response;
	}
	match state.inner.directory.lock().expect("scim").delete_user(&id) {
		Ok(()) => StatusCode::NO_CONTENT.into_response(),
		Err(error) => (StatusCode::NOT_FOUND, error.to_string()).into_response(),
	}
}

async fn scim_list_groups(State(state): State<AppState>, headers: HeaderMap) -> Response {
	if let Err(response) = scim_auth(&state, &headers) {
		return response;
	}
	Json(
		state
			.inner
			.directory
			.lock()
			.expect("scim")
			.list_groups(1, 100),
	)
	.into_response()
}

async fn scim_create_group(
	State(state): State<AppState>,
	headers: HeaderMap,
	Json(body): Json<Value>,
) -> Response {
	if let Err(response) = scim_auth(&state, &headers) {
		return response;
	}
	match state
		.inner
		.directory
		.lock()
		.expect("scim")
		.create_group(body)
	{
		Ok(group) => (StatusCode::CREATED, Json(group)).into_response(),
		Err(error) => (StatusCode::BAD_REQUEST, error.to_string()).into_response(),
	}
}

async fn scim_get_group(
	State(state): State<AppState>,
	headers: HeaderMap,
	Path(id): Path<String>,
) -> Response {
	if let Err(response) = scim_auth(&state, &headers) {
		return response;
	}
	match state
		.inner
		.directory
		.lock()
		.expect("scim")
		.get_group(&id)
		.cloned()
	{
		Some(group) => Json(group).into_response(),
		None => StatusCode::NOT_FOUND.into_response(),
	}
}

async fn scim_patch_group(
	State(state): State<AppState>,
	headers: HeaderMap,
	Path(id): Path<String>,
	Json(body): Json<Value>,
) -> Response {
	if let Err(response) = scim_auth(&state, &headers) {
		return response;
	}
	match state
		.inner
		.directory
		.lock()
		.expect("scim")
		.patch_group(&id, &body)
	{
		Ok(group) => Json(group).into_response(),
		Err(error) => (StatusCode::BAD_REQUEST, error.to_string()).into_response(),
	}
}

async fn scim_delete_group(
	State(state): State<AppState>,
	headers: HeaderMap,
	Path(id): Path<String>,
) -> Response {
	if let Err(response) = scim_auth(&state, &headers) {
		return response;
	}
	match state
		.inner
		.directory
		.lock()
		.expect("scim")
		.delete_group(&id)
	{
		Ok(()) => StatusCode::NO_CONTENT.into_response(),
		Err(error) => (StatusCode::NOT_FOUND, error.to_string()).into_response(),
	}
}

#[derive(Clone)]
struct Settings {
	guardrail_mode: guardrail::Mode,
	sovereign_mode: bool,
	telemetry_enabled: bool,
	presidio_analyzer_url: String,
	retention_days: u32,
	approved_protected: Vec<String>,
	require_approval: bool,
	kill_global: bool,
	kill_agents: String,
}

fn load_settings(state: &AppState) -> Settings {
	let store = state.inner.store.lock().expect("db");
	let guardrail_mode = store.guardrail_mode().ok().flatten().unwrap_or_default();
	let body = store
		.get_document("settings", "global")
		.ok()
		.flatten()
		.map(|doc| doc.body)
		.unwrap_or_else(|| json!({}));
	let kill: KillSwitch = store
		.get_document("kill", "global")
		.ok()
		.flatten()
		.and_then(|doc| serde_json::from_value(doc.body).ok())
		.unwrap_or_default();
	drop(store);
	Settings {
		guardrail_mode,
		sovereign_mode: body
			.get("sovereignMode")
			.and_then(|value| value.as_bool())
			.unwrap_or(true),
		telemetry_enabled: body
			.get("telemetryEnabled")
			.and_then(|value| value.as_bool())
			.unwrap_or(false),
		presidio_analyzer_url: body
			.get("presidioAnalyzerUrl")
			.and_then(|value| value.as_str())
			.unwrap_or("")
			.to_string(),
		retention_days: body
			.get("retentionDays")
			.and_then(|value| value.as_u64())
			.unwrap_or(365) as u32,
		approved_protected: body
			.get("approvedProtected")
			.and_then(|value| value.as_array())
			.map(|items| {
				items
					.iter()
					.filter_map(|item| item.as_str().map(str::to_string))
					.collect()
			})
			.unwrap_or_else(|| vec!["example-au-model".into()]),
		require_approval: body
			.get("requireApproval")
			.and_then(|value| value.as_bool())
			.unwrap_or(true),
		kill_global: kill.global,
		kill_agents: kill.agents.join(", "),
	}
}

fn load_rules(state: &AppState) -> Vec<DlpRule> {
	let docs = state
		.inner
		.store
		.lock()
		.expect("db")
		.list_documents("dlp_rule")
		.unwrap_or_default();
	let mut rules = dlp::default_rules();
	for doc in docs.into_iter().filter(|doc| doc.active) {
		if let Ok(rule) = serde_json::from_value::<DlpRule>(doc.body)
			&& let Some(existing) = rules.iter_mut().find(|r| r.kind == rule.kind)
		{
			*existing = rule;
		}
	}
	rules
}

fn load_approvals(state: &AppState) -> Vec<Approval> {
	state
		.inner
		.store
		.lock()
		.expect("db")
		.list_documents("approval")
		.unwrap_or_default()
		.iter()
		.filter_map(|doc| serde_json::from_value(doc.body.clone()).ok())
		.collect()
}

fn load_argument_rules(state: &AppState) -> Vec<ArgumentRule> {
	state
		.inner
		.store
		.lock()
		.expect("db")
		.list_documents("argument_rule")
		.unwrap_or_default()
		.iter()
		.filter_map(|doc| serde_json::from_value(doc.body.clone()).ok())
		.collect()
}

fn load_kill(state: &AppState) -> KillSwitch {
	state
		.inner
		.store
		.lock()
		.expect("db")
		.get_document("kill", "global")
		.ok()
		.flatten()
		.and_then(|doc| serde_json::from_value(doc.body).ok())
		.unwrap_or_default()
}

fn save_revision(
	state: &AppState,
	kind: &str,
	id: &str,
	actor: &str,
	after: Value,
	require_approval: bool,
) -> magpie::Result<magpie::change::RevisionStatus> {
	let store = state.inner.store.lock().expect("db");
	let existing = store.get_document(kind, id)?;
	let before = existing
		.as_ref()
		.map(|doc| doc.body.clone())
		.unwrap_or(json!({}));
	let next_revision = existing.as_ref().map(|doc| doc.revision + 1).unwrap_or(1);
	let revision = magpie::change::propose(
		kind,
		id,
		actor,
		before,
		after.clone(),
		require_approval,
		Utc::now(),
	);
	store.append_revision(&revision)?;
	if revision.status == magpie::change::RevisionStatus::Active {
		store.put_document(&Document {
			kind: kind.into(),
			id: id.into(),
			body: after,
			revision: next_revision,
			updated_at: Utc::now().to_rfc3339(),
			updated_by: actor.into(),
			active: true,
		})?;
	}
	Ok(revision.status)
}

#[allow(clippy::too_many_arguments)]
fn record_audit(
	state: &AppState,
	kind: &str,
	actor: &str,
	agent: &str,
	tool: &str,
	model: &str,
	decision: &str,
	reason: &str,
) -> magpie::Result<()> {
	let event = AuditEvent::new(kind, actor, agent, tool, model, decision, reason, json!({}));
	state.inner.store.lock().expect("db").append_audit(&event)
}

fn seed(store: &Store) -> magpie::Result<()> {
	let now = Utc::now().to_rfc3339();
	let put = |kind: &str, id: &str, body: Value| {
		store.put_document(&Document {
			kind: kind.into(),
			id: id.into(),
			body,
			revision: 1,
			updated_at: now.clone(),
			updated_by: "seed".into(),
			active: true,
		})
	};
	put(
		"settings",
		"global",
		json!({"sovereignMode": true, "telemetryEnabled": false, "presidioAnalyzerUrl": "", "capturePayloads": false, "retentionDays": 365, "requireApproval": true, "approvedProtected": ["example-au-model"]}),
	)?;
	put(
		"provider",
		"example-au-model",
		json!({"name": "example-au-model", "region": "ap-southeast-2", "kind": "model"}),
	)?;
	put(
		"provider",
		"example-offshore",
		json!({"name": "example-offshore", "region": "us-east-1", "kind": "model"}),
	)?;
	put(
		"agent",
		"example-briefing",
		json!({"name": "example-briefing-agent", "owner": "Alex Nguyen", "tools": ["example_search"], "models": ["example-au-model"]}),
	)?;
	put(
		"tool",
		"example_search",
		json!({"server": "example-notes", "name": "example_search", "status": "pending", "classification": "OFFICIAL"}),
	)?;
	let approval = Approval {
		id: "approval-example".into(),
		kind: ItemKind::Tool,
		name: "example_search".into(),
		business_owner: "Casey Morgan".into(),
		purpose: "Example only. Search approved briefing notes.".into(),
		classification: "OFFICIAL".into(),
		expires_on: "2027-06-01".into(),
		status: ApprovalStatus::Pending,
		use_case: UseCase::example(),
	};
	put(
		"approval",
		"approval-example",
		serde_json::to_value(&approval)?,
	)?;
	let model = Approval {
		id: "approval-model".into(),
		kind: ItemKind::Model,
		name: "example-au-model".into(),
		business_owner: "Casey Morgan".into(),
		purpose: "Example hosted model.".into(),
		classification: "OFFICIAL".into(),
		expires_on: "2027-06-01".into(),
		status: ApprovalStatus::Approved,
		use_case: UseCase::example(),
	};
	put("approval", "approval-model", serde_json::to_value(&model)?)?;
	put("kill", "global", json!({"global": false, "agents": []}))?;
	store.append_audit(&AuditEvent::new(
		"decision",
		"alex.nguyen@example.gov.au",
		"example-briefing-agent",
		"example_search",
		"example-au-model",
		"block",
		"Example only. Tool was not approved yet.",
		json!({"hits": []}),
	))?;
	Ok(())
}

/// Record the deployment's models once. Later portal edits are kept.
/// PROTECTED destinations are merged on every start, because a database
/// created before that list existed would otherwise keep blocking them.
fn bootstrap(
	store: &Store,
	models: &[(String, String)],
	owner: &str,
	protected: &[String],
) -> magpie::Result<()> {
	let now = Utc::now();
	let expires = (now + chrono::Duration::days(90))
		.format("%Y-%m-%d")
		.to_string();
	let owner = if owner.is_empty() {
		"Deployment owner"
	} else {
		owner
	};
	for (name, region) in models {
		let put = |kind: &str, id: &str, body: Value| -> magpie::Result<()> {
			if store.get_document(kind, id)?.is_some() {
				return Ok(());
			}
			store.put_document(&Document {
				kind: kind.into(),
				id: id.into(),
				body,
				revision: 1,
				updated_at: now.to_rfc3339(),
				updated_by: "deployment".into(),
				active: true,
			})
		};
		put(
			"provider",
			name,
			json!({"name": name, "region": region, "kind": "model"}),
		)?;
		let id = format!("approval-model-{name}");
		if store.get_document("approval", &id)?.is_some() {
			continue;
		}
		let approval = Approval {
			id: id.clone(),
			kind: ItemKind::Model,
			name: name.clone(),
			business_owner: owner.into(),
			purpose: "Approved in the deployment settings for the gateway rollout. Review before expiry."
				.into(),
			classification: "OFFICIAL".into(),
			expires_on: expires.clone(),
			status: ApprovalStatus::Approved,
			use_case: UseCase {
				name: "AI coding assistants through Magpie Gateway".into(),
				reference: "MAGPIE-UC-001".into(),
				description: "Developers use coding agents through the gateway.".into(),
				technology: "Generative AI".into(),
				lifecycle: "Pilot".into(),
				technical_standard: "partially applied".into(),
				domains: "Corporate and shared services".into(),
				usage_patterns: "Internal productivity".into(),
				owner_name: owner.into(),
				owner_email: if owner.contains('@') {
					owner.into()
				} else {
					"owner@invalid".into()
				},
				criteria: String::new(),
				inherent_risk: String::new(),
				residual_risk: String::new(),
				impact_assessment_date: String::new(),
				last_review: now.format("%Y-%m-%d").to_string(),
				next_review: expires.clone(),
			},
		};
		put("approval", &id, serde_json::to_value(&approval)?)?;
		store.append_audit(&AuditEvent::new(
			"approval",
			"deployment",
			"",
			"",
			name,
			"allow",
			&format!("Model approved from deployment settings until {expires}, region {region}."),
			json!({}),
		))?;
	}
	merge_protected(store, protected)?;
	Ok(())
}

fn merge_protected(store: &Store, protected: &[String]) -> magpie::Result<()> {
	if protected.is_empty() {
		return Ok(());
	}
	let existing = store.get_document("settings", "global")?;
	let mut body = existing
		.as_ref()
		.map(|doc| doc.body.clone())
		.unwrap_or_else(|| {
			json!({
				"sovereignMode": true,
				"telemetryEnabled": false,
				"presidioAnalyzerUrl": "",
				"capturePayloads": false,
				"retentionDays": 365,
				"requireApproval": true
			})
		});
	let mut list = body
		.get("approvedProtected")
		.and_then(|value| value.as_array())
		.cloned()
		.unwrap_or_default();
	let mut changed = existing.is_none() || body.get("approvedProtected").is_none();
	for name in protected {
		if name.is_empty() || list.iter().any(|item| item.as_str() == Some(name)) {
			continue;
		}
		list.push(json!(name));
		changed = true;
	}
	if !changed {
		return Ok(());
	}
	body["approvedProtected"] = json!(list);
	let revision = existing.as_ref().map(|doc| doc.revision + 1).unwrap_or(1);
	store.put_document(&Document {
		kind: "settings".into(),
		id: "global".into(),
		body,
		revision,
		updated_at: Utc::now().to_rfc3339(),
		updated_by: "deployment".into(),
		active: true,
	})
}

fn placeholder_approval(id: &str) -> Approval {
	Approval {
		id: id.into(),
		kind: ItemKind::Tool,
		name: id.into(),
		business_owner: String::new(),
		purpose: String::new(),
		classification: String::new(),
		expires_on: "2027-01-01".into(),
		status: ApprovalStatus::Pending,
		use_case: UseCase::example(),
	}
}

fn table(headers: &[&str], rows: &[Vec<String>]) -> String {
	let head = headers
		.iter()
		.map(|header| format!("<th scope=\"col\">{}</th>", ui::esc(header)))
		.collect::<String>();
	let body = rows
		.iter()
		.map(|row| {
			let cells = row
				.iter()
				.map(|cell| {
					if cell.starts_with("<form") {
						format!("<td>{cell}</td>")
					} else {
						format!("<td>{}</td>", ui::esc(cell))
					}
				})
				.collect::<String>();
			format!("<tr>{cells}</tr>")
		})
		.collect::<String>();
	format!(
		"<div class=\"table-scroll\" tabindex=\"0\" role=\"region\" aria-label=\"Data table\"><table><thead><tr>{head}</tr></thead><tbody>{body}</tbody></table></div>"
	)
}

fn rows_audit(events: &[AuditEvent]) -> Vec<Vec<String>> {
	events
		.iter()
		.map(|event| {
			vec![
				event.at.to_rfc3339(),
				event.actor.clone(),
				event.agent_id.clone(),
				event.model.clone(),
				event.tool.clone(),
				event.decision.clone(),
				event.reason.clone(),
			]
		})
		.collect()
}

fn rows_audit_full(events: &[AuditEvent]) -> Vec<Vec<String>> {
	events
		.iter()
		.map(|event| {
			vec![
				event.at.to_rfc3339(),
				event.kind.clone(),
				event.actor.clone(),
				event.agent_id.clone(),
				event.model.clone(),
				event.tool.clone(),
				event.decision.clone(),
				event.reason.clone(),
			]
		})
		.collect()
}

fn selected(on: bool) -> &'static str {
	if on { "selected" } else { "" }
}

fn blank_to_none(value: &str) -> Option<String> {
	let trimmed = value.trim();
	if trimmed.is_empty() {
		None
	} else {
		Some(trimmed.to_string())
	}
}

fn second_person(state: &AppState) -> bool {
	load_settings(state).require_approval
}

fn tally_table(events: &[AuditEvent], field: impl Fn(&AuditEvent) -> &str, label: &str) -> String {
	let mut counts: Vec<(String, usize)> = Vec::new();
	for event in events {
		let name = field(event);
		if name.is_empty() {
			continue;
		}
		if let Some(found) = counts.iter_mut().find(|(existing, _)| existing == name) {
			found.1 += 1;
		} else {
			counts.push((name.to_string(), 1));
		}
	}
	counts.sort_by_key(|item| std::cmp::Reverse(item.1));
	counts.truncate(5);
	let rows = counts
		.iter()
		.map(|(name, count)| vec![name.clone(), count.to_string()])
		.collect::<Vec<_>>();
	table(&[label, "Events"], &rows)
}

fn pending_html(state: &AppState) -> String {
	let revisions = state
		.inner
		.store
		.lock()
		.expect("db")
		.list_pending_revisions()
		.unwrap_or_default();
	if revisions.is_empty() {
		return "<h2>Pending changes</h2><p>No change is waiting for a second person.</p>".into();
	}
	let rows = revisions
		.iter()
		.map(|revision| {
			let diff = magpie::change::summarise_diff(&revision.before, &revision.after).join("; ");
			vec![
				revision.kind.clone(),
				revision.doc_id.clone(),
				revision.actor.clone(),
				diff,
				format!(
					"<form method=\"post\" action=\"/revisions/{}/approve\"><button type=\"submit\">Approve</button></form>",
					ui::esc(&revision.id)
				),
			]
		})
		.collect::<Vec<_>>();
	format!(
		"<h2>Pending changes</h2><p>The person who wrote a change cannot approve it.</p>{}",
		table(&["Kind", "Document", "Author", "Diff", "Action"], &rows)
	)
}

fn mock_claims(who: &str, nonce: &str, audience: &str) -> Option<IdClaims> {
	let (sub, email, name, groups) = match who {
		"alex" => (
			"example-user",
			"alex.nguyen@example.gov.au",
			"Alex Nguyen",
			vec![
				"magpie-approvers".into(),
				"magpie-policy-editors".into(),
				"magpie-auditors".into(),
			],
		),
		"sam" => (
			"example-admin",
			"sam.patel@example.gov.au",
			"Sam Patel",
			vec!["magpie-admins".into()],
		),
		_ => return None,
	};
	Some(IdClaims {
		sub: sub.into(),
		email: Some(email.into()),
		name: Some(name.into()),
		preferred_username: None,
		groups,
		roles: vec![],
		nonce: Some(nonce.into()),
		iss: "mock".into(),
		aud: json!(audience),
		exp: (Utc::now() + chrono::Duration::minutes(5)).timestamp() as usize,
		events: json!({}),
	})
}

fn public_base(headers: &HeaderMap) -> String {
	let host = headers
		.get(header::HOST)
		.and_then(|value| value.to_str().ok())
		.unwrap_or("127.0.0.1");
	let scheme = headers
		.get("x-forwarded-proto")
		.and_then(|value| value.to_str().ok())
		.filter(|value| *value == "https")
		.unwrap_or("http");
	format!("{scheme}://{host}")
}

fn session_cookie(value: &str, secure: bool) -> String {
	let secure = if secure { "; Secure" } else { "" };
	format!("magpie_session={value}; HttpOnly; SameSite=Lax; Path=/{secure}; Max-Age=43200")
}

fn read_cookie(headers: &HeaderMap, name: &str) -> Option<String> {
	let raw = headers.get(header::COOKIE)?.to_str().ok()?;
	for part in raw.split(';') {
		let mut pair = part.trim().splitn(2, '=');
		if pair.next() == Some(name) {
			return pair.next().map(|value| value.to_string());
		}
	}
	None
}

fn header_str<'a>(headers: &'a HeaderMap, name: &str) -> Option<&'a str> {
	headers.get(name).and_then(|value| value.to_str().ok())
}

fn metadata_from(headers: &HeaderMap) -> std::collections::BTreeMap<String, String> {
	let mut map = std::collections::BTreeMap::new();
	if let Some(value) = header_str(headers, "x-magpie-marking") {
		map.insert("classification".into(), value.to_string());
	}
	map
}

#[derive(Deserialize)]
struct CallbackQuery {
	code: Option<String>,
	state: Option<String>,
	error: Option<String>,
	error_description: Option<String>,
}

#[cfg(test)]
mod tests {
	use super::*;

	#[test]
	fn bootstrap_allows_deployment_models_for_protected_traffic() {
		let store = Store::open_memory().unwrap();
		let destination = "bedrock:au.anthropic.claude-opus-4-6-v1";
		bootstrap(
			&store,
			&[("claude-opus-4-6".into(), "ap-southeast-2".into())],
			"owner@example.com",
			&[destination.into()],
		)
		.unwrap();
		let doc = store.get_document("settings", "global").unwrap().unwrap();
		let list = doc.body["approvedProtected"].as_array().unwrap();
		assert_eq!(list.len(), 1);
		assert_eq!(list[0], destination);
		assert!(
			store
				.get_document("approval", "approval-model-claude-opus-4-6")
				.unwrap()
				.is_some()
		);

		bootstrap(
			&store,
			&[("claude-opus-4-6".into(), "ap-southeast-2".into())],
			"owner@example.com",
			&[
				destination.into(),
				"bedrock:au.anthropic.claude-sonnet-4-6".into(),
			],
		)
		.unwrap();
		let doc = store.get_document("settings", "global").unwrap().unwrap();
		let list = doc.body["approvedProtected"].as_array().unwrap();
		assert_eq!(list.len(), 2);
		assert_eq!(doc.revision, 2);
	}
}
