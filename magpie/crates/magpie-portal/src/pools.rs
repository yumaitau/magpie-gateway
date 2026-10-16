//! Authenticated subscription pooling, with native provider wire contracts.
use super::*;
use axum::body::Body;
use base64::{Engine, engine::general_purpose::URL_SAFE_NO_PAD};
use futures_util::{Stream, StreamExt};
use magpie::accounts::token_hash;
use magpie::dlp::Action;
use magpie::pools::{Pool, PoolKey, Subscription};
use ring::{
	aead,
	rand::{SecureRandom, SystemRandom},
};
use serde::Serialize;
use sha2::{Digest, Sha256};
use std::{
	pin::Pin,
	task::{Context, Poll},
};

#[cfg(test)]
mod tests;

pub(super) fn routes() -> Router<AppState> {
	Router::new()
		.route("/api/magpie/pools", get(list).post(change))
		.route("/pool/v1/models", get(models))
		.route("/pool/v1/models/{id}", get(model))
		.route("/pool/v1/messages", post(messages))
		.route("/pool/v1/messages/count_tokens", post(count_tokens))
		.route("/pool/v1/responses", post(responses))
}

fn private(value: Value) -> Response {
	([(header::CACHE_CONTROL, "no-store")], Json(value)).into_response()
}
fn fail(status: StatusCode, message: &str) -> Response {
	(
		status,
		[(header::CACHE_CONTROL, "no-store")],
		Json(json!({"error":message})),
	)
		.into_response()
}
fn api_error(status: StatusCode, kind: &str, message: &str) -> Response {
	(
		status,
		[(header::CACHE_CONTROL, "no-store")],
		Json(json!({
			"type":"error", "error":{"type":kind,"code":kind,"message":message}
		})),
	)
		.into_response()
}

#[allow(clippy::result_large_err)]
fn signed_in(state: &AppState, headers: &HeaderMap) -> Result<SessionRecord, Response> {
	let session = session_from(state, headers)
		.map(|s| s.0)
		.map_err(|_| fail(StatusCode::UNAUTHORIZED, "Sign in required."))?;
	if !can_use(state, &session) {
		return Err(fail(
			StatusCode::FORBIDDEN,
			"Catalogue access or gateway provider/key management permission is required.",
		));
	}
	Ok(session)
}

pub(super) fn can_use(state: &AppState, session: &SessionRecord) -> bool {
	[
		Permission::ReadCatalogue,
		Permission::EditGatewayProviders,
		Permission::EditGatewayKeys,
	]
	.into_iter()
	.any(|p| access::allows(state, session, p))
}

async fn list(State(state): State<AppState>, headers: HeaderMap) -> Response {
	let session = match signed_in(&state, &headers) {
		Ok(s) => s,
		Err(r) => return r,
	};
	let manage = access::allows(&state, &session, Permission::EditGatewayProviders);
	let manage_keys = access::allows(&state, &session, Permission::EditGatewayKeys);
	let store = state.inner.store.lock().expect("db");
	let result = (|| -> magpie::Result<Value> {
		let mut pools = Vec::new();
		for pool in store
			.subscription_pools()?
			.into_iter()
			.filter(|p| p.visible_to(&session.subject) || manage)
		{
			let members: Vec<Value> = store.pool_members(&pool.id, Utc::now().timestamp())?.into_iter().map(|m| {
				let s = m.subscription;
				json!({"id":s.id,"name":s.name,"provider":s.provider,"models":s.models,"enabled":s.enabled,
					"maxParallel":s.max_parallel,"active":m.active,"status":m.status,"cooldownUntil":m.cooldown_until})
			}).collect();
			let keys: Vec<_> = store
				.pool_keys(&pool.id)?
				.into_iter()
				.filter(|k| k.owner == session.subject || manage_keys)
				.collect();
			let can_manage = manage && pool.owner == session.subject;
			pools.push(
				json!({"pool":pool,"subscriptions":members,"keys":keys,"canManage":can_manage,
				"canIssue":pool.visible_to(&session.subject),"canManageKeys":manage_keys}),
			);
		}
		Ok(json!({"pools":pools,"canCreate":manage}))
	})();
	match result {
		Ok(value) => private(value),
		Err(_) => fail(StatusCode::INTERNAL_SERVER_ERROR, "Could not load pools."),
	}
}

#[derive(Deserialize)]
#[serde(
	tag = "action",
	rename_all = "camelCase",
	rename_all_fields = "camelCase"
)]
enum Change {
	SavePool {
		id: Option<String>,
		name: String,
		shared: bool,
		enabled: bool,
	},
	SaveSubscription {
		pool_id: String,
		id: Option<String>,
		name: String,
		provider: String,
		models: Vec<String>,
		credential: Option<String>,
		enabled: bool,
		max_parallel: u32,
	},
	RemoveSubscription {
		pool_id: String,
		id: String,
	},
	IssueKey {
		pool_id: String,
		name: String,
		models: Vec<String>,
		expires_in_days: u32,
	},
	RevokeKey {
		pool_id: String,
		id: String,
	},
}

async fn change(
	State(state): State<AppState>,
	headers: HeaderMap,
	Json(change): Json<Change>,
) -> Response {
	let session = match signed_in(&state, &headers) {
		Ok(s) => s,
		Err(r) => return r,
	};
	if !accounts::same_origin(&state, &headers) {
		return fail(StatusCode::FORBIDDEN, "Same-origin request required.");
	}
	let manage = access::allows(&state, &session, Permission::EditGatewayProviders);
	let manage_keys = access::allows(&state, &session, Permission::EditGatewayKeys);
	let store = state.inner.store.lock().expect("db");
	let result = mutate(&state, &store, &session, manage, manage_keys, change);
	match result {
		Ok(v) => private(v),
		Err(r) => r,
	}
}

#[allow(clippy::result_large_err)]
fn mutate(
	state: &AppState,
	store: &Store,
	session: &SessionRecord,
	manage: bool,
	manage_keys: bool,
	change: Change,
) -> Result<Value, Response> {
	let db_error = |_| {
		fail(
			StatusCode::INTERNAL_SERVER_ERROR,
			"Could not save the pool change. A subscription can belong to only one pool.",
		)
	};
	let bad = |message| fail(StatusCode::BAD_REQUEST, message);
	let owned_pool = |id: &str| -> Result<Pool, Response> {
		let pool = store
			.subscription_pool(id)
			.map_err(db_error)?
			.ok_or_else(|| fail(StatusCode::NOT_FOUND, "Pool not found."))?;
		if !manage || pool.owner != session.subject {
			return Err(fail(
				StatusCode::FORBIDDEN,
				"Only the pool owner with provider-edit permission can change subscriptions.",
			));
		}
		Ok(pool)
	};
	match change {
		Change::SavePool {
			id,
			name,
			shared,
			enabled,
		} => {
			if !manage {
				return Err(fail(
					StatusCode::FORBIDDEN,
					"Provider-edit permission required.",
				));
			}
			valid_name(&name).map_err(bad)?;
			let pool = if let Some(id) = id {
				let mut pool = owned_pool(&id)?;
				pool.name = name.trim().into();
				pool.shared = shared;
				pool.enabled = enabled;
				pool
			} else {
				Pool {
					id: magpie::new_id(),
					name: name.trim().into(),
					owner: session.subject.clone(),
					shared,
					enabled,
				}
			};
			store
				.save_subscription_pool(&pool, &session.subject)
				.map_err(db_error)?;
			Ok(json!({"id":pool.id}))
		},
		Change::SaveSubscription {
			pool_id,
			id,
			name,
			provider,
			models,
			credential,
			enabled,
			max_parallel,
		} => {
			owned_pool(&pool_id)?;
			valid_name(&name).map_err(bad)?;
			valid_models(&models).map_err(bad)?;
			if !matches!(provider.as_str(), "claude" | "codex")
				|| !(1..=10).contains(&max_parallel)
				|| (provider == "codex" && max_parallel != 1)
			{
				return Err(bad(
					"Use Claude (1–10 parallel requests) or Codex (one parallel request).",
				));
			}
			let existing = if let Some(id) = &id {
				Some(
					store
						.pool_members(&pool_id, Utc::now().timestamp())
						.map_err(db_error)?
						.into_iter()
						.find(|m| m.subscription.id == *id)
						.ok_or_else(|| bad("Subscription not found."))?
						.subscription,
				)
			} else {
				None
			};
			let id = id.unwrap_or_else(magpie::new_id);
			let credential = credential.filter(|v| !v.trim().is_empty());
			let reconnect = credential.is_some();
			if reconnect
				&& existing.as_ref().is_some_and(|s| {
					store
						.pool_members(&pool_id, Utc::now().timestamp())
						.ok()
						.is_none_or(|members| {
							members
								.iter()
								.any(|m| m.subscription.id == s.id && m.active > 0)
						})
				}) {
				return Err(bad(
					"Wait for active requests to finish before reconnecting credentials.",
				));
			}
			let credentials = if let Some(raw) = credential {
				parse_credential(&provider, &raw).map_err(bad)?
			} else if let Some(s) = &existing {
				if s.provider != provider {
					return Err(bad("Reconnect credentials when changing provider."));
				}
				decrypt(state, &id, &s.credential)
					.map_err(|_| bad("Reconnect this subscription; its credentials cannot be opened."))?
			} else {
				return Err(bad("Subscription credentials required."));
			};
			let fingerprint = token_hash(&format!("{}:{}", provider, credentials.identity()));
			let encrypted = if !reconnect {
				existing.as_ref().unwrap().credential.clone()
			} else {
				encrypt(state, &id, &credentials)
					.map_err(|_| bad("Could not protect subscription credentials."))?
			};
			let subscription = Subscription {
				id: id.clone(),
				pool_id,
				name: name.trim().into(),
				provider,
				models,
				credential: encrypted,
				enabled,
				max_parallel,
			};
			store
				.save_pool_subscription(&subscription, &fingerprint, &session.subject)
				.map_err(db_error)?;
			Ok(json!({"id":id}))
		},
		Change::RemoveSubscription { pool_id, id } => {
			owned_pool(&pool_id)?;
			store
				.remove_pool_subscription(&id, &pool_id, &session.subject)
				.map_err(db_error)?;
			Ok(json!({"ok":true}))
		},
		Change::IssueKey {
			pool_id,
			name,
			models,
			expires_in_days,
		} => {
			valid_name(&name).map_err(bad)?;
			valid_models(&models).map_err(bad)?;
			let pool = store
				.subscription_pool(&pool_id)
				.map_err(db_error)?
				.ok_or_else(|| bad("Pool not found."))?;
			if !pool.enabled || !pool.visible_to(&session.subject) {
				return Err(fail(
					StatusCode::FORBIDDEN,
					"This pool is not available to you.",
				));
			}
			if !(1..=365).contains(&expires_in_days) {
				return Err(bad("Key lifetime must be 1–365 days."));
			}
			let members = store
				.pool_members(&pool_id, Utc::now().timestamp())
				.map_err(db_error)?;
			if !models.iter().all(|model| {
				members
					.iter()
					.any(|m| m.subscription.enabled && m.subscription.models.contains(model))
			}) {
				return Err(bad(
					"Choose models exposed by enabled subscriptions in this pool.",
				));
			}
			if store
				.pool_keys(&pool_id)
				.map_err(db_error)?
				.iter()
				.filter(|k| {
					k.owner == session.subject && !k.revoked && k.expires_at > Utc::now().timestamp()
				})
				.count()
				>= 100
			{
				return Err(bad(
					"Revoke an existing key before issuing another (100 active keys per pool).",
				));
			}
			let mut bytes = [0u8; 32];
			SystemRandom::new()
				.fill(&mut bytes)
				.map_err(|_| bad("Could not generate key."))?;
			let token = format!("mp_pool_{}", URL_SAFE_NO_PAD.encode(bytes));
			let key = PoolKey {
				id: magpie::new_id(),
				pool_id,
				owner: session.subject.clone(),
				name: name.trim().into(),
				models,
				expires_at: Utc::now().timestamp() + i64::from(expires_in_days) * 86400,
				revoked: false,
			};
			store
				.issue_pool_key(&key, &token, &session.subject)
				.map_err(db_error)?;
			Ok(json!({"key":key,"token":token}))
		},
		Change::RevokeKey { pool_id, id } => {
			let key = store
				.pool_keys(&pool_id)
				.map_err(db_error)?
				.into_iter()
				.find(|k| k.id == id)
				.ok_or_else(|| bad("Key not found."))?;
			if key.owner != session.subject && !manage_keys {
				return Err(fail(
					StatusCode::FORBIDDEN,
					"You can revoke only your own keys.",
				));
			}
			store
				.revoke_pool_key(key, &session.subject)
				.map_err(db_error)?;
			Ok(json!({"ok":true}))
		},
	}
}

fn valid_name(name: &str) -> Result<(), &'static str> {
	if name.trim().is_empty() || name.len() > 100 {
		Err("Use a name of 1–100 bytes.")
	} else {
		Ok(())
	}
}
fn valid_models(models: &[String]) -> Result<(), &'static str> {
	if models.is_empty()
		|| models.len() > 100
		|| models
			.iter()
			.collect::<std::collections::HashSet<_>>()
			.len()
			!= models.len()
		|| models.iter().any(|m| {
			m.is_empty()
				|| m.len() > 150
				|| !m
					.bytes()
					.all(|c| c.is_ascii_alphanumeric() || b"-._/:".contains(&c))
		}) {
		Err(
			"Choose 1–100 explicit model IDs (letters, numbers, dots, hyphens, underscores, slashes and colons).",
		)
	} else {
		Ok(())
	}
}

#[derive(Clone, Serialize, Deserialize)]
struct Credentials {
	access: String,
	#[serde(default)]
	refresh: String,
	#[serde(default)]
	account: String,
	#[serde(default)]
	expires: i64,
}
impl Credentials {
	fn identity(&self) -> &str {
		if self.account.is_empty() {
			&self.access
		} else {
			&self.account
		}
	}
}
fn jwt_payload(token: &str) -> Option<Value> {
	serde_json::from_slice(&URL_SAFE_NO_PAD.decode(token.split('.').nth(1)?).ok()?).ok()
}
fn parse_credential(provider: &str, raw: &str) -> Result<Credentials, &'static str> {
	if raw.len() > 32000 {
		return Err("Credentials are too large.");
	}
	if provider == "claude" {
		let access = raw.trim();
		if !access.starts_with("sk-ant-oat")
			|| access.len() < 30
			|| !access
				.bytes()
				.all(|c| c.is_ascii_alphanumeric() || b"-_".contains(&c))
		{
			return Err("Paste a Claude subscription token from claude setup-token.");
		}
		return Ok(Credentials {
			access: access.into(),
			refresh: String::new(),
			account: String::new(),
			expires: 0,
		});
	}
	let auth: Value = serde_json::from_str(raw)
		.map_err(|_| "Paste the Codex auth.json from a ChatGPT subscription login.")?;
	if auth["auth_mode"] != "chatgpt" || auth.get("OPENAI_API_KEY").is_some_and(|v| !v.is_null()) {
		return Err("Use a ChatGPT subscription login, with auth_mode chatgpt.");
	}
	let tokens = &auth["tokens"];
	let access = tokens["access_token"]
		.as_str()
		.filter(|s| !s.is_empty())
		.ok_or("Missing Codex access token.")?;
	let refresh = tokens["refresh_token"]
		.as_str()
		.filter(|s| !s.is_empty())
		.ok_or("Missing Codex refresh token.")?;
	let claims = jwt_payload(access).unwrap_or(Value::Null);
	let id_claims = tokens["id_token"]
		.as_str()
		.and_then(jwt_payload)
		.unwrap_or(Value::Null);
	let account = tokens["account_id"]
		.as_str()
		.or_else(|| claims["https://api.openai.com/auth"]["chatgpt_account_id"].as_str())
		.or_else(|| id_claims["https://api.openai.com/auth"]["chatgpt_account_id"].as_str())
		.filter(|s| {
			!s.is_empty()
				&& s.len() < 200
				&& s
					.bytes()
					.all(|b| b.is_ascii_alphanumeric() || b"-_".contains(&b))
		})
		.ok_or("Codex login has no ChatGPT account ID.")?;
	if access.contains(['\r', '\n']) || refresh.contains(['\r', '\n']) {
		return Err("Invalid login tokens.");
	}
	Ok(Credentials {
		access: access.into(),
		refresh: refresh.into(),
		account: account.into(),
		expires: claims["exp"].as_i64().unwrap_or(0),
	})
}

fn cipher(state: &AppState) -> Result<aead::LessSafeKey, ()> {
	let mut digest = Sha256::new();
	digest.update(b"magpie/subscription-credentials/v1\0");
	digest.update(state.inner.config.session_secret.as_bytes());
	let key = aead::UnboundKey::new(&aead::CHACHA20_POLY1305, &digest.finalize()).map_err(|_| ())?;
	Ok(aead::LessSafeKey::new(key))
}
fn encrypt(state: &AppState, id: &str, credentials: &Credentials) -> Result<String, ()> {
	let mut nonce = [0u8; 12];
	SystemRandom::new().fill(&mut nonce).map_err(|_| ())?;
	let mut body = serde_json::to_vec(credentials).map_err(|_| ())?;
	cipher(state)?
		.seal_in_place_append_tag(
			aead::Nonce::assume_unique_for_key(nonce),
			aead::Aad::from(id.as_bytes()),
			&mut body,
		)
		.map_err(|_| ())?;
	let mut sealed = nonce.to_vec();
	sealed.extend(body);
	Ok(URL_SAFE_NO_PAD.encode(sealed))
}
fn decrypt(state: &AppState, id: &str, sealed: &str) -> Result<Credentials, ()> {
	let bytes = URL_SAFE_NO_PAD.decode(sealed).map_err(|_| ())?;
	if bytes.len() < 28 {
		return Err(());
	}
	let nonce: [u8; 12] = bytes[..12].try_into().map_err(|_| ())?;
	let mut body = bytes[12..].to_vec();
	let opened = cipher(state)?
		.open_in_place(
			aead::Nonce::assume_unique_for_key(nonce),
			aead::Aad::from(id.as_bytes()),
			&mut body,
		)
		.map_err(|_| ())?;
	serde_json::from_slice(opened).map_err(|_| ())
}

#[allow(clippy::result_large_err)]
fn authenticate(state: &AppState, headers: &HeaderMap) -> Result<(PoolKey, Pool), Response> {
	let bearer = headers
		.get(header::AUTHORIZATION)
		.and_then(|h| h.to_str().ok())
		.and_then(|h| h.split_once(' '))
		.filter(|(scheme, _)| scheme.eq_ignore_ascii_case("Bearer"))
		.map(|(_, token)| token);
	let api_key = headers.get("x-api-key").and_then(|h| h.to_str().ok());
	if bearer.is_some() && api_key.is_some() && bearer != api_key {
		return Err(api_error(
			StatusCode::UNAUTHORIZED,
			"authentication_error",
			"Conflicting API keys.",
		));
	}
	let Some(token) = bearer.or(api_key).filter(|t| t.starts_with("mp_pool_")) else {
		return Err(api_error(
			StatusCode::UNAUTHORIZED,
			"authentication_error",
			"A subscription pool API key is required.",
		));
	};
	state
		.inner
		.store
		.lock()
		.expect("db")
		.authenticate_pool_key(token, Utc::now().timestamp())
		.map_err(|_| {
			api_error(
				StatusCode::INTERNAL_SERVER_ERROR,
				"api_error",
				"Could not authenticate key.",
			)
		})?
		.ok_or_else(|| {
			api_error(
				StatusCode::UNAUTHORIZED,
				"authentication_error",
				"Invalid, revoked, expired or unavailable pool key.",
			)
		})
}

fn model_list(state: &AppState, key: &PoolKey) -> magpie::Result<Vec<Value>> {
	let members = state
		.inner
		.store
		.lock()
		.expect("db")
		.pool_members(&key.pool_id, Utc::now().timestamp())?;
	Ok(key.models.iter().filter(|model| members.iter().any(|m| m.subscription.enabled && m.subscription.models.contains(model)))
		.map(|model| json!({"id":model,"object":"model","type":"model","display_name":model,"created":0,"created_at":"1970-01-01T00:00:00Z","owned_by":"subscription-pool"})).collect())
}
async fn models(State(state): State<AppState>, headers: HeaderMap) -> Response {
	let (key, _) = match authenticate(&state, &headers) {
		Ok(v) => v,
		Err(r) => return r,
	};
	match model_list(&state, &key) {
		Ok(data) => private(
			json!({"object":"list","first_id":data.first().map(|m| &m["id"]),"last_id":data.last().map(|m| &m["id"]),"has_more":false,"data":data}),
		),
		Err(_) => api_error(
			StatusCode::INTERNAL_SERVER_ERROR,
			"api_error",
			"Could not load models.",
		),
	}
}
async fn model(
	State(state): State<AppState>,
	Path(id): Path<String>,
	headers: HeaderMap,
) -> Response {
	let (key, _) = match authenticate(&state, &headers) {
		Ok(v) => v,
		Err(r) => return r,
	};
	match model_list(&state, &key) {
		Ok(models) => models
			.into_iter()
			.find(|m| m["id"] == id)
			.map(private)
			.unwrap_or_else(|| {
				api_error(
					StatusCode::NOT_FOUND,
					"not_found_error",
					"Model is not available to this key.",
				)
			}),
		Err(_) => api_error(
			StatusCode::INTERNAL_SERVER_ERROR,
			"api_error",
			"Could not load models.",
		),
	}
}

async fn messages(
	State(state): State<AppState>,
	headers: HeaderMap,
	input: Result<Json<Value>, axum::extract::rejection::JsonRejection>,
) -> Response {
	let body = match api_input(input) {
		Ok(v) => v,
		Err(r) => return r,
	};
	forward(
		state,
		headers,
		body,
		"claude",
		false,
		"https://api.anthropic.com/v1/messages?beta=true",
	)
	.await
}
async fn count_tokens(
	State(state): State<AppState>,
	headers: HeaderMap,
	input: Result<Json<Value>, axum::extract::rejection::JsonRejection>,
) -> Response {
	let body = match api_input(input) {
		Ok(v) => v,
		Err(r) => return r,
	};
	forward(
		state,
		headers,
		body,
		"claude",
		true,
		"https://api.anthropic.com/v1/messages/count_tokens?beta=true",
	)
	.await
}
async fn responses(
	State(state): State<AppState>,
	headers: HeaderMap,
	input: Result<Json<Value>, axum::extract::rejection::JsonRejection>,
) -> Response {
	let body = match api_input(input) {
		Ok(v) => v,
		Err(r) => return r,
	};
	forward(
		state,
		headers,
		body,
		"codex",
		false,
		"https://chatgpt.com/backend-api/codex/responses",
	)
	.await
}

#[allow(clippy::result_large_err)]
fn api_input(
	input: Result<Json<Value>, axum::extract::rejection::JsonRejection>,
) -> Result<Value, Response> {
	input.map(|Json(v)| v).map_err(|e| {
		api_error(
			if e.status() == StatusCode::PAYLOAD_TOO_LARGE {
				StatusCode::PAYLOAD_TOO_LARGE
			} else {
				StatusCode::BAD_REQUEST
			},
			"invalid_request_error",
			"Send a valid JSON request body with Content-Type application/json (at most 2 MB).",
		)
	})
}

/// A lease lasts until EOF, transport failure or client disconnect, including SSE.
struct Lease {
	state: AppState,
	id: String,
}
impl Drop for Lease {
	fn drop(&mut self) {
		let _ = self
			.state
			.inner
			.store
			.lock()
			.expect("db")
			.release_pool_lease(&self.id);
	}
}
struct LeasedStream<S> {
	stream: Pin<Box<S>>,
	_lease: Lease,
}
impl<S: Stream> Stream for LeasedStream<S> {
	type Item = S::Item;
	fn poll_next(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Option<Self::Item>> {
		self.stream.as_mut().poll_next(cx)
	}
}

enum RefreshError {
	Temporary,
	Reauthenticate,
}
async fn refresh(
	state: &AppState,
	subscription: &mut Subscription,
	credentials: &mut Credentials,
) -> Result<(), RefreshError> {
	refresh_at(
		state,
		subscription,
		credentials,
		"https://auth.openai.com/oauth/token",
	)
	.await
}

async fn refresh_at(
	state: &AppState,
	subscription: &mut Subscription,
	credentials: &mut Credentials,
	url: &str,
) -> Result<(), RefreshError> {
	// The subscription has only one Codex lease, serialising rotating refresh tokens.
	let response = state.inner.gateway_client.post(url)
		.timeout(Duration::from_secs(20))
		.json(&json!({"grant_type":"refresh_token","client_id":"app_EMoamEEZ73f0CkXaXp7hrann","refresh_token":credentials.refresh}))
		.send().await.map_err(|_| RefreshError::Temporary)?;
	if response.status().is_client_error() && response.status() != StatusCode::TOO_MANY_REQUESTS {
		return Err(RefreshError::Reauthenticate);
	}
	if !response.status().is_success() {
		return Err(RefreshError::Temporary);
	}
	let body: Value = response.json().await.map_err(|_| RefreshError::Temporary)?;
	credentials.access = body["access_token"]
		.as_str()
		.filter(|s| !s.is_empty())
		.ok_or(RefreshError::Temporary)?
		.into();
	if let Some(token) = body["refresh_token"].as_str().filter(|s| !s.is_empty()) {
		credentials.refresh = token.into()
	}
	credentials.expires = jwt_payload(&credentials.access)
		.and_then(|v| v["exp"].as_i64())
		.unwrap_or(Utc::now().timestamp() + body["expires_in"].as_i64().unwrap_or(3600));
	let old = subscription.credential.clone();
	subscription.credential =
		encrypt(state, &subscription.id, credentials).map_err(|_| RefreshError::Temporary)?;
	if !state
		.inner
		.store
		.lock()
		.expect("db")
		.refresh_pool_credential(subscription, &old)
		.map_err(|_| RefreshError::Temporary)?
	{
		return Err(RefreshError::Temporary);
	}
	Ok(())
}

fn governance_headers(key: &PoolKey, provider: &str, model: &str) -> HeaderMap {
	let mut headers = HeaderMap::new();
	for (name, value) in [
		("x-magpie-actor", key.owner.as_str()),
		("x-magpie-agent", key.id.as_str()),
		("x-magpie-model", model),
		("x-magpie-region", "global"),
		("x-magpie-destination", provider),
	] {
		if let Ok(value) = value.parse() {
			headers.insert(name, value);
		}
	}
	headers
}

async fn forward(
	state: AppState,
	headers: HeaderMap,
	mut body: Value,
	provider: &str,
	count: bool,
	url: &str,
) -> Response {
	let (key, _) = match authenticate(&state, &headers) {
		Ok(v) => v,
		Err(r) => return r,
	};
	let Some(model) = body["model"].as_str().map(str::to_string) else {
		return api_error(
			StatusCode::BAD_REQUEST,
			"invalid_request_error",
			"A model ID is required.",
		);
	};
	if !key.models.contains(&model) {
		return api_error(
			StatusCode::FORBIDDEN,
			"permission_error",
			"This model is not allowed by the pool key.",
		);
	}
	let members = match state
		.inner
		.store
		.lock()
		.expect("db")
		.pool_members(&key.pool_id, Utc::now().timestamp())
	{
		Ok(m) => m,
		Err(_) => {
			return api_error(
				StatusCode::INTERNAL_SERVER_ERROR,
				"api_error",
				"Could not load pool.",
			);
		},
	};
	if !members.iter().any(|m| {
		m.subscription.enabled
			&& m.subscription.provider == provider
			&& m.subscription.models.contains(&model)
	}) {
		return api_error(
			StatusCode::BAD_REQUEST,
			"invalid_request_error",
			"This model is not exposed by an enabled subscription using this API contract.",
		);
	}
	let stream = match body.get("stream") {
		None => false,
		Some(Value::Bool(b)) => *b,
		_ => {
			return api_error(
				StatusCode::BAD_REQUEST,
				"invalid_request_error",
				"stream must be a boolean.",
			);
		},
	};
	// Inspect the entire input, including tool arguments, images' metadata and system instructions.
	// Identity and destination are derived from the key, never client-supplied headers.
	let mut governance = governance_headers(&key, provider, &model);
	if let Some(marking) = headers.get("x-magpie-marking") {
		governance.insert("x-magpie-marking", marking.clone());
	}
	if !governance.contains_key("x-magpie-actor") {
		return api_error(
			StatusCode::FORBIDDEN,
			"permission_error",
			"The key owner identity cannot be used for governance.",
		);
	}
	let outcome = run_decision(
		&state,
		&governance,
		&inspection_text(&body),
		Phase::Request,
		None,
	)
	.await;
	if !matches!(outcome.action, Action::Allow | Action::Log) {
		return api_error(
			StatusCode::FORBIDDEN,
			"permission_error",
			"Magpie governance blocked this request. Ask an administrator to review the audit log.",
		);
	}
	if provider == "codex" {
		if body["store"] == true
			|| body["background"] == true
			|| !body["previous_response_id"].is_null()
			|| !body["conversation"].is_null()
			|| ["max_output_tokens", "temperature", "top_p"]
				.iter()
				.any(|f| body.get(*f).is_some_and(|v| !v.is_null()))
		{
			return api_error(
				StatusCode::BAD_REQUEST,
				"invalid_request_error",
				"Subscription Responses supports stateless requests. Omit store:true, background:true, previous_response_id, conversation, max_output_tokens, temperature and top_p.",
			);
		}
		// ChatGPT's subscription backend requires an ephemeral, streamed Responses request.
		body["store"] = json!(false);
		body["stream"] = json!(true);
		if body.get("instructions").is_none_or(Value::is_null) {
			body["instructions"] = json!("")
		}
		if let Some(input) = body["input"].as_str() {
			body["input"] = json!([{"role":"user","content":[{"type":"input_text","text":input}]}]);
		}
	}
	let mut tried = Vec::new();
	let mut permission_denied = false;
	'accounts: loop {
		let claimed = state
			.inner
			.store
			.lock()
			.expect("db")
			.claim_pool_subscription(
				&key.pool_id,
				provider,
				&model,
				&tried,
				Utc::now().timestamp(),
			);
		let (mut subscription, lease) = match claimed {
			Ok(Some(v)) => v,
			Ok(None) => {
				if permission_denied {
					return api_error(
						StatusCode::FORBIDDEN,
						"permission_error",
						"Connected subscriptions did not allow this model. Ask the pool owner to review model access.",
					);
				}
				let mut response = api_error(
					StatusCode::TOO_MANY_REQUESTS,
					"rate_limit_error",
					"No subscription is currently available for this model and API contract.",
				);
				response
					.headers_mut()
					.insert(header::RETRY_AFTER, "30".parse().unwrap());
				return response;
			},
			Err(_) => {
				return api_error(
					StatusCode::INTERNAL_SERVER_ERROR,
					"api_error",
					"Could not reserve a subscription.",
				);
			},
		};
		let lease = Lease {
			state: state.clone(),
			id: lease,
		};
		tried.push(subscription.id.clone());
		let mut credentials = match decrypt(&state, &subscription.id, &subscription.credential) {
			Ok(c) => c,
			Err(_) => {
				mark(&state, &subscription.id, "reauthenticate", 0);
				continue;
			},
		};
		if provider == "codex"
			&& credentials.expires <= Utc::now().timestamp() + 60
			&& let Err(error) = refresh(&state, &mut subscription, &mut credentials).await
		{
			refresh_failed(&state, &subscription.id, error);
			continue;
		}
		let mut refreshed = false;
		let upstream = loop {
			let mut request = state
				.inner
				.gateway_client
				.post(url)
				.timeout(Duration::from_secs(600))
				.bearer_auth(&credentials.access)
				.json(&body);
			if provider == "claude" {
				request = request.header("anthropic-version", "2023-06-01");
				let beta = headers
					.get("anthropic-beta")
					.and_then(|h| h.to_str().ok())
					.unwrap_or("");
				request = request.header(
					"anthropic-beta",
					format!(
						"oauth-2025-04-20{}{}",
						if beta.is_empty() { "" } else { "," },
						beta
					),
				);
			} else {
				request = request
					.header("chatgpt-account-id", &credentials.account)
					.header("originator", "codex_cli_rs")
					.header(header::ACCEPT, "text/event-stream");
			}
			let result = request.send().await;
			if matches!(&result, Ok(r) if r.status() == StatusCode::UNAUTHORIZED)
				&& provider == "codex"
				&& !refreshed
			{
				refreshed = true;
				match refresh(&state, &mut subscription, &mut credentials).await {
					Ok(()) => continue,
					Err(error) => {
						refresh_failed(&state, &subscription.id, error);
						continue 'accounts;
					},
				}
			}
			break result;
		};
		let upstream = match upstream {
			Ok(r) => r,
			Err(_) => {
				return api_error(
					StatusCode::BAD_GATEWAY,
					"api_error",
					"The subscription provider could not be reached.",
				);
			},
		};
		let status = upstream.status();
		if status == StatusCode::UNAUTHORIZED {
			mark(&state, &subscription.id, "reauthenticate", 0);
			continue;
		}
		if status == StatusCode::FORBIDDEN {
			// Entitlements may differ by model; a 403 must not disable other models.
			permission_denied = true;
			continue;
		}
		let cooldown = cooldown(
			upstream.headers(),
			Utc::now().timestamp(),
			status == StatusCode::TOO_MANY_REQUESTS,
		);
		mark(
			&state,
			&subscription.id,
			if cooldown > 0 { "limited" } else { "ready" },
			cooldown,
		);
		if status == StatusCode::TOO_MANY_REQUESTS {
			continue;
		}
		if !status.is_success() {
			// Provider errors can contain credentials or account details. Return a safe contract-shaped error.
			return api_error(
				if status.is_client_error() {
					status
				} else {
					StatusCode::BAD_GATEWAY
				},
				"api_error",
				"The subscription provider rejected the request.",
			);
		}
		if (provider == "codex" || (stream && !count))
			&& !upstream
				.headers()
				.get(header::CONTENT_TYPE)
				.and_then(|h| h.to_str().ok())
				.is_some_and(|h| h.starts_with("text/event-stream"))
		{
			return api_error(
				StatusCode::BAD_GATEWAY,
				"api_error",
				"The subscription provider did not return an event stream.",
			);
		}
		let _ = state
			.inner
			.store
			.lock()
			.expect("db")
			.append_audit(&AuditEvent::new(
				"subscription-pool",
				&key.owner,
				&key.id,
				"",
				&model,
				"forward",
				"Request assigned to subscription pool.",
				json!({"poolId":key.pool_id,"subscriptionId":subscription.id}),
			));
		if stream && !count {
			let mut response = Response::new(Body::from_stream(LeasedStream {
				stream: Box::pin(guarded_stream(upstream, state.clone(), governance)),
				_lease: lease,
			}));
			response
				.headers_mut()
				.insert(header::CONTENT_TYPE, "text/event-stream".parse().unwrap());
			response
				.headers_mut()
				.insert(header::CACHE_CONTROL, "no-store".parse().unwrap());
			response
				.headers_mut()
				.insert("x-accel-buffering", "no".parse().unwrap());
			return response;
		}
		let output = if provider == "codex" {
			collect_response(upstream).await
		} else {
			collect_json(upstream).await
		};
		let output = match output {
			Ok(v) => v,
			Err(_) => {
				return api_error(
					StatusCode::BAD_GATEWAY,
					"api_error",
					"The subscription provider returned an incomplete response.",
				);
			},
		};
		let outcome = run_decision(
			&state,
			&governance,
			&inspection_text(&output),
			Phase::Response,
			None,
		)
		.await;
		if !matches!(outcome.action, Action::Allow | Action::Log) {
			return api_error(
				StatusCode::FORBIDDEN,
				"permission_error",
				"Magpie governance blocked the provider response. Ask an administrator to review the audit log.",
			);
		}
		return private(output);
	}
}

fn inspection_text(value: &Value) -> String {
	fn collect(value: &Value, text: &mut String) {
		match value {
			Value::String(s) => {
				text.push_str(s);
				text.push('\n');
			},
			Value::Array(values) => {
				for v in values {
					collect(v, text);
				}
			},
			Value::Object(values) => {
				for v in values.values() {
					collect(v, text);
				}
			},
			_ => {},
		}
	}
	let mut text = String::new();
	collect(value, &mut text);
	text
}

fn take_sse_frame(buffer: &mut Vec<u8>) -> Option<Vec<u8>> {
	let end = buffer.windows(2).position(|s| s == b"\n\n").map(|n| n + 2);
	let crlf = buffer
		.windows(4)
		.position(|s| s == b"\r\n\r\n")
		.map(|n| n + 4);
	let end = match (end, crlf) {
		(Some(a), Some(b)) => a.min(b),
		(Some(n), None) | (None, Some(n)) => n,
		_ => return None,
	};
	Some(buffer.drain(..end).collect())
}
fn sse_value(frame: &[u8]) -> Option<Value> {
	let data = frame
		.split(|b| *b == b'\n')
		.filter_map(|line| line.strip_prefix(b"data:"))
		.map(|line| String::from_utf8_lossy(line).trim().to_string())
		.collect::<Vec<_>>()
		.join("\n");
	serde_json::from_str(&data).ok()
}

/// Check complete SSE frames and accumulated deltas before forwarding them.
fn guarded_stream(
	response: reqwest::Response,
	state: AppState,
	headers: HeaderMap,
) -> impl Stream<Item = Result<axum::body::Bytes, std::io::Error>> + Send {
	struct Pending<S> {
		stream: Pin<Box<S>>,
		state: AppState,
		headers: HeaderMap,
		buffer: Vec<u8>,
		text: String,
		audited: std::collections::HashSet<String>,
		total: usize,
		ended: bool,
	}
	let pending = Pending {
		stream: Box::pin(response.bytes_stream()),
		state,
		headers,
		buffer: Vec::new(),
		text: String::new(),
		audited: std::collections::HashSet::new(),
		total: 0,
		ended: false,
	};
	futures_util::stream::unfold(pending, |mut p| async move {
		loop {
			if p.ended {
				return None;
			}
			if let Some(frame) = take_sse_frame(&mut p.buffer) {
				if let Some(event) = sse_value(&frame) {
					let delta = event["delta"]
						.as_str()
						.or_else(|| event["delta"]["text"].as_str())
						.or_else(|| event["delta"]["partial_json"].as_str())
						.or_else(|| event["delta"]["thinking"].as_str());
					if let Some(delta) = delta {
						p.text.push_str(delta);
					}
					let text = if delta.is_some() {
						p.text.clone()
					} else {
						inspection_text(&event)
					};
					if !text.is_empty() {
						let outcome =
							run_decision_recorded(&p.state, &p.headers, &text, Phase::Response, None, false)
								.await;
						// Record a repeated streaming decision once, instead of per token.
						let signature = format!("{}:{}", outcome.audit.detail["wouldAction"], outcome.reason);
						if p.audited.insert(signature) {
							let _ = p
								.state
								.inner
								.store
								.lock()
								.expect("db")
								.append_audit(&outcome.audit);
						}
						if !matches!(outcome.action, Action::Allow | Action::Log) {
							p.ended = true;
							let error = "event: error\ndata: {\"type\":\"error\",\"error\":{\"type\":\"permission_error\",\"code\":\"permission_error\",\"message\":\"Magpie governance blocked the provider response.\"}}\n\n";
							return Some((Ok(axum::body::Bytes::from(error)), p));
						}
					}
				}
				return Some((Ok(axum::body::Bytes::from(frame)), p));
			}
			match p.stream.next().await {
				Some(Ok(bytes)) => {
					p.total += bytes.len();
					if p.total > 32 * 1024 * 1024 {
						p.ended = true;
						return Some((
							Err(std::io::Error::other("Provider stream exceeded 32 MB.")),
							p,
						));
					}
					p.buffer.extend_from_slice(&bytes);
				},
				Some(Err(_)) => {
					p.ended = true;
					return Some((
						Err(std::io::Error::other("Provider stream interrupted.")),
						p,
					));
				},
				None => return None,
			}
		}
	})
}

fn mark(state: &AppState, id: &str, status: &str, cooldown: i64) {
	let _ = state
		.inner
		.store
		.lock()
		.expect("db")
		.pool_subscription_status(id, status, cooldown);
}
fn refresh_failed(state: &AppState, id: &str, error: RefreshError) {
	match error {
		RefreshError::Temporary => mark(state, id, "limited", Utc::now().timestamp() + 60),
		RefreshError::Reauthenticate => mark(state, id, "reauthenticate", 0),
	}
}

fn cooldown(headers: &HeaderMap, now: i64, limited: bool) -> i64 {
	let number = |name: &str| {
		headers
			.get(name)
			.and_then(|v| v.to_str().ok())
			.and_then(|s| s.parse::<f64>().ok())
	};
	let mut reset = 0;
	for window in ["primary", "secondary"] {
		if number(&format!("x-codex-{window}-used-percent")).is_some_and(|n| n >= 100.) {
			reset = reset.max(
				now
					+ number(&format!("x-codex-{window}-reset-after-seconds"))
						.unwrap_or(60.)
						.clamp(1., 604800.) as i64,
			);
		}
	}
	for window in ["5h", "7d"] {
		let status = format!("anthropic-ratelimit-unified-{window}-status");
		if headers.get(status).is_some_and(|h| h == "rejected") {
			reset = reset.max(
				number(&format!("anthropic-ratelimit-unified-{window}-reset"))
					.map(|n| n as i64)
					.unwrap_or(now + 60),
			);
		}
	}
	if headers
		.get("anthropic-ratelimit-unified-status")
		.is_some_and(|h| h == "rejected")
	{
		reset = reset.max(
			number("anthropic-ratelimit-unified-reset")
				.map(|n| n as i64)
				.unwrap_or(now + 60),
		);
	}
	if limited {
		let retry = headers
			.get(header::RETRY_AFTER)
			.and_then(|v| v.to_str().ok())
			.and_then(|s| {
				s.parse::<i64>()
					.ok()
					.map(|n| now + n.clamp(1, 604800))
					.or_else(|| {
						chrono::DateTime::parse_from_rfc2822(s)
							.ok()
							.map(|d| d.timestamp())
					})
			})
			.unwrap_or(now + 60);
		reset = reset.max(retry).max(now + 1);
	}
	reset
}

async fn collect_json(response: reqwest::Response) -> Result<Value, ()> {
	let mut stream = response.bytes_stream();
	let mut buffer = Vec::new();
	while let Some(chunk) = stream.next().await {
		let chunk = chunk.map_err(|_| ())?;
		if buffer.len() + chunk.len() > 32 * 1024 * 1024 {
			return Err(());
		}
		buffer.extend_from_slice(&chunk);
	}
	serde_json::from_slice(&buffer).map_err(|_| ())
}

/// Subscription Responses always stream. For non-stream clients return the final Responses object.
async fn collect_response(response: reqwest::Response) -> Result<Value, ()> {
	let mut stream = response.bytes_stream();
	let mut buffer = Vec::new();
	let mut total = 0usize;
	while let Some(chunk) = stream.next().await {
		let chunk = chunk.map_err(|_| ())?;
		total += chunk.len();
		if total > 32 * 1024 * 1024 {
			return Err(());
		}
		buffer.extend_from_slice(&chunk);
		while let Some(frame) = take_sse_frame(&mut buffer) {
			if let Some(event) = sse_value(&frame) {
				if matches!(
					event["type"].as_str(),
					Some("response.completed" | "response.incomplete")
				) {
					return event.get("response").cloned().ok_or(());
				}
				if matches!(event["type"].as_str(), Some("response.failed" | "error")) {
					return Err(());
				}
			}
		}
	}
	Err(())
}
