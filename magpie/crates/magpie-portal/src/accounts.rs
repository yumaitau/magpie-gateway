use super::*;
use magpie::accounts::{AccountLink, LocalAccount, password_problem};

pub(super) fn routes() -> Router<AppState> {
	Router::new()
		.route("/api/magpie/login-options", get(login_options))
		.route("/api/magpie/login", post(login_json))
		.route("/api/magpie/account", get(account))
		.route("/api/magpie/account/password", post(change_password))
		.route("/api/magpie/people", get(people))
		.route("/api/magpie/invitations", post(invite))
		.route("/api/magpie/password-reset", post(reset))
		.route("/api/magpie/links/{id}/revoke", post(revoke))
		.route("/api/magpie/accept-link", post(accept))
}
fn fail(status: StatusCode, message: &str) -> Response {
	(status, Json(json!({"error":message}))).into_response()
}
fn private(value: Value) -> Response {
	([(header::CACHE_CONTROL, "no-store")], Json(value)).into_response()
}
#[allow(clippy::result_large_err)]
fn signed_in(state: &AppState, headers: &HeaderMap) -> Result<SessionRecord, Response> {
	session_from(state, headers)
		.map(|s| s.0)
		.map_err(|_| fail(StatusCode::UNAUTHORIZED, "Sign in to continue."))
}
pub(super) fn same_origin(state: &AppState, headers: &HeaderMap) -> bool {
	if headers
		.get("sec-fetch-site")
		.is_some_and(|v| v == "cross-site")
	{
		return false;
	}
	let Some(origin) = headers.get(header::ORIGIN) else {
		return true;
	};
	let Ok(origin) = origin.to_str() else {
		return false;
	};
	let expected = if state.inner.config.public_url.is_empty() {
		public_base(headers)
	} else {
		state.inner.config.public_url.clone()
	};
	origin.trim_end_matches('/') == expected.trim_end_matches('/')
}
#[allow(clippy::result_large_err)]
fn mutation_access(
	state: &AppState,
	headers: &HeaderMap,
	admin: bool,
) -> Result<SessionRecord, Response> {
	if !same_origin(state, headers) {
		return Err(fail(
			StatusCode::FORBIDDEN,
			"Cross-origin account changes are refused.",
		));
	}
	let session = signed_in(state, headers)?;
	if admin && !access::allows(state, &session, Permission::ManageAccess) {
		return Err(fail(
			StatusCode::FORBIDDEN,
			"Manage access permission is required.",
		));
	}
	Ok(session)
}
async fn account(State(state): State<AppState>, headers: HeaderMap) -> Response {
	let session = match signed_in(&state, &headers) {
		Ok(s) => s,
		Err(r) => return r,
	};
	let local = state
		.inner
		.store
		.lock()
		.expect("db")
		.local_account(&session.subject)
		.ok()
		.flatten()
		.is_some();
	private(
		json!({"name":session.name,"email":session.email,"local":local,"canManagePeople":access::allows(&state,&session,Permission::ManageAccess)}),
	)
}
#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct PasswordChange {
	current_password: String,
	password: String,
	confirm_password: String,
}
async fn change_password(
	State(state): State<AppState>,
	headers: HeaderMap,
	Json(form): Json<PasswordChange>,
) -> Response {
	let session = match mutation_access(&state, &headers, false) {
		Ok(s) => s,
		Err(r) => return r,
	};
	if form.password != form.confirm_password {
		return fail(StatusCode::BAD_REQUEST, "The new passwords do not match.");
	}
	if let Some(reason) = password_problem(&form.password) {
		return fail(StatusCode::BAD_REQUEST, reason);
	}
	if state
		.inner
		.limiter
		.check(&session.subject, "account-password")
		.is_err()
	{
		return fail(
			StatusCode::TOO_MANY_REQUESTS,
			"Please wait a minute before trying again.",
		);
	}
	let account = state
		.inner
		.store
		.lock()
		.expect("db")
		.local_account(&session.subject)
		.ok()
		.flatten();
	let Some(account) = account else {
		return fail(
			StatusCode::BAD_REQUEST,
			"Your password is managed by your single sign-on provider.",
		);
	};
	let expected = account.password_hash.clone();
	let result = tokio::task::spawn_blocking(move || {
		if !magpie::accounts::verify(&form.current_password, &account.password_hash) {
			return Err(magpie::Error::Unauthorised);
		}
		identity::hash_password(&form.password)
	})
	.await;
	let Ok(Ok(hash)) = result else {
		return fail(
			StatusCode::BAD_REQUEST,
			"The current password is incorrect.",
		);
	};
	if state
		.inner
		.store
		.lock()
		.expect("db")
		.change_local_password(&session.subject, &expected, &hash)
		.is_err()
	{
		return fail(
			StatusCode::CONFLICT,
			"Password changed in another session. Sign in again.",
		);
	}
	let mut response = private(json!({"ok":true}));
	response.headers_mut().insert(
		header::SET_COOKIE,
		"magpie_session=; Path=/; HttpOnly; SameSite=Lax; Max-Age=0"
			.parse()
			.unwrap(),
	);
	response
}
async fn people(State(state): State<AppState>, headers: HeaderMap) -> Response {
	let session = match signed_in(&state, &headers) {
		Ok(s) => s,
		Err(r) => return r,
	};
	if !access::allows(&state, &session, Permission::ManageAccess) {
		return fail(
			StatusCode::FORBIDDEN,
			"Manage access permission is required.",
		);
	}
	let store = state.inner.store.lock().expect("db");
	let users:Vec<_>=store.portal_users().unwrap_or_default().into_iter().map(|u|{
  let local=store.local_account(&u.subject).ok().flatten();
  json!({"subject":u.subject,"name":u.name,"email":u.email,"disabled":u.disabled,"lastLogin":u.last_login,"local":local.is_some(),"canReset":local.is_some_and(|a|!a.recovery)})
 }).collect();
	private(
		json!({"users":users,"groups":store.access_control().ok().flatten().map(|a|a.groups).unwrap_or_default(),"links":store.account_links().unwrap_or_default(),"currentSubject":session.subject}),
	)
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Invite {
	email: String,
	name: String,
	groups: Vec<String>,
}
async fn invite(
	State(state): State<AppState>,
	headers: HeaderMap,
	Json(form): Json<Invite>,
) -> Response {
	let session = match mutation_access(&state, &headers, true) {
		Ok(s) => s,
		Err(r) => return r,
	};
	let email = form.email.trim().to_lowercase();
	let name = form.name.trim();
	if email.len() > 254
		|| email.chars().any(char::is_whitespace)
		|| email.split('@').count() != 2
		|| !email
			.split('@')
			.next_back()
			.is_some_and(|d| d.contains('.') && !d.starts_with('.'))
		|| email.starts_with('@')
		|| name.is_empty()
		|| name.len() > 100
	{
		return fail(
			StatusCode::BAD_REQUEST,
			"Enter a name and a valid email address.",
		);
	}
	let store = state.inner.store.lock().expect("db");
	if store.local_login(&email).ok().flatten().is_some()
		|| store
			.portal_users()
			.unwrap_or_default()
			.iter()
			.any(|u| u.email.eq_ignore_ascii_case(&email))
	{
		return fail(
			StatusCode::CONFLICT,
			"This person already has an account. Manage them in People.",
		);
	}
	let Some(access) = store.access_control().ok().flatten() else {
		return fail(
			StatusCode::INTERNAL_SERVER_ERROR,
			"Could not load user groups.",
		);
	};
	if form.groups.is_empty()
		|| form
			.groups
			.iter()
			.any(|id| !access.groups.iter().any(|g| &g.id == id))
	{
		return fail(
			StatusCode::BAD_REQUEST,
			"Select at least one existing user group.",
		);
	}
	drop(store);
	issue(
		&state,
		&session,
		AccountLink {
			id: identity::start_pkce().state,
			kind: "invite".into(),
			subject: format!("local:{}", identity::start_pkce().state),
			email,
			name: name.into(),
			groups: form.groups,
			expires_at: (Utc::now() + chrono::Duration::hours(24)).to_rfc3339(),
		},
	)
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Reset {
	subject: String,
}
async fn reset(
	State(state): State<AppState>,
	headers: HeaderMap,
	Json(form): Json<Reset>,
) -> Response {
	let session = match mutation_access(&state, &headers, true) {
		Ok(s) => s,
		Err(r) => return r,
	};
	let store = state.inner.store.lock().expect("db");
	let Some(account) = store.local_account(&form.subject).ok().flatten() else {
		return fail(StatusCode::BAD_REQUEST, "This account uses single sign-on.");
	};
	if account.recovery {
		return fail(
			StatusCode::BAD_REQUEST,
			"Use My account to change the recovery administrator password.",
		);
	}
	if store
		.portal_user(&account.subject)
		.ok()
		.flatten()
		.is_some_and(|u| u.disabled)
	{
		return fail(
			StatusCode::BAD_REQUEST,
			"Enable the account before resetting its password.",
		);
	}
	drop(store);
	issue(
		&state,
		&session,
		AccountLink {
			id: identity::start_pkce().state,
			kind: "reset".into(),
			subject: account.subject,
			email: account.email,
			name: account.name,
			groups: vec![],
			expires_at: (Utc::now() + chrono::Duration::hours(1)).to_rfc3339(),
		},
	)
}
fn issue(state: &AppState, session: &SessionRecord, link: AccountLink) -> Response {
	let token = identity::start_pkce().verifier;
	if state
		.inner
		.store
		.lock()
		.expect("db")
		.issue_account_link(&link, &token, &session.subject)
		.is_err()
	{
		return fail(
			StatusCode::INTERNAL_SERVER_ERROR,
			"Could not create the account link.",
		);
	}
	// Relative URL avoids deriving a security-sensitive link from an untrusted Host header.
	let path = if link.kind == "invite" {
		"accept-invitation"
	} else {
		"reset-password"
	};
	private(
		json!({"link":format!("/ui/{path}#{token}"),"expiresAt":link.expires_at,"email":link.email}),
	)
}
async fn revoke(
	State(state): State<AppState>,
	headers: HeaderMap,
	Path(id): Path<String>,
) -> Response {
	let session = match mutation_access(&state, &headers, true) {
		Ok(s) => s,
		Err(r) => return r,
	};
	if state
		.inner
		.store
		.lock()
		.expect("db")
		.revoke_account_link(&id, &session.subject)
		.is_err()
	{
		return fail(
			StatusCode::INTERNAL_SERVER_ERROR,
			"Could not revoke the link.",
		);
	}
	private(json!({"ok":true}))
}
#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct Accept {
	token: String,
	password: String,
	confirm_password: String,
}
async fn accept(
	State(state): State<AppState>,
	headers: HeaderMap,
	Json(form): Json<Accept>,
) -> Response {
	if !same_origin(&state, &headers) {
		return fail(
			StatusCode::FORBIDDEN,
			"Cross-origin account changes are refused.",
		);
	}
	if form.token.len() < 32 || form.token.len() > 128 {
		return fail(StatusCode::BAD_REQUEST, "This link is invalid or expired.");
	}
	if form.password != form.confirm_password {
		return fail(StatusCode::BAD_REQUEST, "The passwords do not match.");
	}
	if let Some(reason) = password_problem(&form.password) {
		return fail(StatusCode::BAD_REQUEST, reason);
	}
	if state.inner.limiter.check("all", "account-accept").is_err() {
		return fail(
			StatusCode::TOO_MANY_REQUESTS,
			"Please wait a minute before trying again.",
		);
	}
	let Ok(Ok(hash)) =
		tokio::task::spawn_blocking(move || identity::hash_password(&form.password)).await
	else {
		return fail(
			StatusCode::INTERNAL_SERVER_ERROR,
			"Could not set the password.",
		);
	};
	if state
		.inner
		.store
		.lock()
		.expect("db")
		.redeem_account_link(&form.token, &hash)
		.is_err()
	{
		return fail(
			StatusCode::BAD_REQUEST,
			"This link is invalid, expired, revoked or already used. Ask your administrator for a new link.",
		);
	}
	private(json!({"ok":true}))
}
pub(super) async fn local_login(state: AppState, headers: HeaderMap, form: GlassForm) -> Response {
	let login = form.username.trim().to_lowercase();
	if login.len() > 254
		|| state
			.inner
			.limiter
			.check("all", "account-login-global")
			.is_err()
	{
		return login_error(&state, "Too many attempts. Please wait a minute.");
	}
	let account = state
		.inner
		.store
		.lock()
		.expect("db")
		.local_login(&login)
		.ok()
		.flatten();
	if state
		.inner
		.limiter
		.check(
			if account.is_some() {
				&login
			} else {
				"unknown-account"
			},
			"account-login",
		)
		.is_err()
	{
		return login_error(&state, "Too many attempts. Please wait a minute.");
	}
	let dummy = state.inner.login_dummy_hash.clone();
	let password = form.password;
	let verified = tokio::task::spawn_blocking(move || {
		let hash = account
			.as_ref()
			.map(|a| a.password_hash.as_str())
			.unwrap_or(&dummy);
		if magpie::accounts::verify(&password, hash) {
			account
		} else {
			None
		}
	})
	.await
	.ok()
	.flatten();
	let Some(account) = verified else {
		let _ = record_audit(
			&state,
			"login",
			&login,
			"",
			"",
			"",
			"block",
			"Local sign-in rejected.",
		);
		return login_error(
			&state,
			"The email or username and password were not recognised.",
		);
	};
	if account.recovery && !state.inner.config.break_glass.enabled {
		return login_error(&state, "Administrator recovery login is disabled.");
	}
	let claims = IdClaims {
		sub: account.subject.clone(),
		email: Some(account.email.clone()),
		name: Some(account.name.clone()),
		preferred_username: None,
		groups: vec![],
		roles: vec![],
		nonce: None,
		iss: "local".into(),
		aud: json!(state.inner.config.audience),
		exp: (Utc::now() + chrono::Duration::hours(1)).timestamp() as usize,
		events: json!({}),
	};
	finish_login(
		&state,
		&headers,
		&claims,
		account.recovery,
		Some(&account.password_hash),
	)
}
pub(super) fn bootstrap(store: &Store, config: &Config) -> magpie::Result<()> {
	if config.break_glass.enabled {
		store.bootstrap_local_account(&LocalAccount {
			subject: config.break_glass.username.clone(),
			login: config.break_glass.username.clone(),
			name: "Administrator".into(),
			email: format!("{}@local.invalid", config.break_glass.username),
			password_hash: config.break_glass.password_hash.clone(),
			recovery: true,
		})?;
	}
	Ok(())
}

async fn login_json(
	State(state): State<AppState>,
	headers: HeaderMap,
	Json(form): Json<GlassForm>,
) -> Response {
	if !same_origin(&state, &headers) {
		return fail(StatusCode::FORBIDDEN, "Cross-origin sign-in is refused.");
	}
	let response = local_login(state, headers, form).await;
	if !response.status().is_redirection() {
		return fail(
			StatusCode::UNAUTHORIZED,
			"The email or username and password were not recognised, or too many attempts were made. Try again shortly.",
		);
	}
	let mut out = private(json!({"ok":true}));
	if let Some(cookie) = response.headers().get(header::SET_COOKIE) {
		out.headers_mut().insert(header::SET_COOKIE, cookie.clone());
	}
	out
}

async fn login_options(State(state): State<AppState>) -> Response {
	private(json!({"sso":state.inner.config.oidc_enabled()}))
}
