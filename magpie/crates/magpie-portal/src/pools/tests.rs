use super::*;
use magpie::{identity::Role, rbac::PortalUser};
use std::sync::atomic::{AtomicUsize, Ordering};

#[tokio::test]
async fn model_permission_failures_do_not_quarantine_the_entire_subscription() {
	let state = test_state();
	add_member(&state, "a", "claude");
	add_member(&state, "b", "claude");
	let url = mock(Router::new().route(
		"/",
		post(|headers: HeaderMap| async move {
			if headers[header::AUTHORIZATION] == "Bearer fixture-a" {
				StatusCode::FORBIDDEN.into_response()
			} else {
				Json(json!({"type":"message","content":[]})).into_response()
			}
		}),
	))
	.await;
	let response = forward(
		state.clone(),
		key_headers(),
		json!({"model":"model","messages":[]}),
		"claude",
		false,
		&url,
	)
	.await;
	assert_eq!(response.status(), StatusCode::OK);
	let members = state
		.inner
		.store
		.lock()
		.unwrap()
		.pool_members("pool", Utc::now().timestamp())
		.unwrap();
	assert_eq!(
		members
			.iter()
			.find(|m| m.subscription.id == "a")
			.unwrap()
			.status,
		"ready"
	);
}

#[tokio::test]
async fn refresh_rotates_credentials_without_overwriting_subscription_edits() {
	let state = test_state();
	add_member(&state, "a", "codex");
	let mut subscription = state
		.inner
		.store
		.lock()
		.unwrap()
		.pool_members("pool", Utc::now().timestamp())
		.unwrap()
		.remove(0)
		.subscription;
	let mut credentials = decrypt(&state, "a", &subscription.credential).unwrap();
	credentials.refresh = "fixture-old-refresh".into();
	let mut edited = subscription.clone();
	edited.name = "Renamed while request was running".into();
	state
		.inner
		.store
		.lock()
		.unwrap()
		.save_pool_subscription(&edited, "a", "owner")
		.unwrap();
	let url = mock(Router::new().route("/",post(|Json(input): Json<Value>| async move {
		assert_eq!(input["grant_type"],"refresh_token");
		assert_eq!(input["refresh_token"],"fixture-old-refresh");
		Json(json!({"access_token":"fixture-new-access","refresh_token":"fixture-rotated-refresh","expires_in":3600}))
	}))).await;
	assert!(
		refresh_at(&state, &mut subscription, &mut credentials, &url)
			.await
			.is_ok()
	);
	let current = state
		.inner
		.store
		.lock()
		.unwrap()
		.pool_members("pool", Utc::now().timestamp())
		.unwrap()
		.remove(0)
		.subscription;
	assert_eq!(current.name, edited.name);
	let opened = decrypt(&state, "a", &current.credential).unwrap();
	assert_eq!(opened.access, "fixture-new-access");
	assert_eq!(opened.refresh, "fixture-rotated-refresh");
	assert!(opened.expires > Utc::now().timestamp());
	let transient =
		mock(Router::new().route("/", post(|| async { StatusCode::SERVICE_UNAVAILABLE }))).await;
	assert!(matches!(
		refresh_at(&state, &mut subscription, &mut credentials, &transient).await,
		Err(RefreshError::Temporary)
	));
	let rejected = mock(Router::new().route("/", post(|| async { StatusCode::UNAUTHORIZED }))).await;
	assert!(matches!(
		refresh_at(&state, &mut subscription, &mut credentials, &rejected).await,
		Err(RefreshError::Reauthenticate)
	));
}

pub(crate) fn test_state() -> AppState {
	let state = crate::state(Config {
		gateway_admin_url: String::new(),
		gateway_ui_dir: String::new(),
		guardrail_mode: guardrail::Mode::LogOnly,
		database_path: ":memory:".into(),
		cookie_secure: false,
		demo: false,
		seed_examples: false,
		mock_idp: true,
		oidc_kind: OidcProviderKind::Generic,
		issuer: String::new(),
		client_id: "test".into(),
		client_secret: String::new(),
		audience: "test".into(),
		public_url: String::new(),
		role_map: Default::default(),
		webhook_token: String::new(),
		bootstrap_models: vec![("model".into(), "global".into())],
		bootstrap_protected: vec![],
		bootstrap_owner: "owner".into(),
		hmac_secret: "test-only".into(),
		session_secret: "test-only-secret".into(),
		scim_token: String::new(),
		break_glass: Default::default(),
		idle: Duration::from_secs(3600),
		absolute: Duration::from_secs(86400),
		listen: "127.0.0.1:0".into(),
	})
	.unwrap();
	let store = state.inner.store.lock().unwrap();
	for subject in ["owner", "colleague", "outsider"] {
		store
			.remember_portal_user(&PortalUser {
				subject: subject.into(),
				name: subject.into(),
				email: String::new(),
				last_login: String::new(),
				disabled: false,
			})
			.unwrap();
	}
	store
		.save_subscription_pool(
			&Pool {
				id: "pool".into(),
				name: "Team".into(),
				owner: "owner".into(),
				shared: true,
				enabled: true,
			},
			"owner",
		)
		.unwrap();
	store
		.issue_pool_key(
			&PoolKey {
				id: "key".into(),
				pool_id: "pool".into(),
				owner: "colleague".into(),
				name: "Coding".into(),
				models: vec!["model".into()],
				expires_at: Utc::now().timestamp() + 3600,
				revoked: false,
			},
			"mp_pool_test",
			"colleague",
		)
		.unwrap();
	drop(store);
	state
}
fn session(state: &AppState, subject: &str, roles: Vec<Role>) -> (SessionRecord, HeaderMap) {
	let groups = roles
		.iter()
		.map(|role| format!("Magpie.{}", role.as_str().replace(' ', "")))
		.collect();
	let (mut session, _) = state
		.inner
		.codec
		.issue(subject, "", "Test", roles, Utc::now(), false)
		.unwrap();
	session.identity_groups = groups;
	let (session, cookie) = state.inner.codec.touch(session, Utc::now()).unwrap();
	state
		.inner
		.store
		.lock()
		.unwrap()
		.put_session(
			&session.session_id,
			subject,
			&serde_json::to_string(&session).unwrap(),
			&session.absolute_expiry.to_rfc3339(),
		)
		.unwrap();
	let mut headers = HeaderMap::new();
	headers.insert(
		header::COOKIE,
		format!("magpie_session={cookie}").parse().unwrap(),
	);
	(session, headers)
}
fn add_member(state: &AppState, id: &str, provider: &str) {
	let credentials = Credentials {
		access: format!("fixture-{id}"),
		refresh: String::new(),
		account: "test-account".into(),
		expires: Utc::now().timestamp() + 3600,
	};
	state
		.inner
		.store
		.lock()
		.unwrap()
		.save_pool_subscription(
			&Subscription {
				id: id.into(),
				pool_id: "pool".into(),
				name: id.into(),
				provider: provider.into(),
				models: vec!["model".into()],
				enabled: true,
				max_parallel: 1,
				credential: encrypt(state, id, &credentials).unwrap(),
			},
			id,
			"owner",
		)
		.unwrap();
}
fn key_headers() -> HeaderMap {
	let mut h = HeaderMap::new();
	h.insert(
		header::AUTHORIZATION,
		"Bearer mp_pool_test".parse().unwrap(),
	);
	h
}
async fn body(response: Response) -> Value {
	serde_json::from_slice(
		&axum::body::to_bytes(response.into_body(), 1024 * 1024)
			.await
			.unwrap(),
	)
	.unwrap()
}
async fn mock(router: Router) -> String {
	let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
	let url = format!("http://{}", listener.local_addr().unwrap());
	tokio::spawn(async move {
		axum::serve(listener, router).await.unwrap();
	});
	url
}

#[test]
fn encrypted_credentials_are_bound_to_the_subscription() {
	let state = test_state();
	let credentials = Credentials {
		access: "fixture-access".into(),
		refresh: "fixture-refresh".into(),
		account: String::new(),
		expires: 0,
	};
	let sealed = encrypt(&state, "a", &credentials).unwrap();
	assert!(!sealed.contains("fixture"));
	assert_eq!(
		decrypt(&state, "a", &sealed).unwrap().access,
		"fixture-access"
	);
	assert!(decrypt(&state, "b", &sealed).is_err());
	let mut bytes = URL_SAFE_NO_PAD.decode(&sealed).unwrap();
	bytes[15] ^= 1;
	assert!(decrypt(&state, "a", &URL_SAFE_NO_PAD.encode(bytes)).is_err());
}

#[test]
fn imports_require_subscription_logins() {
	assert!(parse_credential("claude", "sk-ant-api03-not-a-subscription").is_err());
	assert!(
		parse_credential(
			"codex",
			r#"{"auth_mode":"apikey","OPENAI_API_KEY":"fixture"}"#
		)
		.is_err()
	);
	let token = format!(
		"header.{}.signature",
		URL_SAFE_NO_PAD.encode(
			br#"{"exp":9999999999,"https://api.openai.com/auth":{"chatgpt_account_id":"account-1"}}"#
		)
	);
	let raw = json!({"auth_mode":"chatgpt","tokens":{"access_token":token,"refresh_token":"fixture-refresh"}}).to_string();
	assert_eq!(
		parse_credential("codex", &raw).unwrap().account,
		"account-1"
	);
	assert!(parse_credential("claude", &format!("sk-ant-oat01-{}", "x".repeat(30))).is_ok());
}

#[tokio::test]
async fn management_respects_ownership_and_keys_are_returned_once() {
	let state = test_state();
	add_member(&state, "a", "claude");
	let (viewer, _) = session(&state, "colleague", vec![Role::Viewer]);
	let change = Change::IssueKey {
		pool_id: "pool".into(),
		name: "Self-service".into(),
		models: vec!["model".into()],
		expires_in_days: 30,
	};
	let issued = mutate(
		&state,
		&state.inner.store.lock().unwrap(),
		&viewer,
		false,
		false,
		change,
	)
	.unwrap();
	assert!(issued["token"].as_str().unwrap().starts_with("mp_pool_"));
	let (_, headers) = session(&state, "colleague", vec![Role::Viewer]);
	let listed = body(list(State(state.clone()), headers).await).await;
	assert!(
		!listed
			.to_string()
			.contains(issued["token"].as_str().unwrap())
	);
	assert!(!listed.to_string().contains("fixture-a"));
	assert!(!listed.to_string().contains("credential"));
	assert!(listed["catalog"].is_null());
	let forbidden = mutate(
		&state,
		&state.inner.store.lock().unwrap(),
		&viewer,
		false,
		false,
		Change::SavePool {
			id: Some("pool".into()),
			name: "Hijack".into(),
			enabled: true,
			shared: true,
		},
	)
	.unwrap_err();
	assert_eq!(forbidden.status(), StatusCode::FORBIDDEN);
	let forbidden = mutate(
		&state,
		&state.inner.store.lock().unwrap(),
		&viewer,
		false,
		false,
		Change::SaveSubscription {
			pool_id: "pool".into(),
			id: None,
			name: "Hijack".into(),
			provider: "claude".into(),
			models: vec!["model".into()],
			credential: None,
			enabled: true,
			max_parallel: 1,
		},
	)
	.unwrap_err();
	assert_eq!(forbidden.status(), StatusCode::FORBIDDEN);
	let (outsider, _) = session(&state, "outsider", vec![Role::Admin]);
	assert_eq!(
		mutate(
			&state,
			&state.inner.store.lock().unwrap(),
			&outsider,
			true,
			true,
			Change::SavePool {
				id: Some("pool".into()),
				name: "Hijack".into(),
				shared: true,
				enabled: true
			}
		)
		.unwrap_err()
		.status(),
		StatusCode::FORBIDDEN
	);
	assert_eq!(
		mutate(
			&state,
			&state.inner.store.lock().unwrap(),
			&outsider,
			false,
			false,
			Change::RevokeKey {
				pool_id: "pool".into(),
				id: "key".into()
			}
		)
		.unwrap_err()
		.status(),
		StatusCode::FORBIDDEN
	);
}

#[tokio::test]
async fn messages_forward_provider_auth_and_preserve_the_response() {
	let state = test_state();
	add_member(&state, "a", "claude");
	let url = mock(Router::new().route("/",post(|headers: HeaderMap,Json(input): Json<Value>| async move {
		assert_eq!(headers[header::AUTHORIZATION],"Bearer fixture-a");
		assert!(!headers.contains_key("x-api-key"));
		assert_eq!(headers["anthropic-version"],"2023-06-01");
		assert!(headers["anthropic-beta"].to_str().unwrap().contains("oauth-2025-04-20"));
		assert_eq!(input["messages"][0]["content"],"Hello");
		Json(json!({"id":"msg_test","type":"message","role":"assistant","content":[{"type":"text","text":"Hello there"}],"usage":{"input_tokens":1,"output_tokens":2}}))
	}))).await;
	let mut headers = key_headers();
	headers.insert(
		"traceparent",
		"00-4bf92f3577b34da6a3ce929d0e0e4736-00f067aa0ba902b7-01"
			.parse()
			.unwrap(),
	);
	headers.insert("x-claude-code-session-id", "session-1".parse().unwrap());
	let response = forward(
		state.clone(),
		headers,
		json!({"model":"model","max_tokens":32,"messages":[{"role":"user","content":"Hello"}]}),
		"claude",
		false,
		&url,
	)
	.await;
	assert_eq!(response.status(), StatusCode::OK);
	assert_eq!(body(response).await["usage"]["output_tokens"], 2);
	let events = state
		.inner
		.store
		.lock()
		.unwrap()
		.search_audit(&Default::default())
		.unwrap();
	let completed = events.iter().find(|e| e.decision == "complete").unwrap();
	assert_eq!(completed.telemetry.provider, "anthropic");
	assert_eq!(completed.telemetry.conversation_id, "session-1");
	assert_eq!(
		completed.telemetry.trace_id,
		"4bf92f3577b34da6a3ce929d0e0e4736"
	);
	assert_eq!(completed.telemetry.input_tokens, Some(1));
	assert_eq!(completed.telemetry.output_tokens, Some(2));
	// Governance decisions for the same request join the same conversation and trace.
	let decisions: Vec<_> = events.iter().filter(|e| e.kind == "decision").collect();
	assert!(!decisions.is_empty());
	assert!(
		decisions
			.iter()
			.all(|e| e.telemetry.conversation_id == "session-1"
				&& e.telemetry.trace_id == "4bf92f3577b34da6a3ce929d0e0e4736")
	);
	assert_eq!(
		state
			.inner
			.store
			.lock()
			.unwrap()
			.pool_members("pool", Utc::now().timestamp())
			.unwrap()[0]
			.active,
		0
	);
}

#[tokio::test]
async fn rate_limits_fail_over_without_forwarding_client_credentials() {
	let state = test_state();
	add_member(&state, "a", "claude");
	add_member(&state, "b", "claude");
	let calls = Arc::new(AtomicUsize::new(0));
	let received = calls.clone();
	let url = mock(Router::new().route(
		"/",
		post(move |headers: HeaderMap| {
			let calls = received.clone();
			async move {
				calls.fetch_add(1, Ordering::SeqCst);
				if headers[header::AUTHORIZATION] == "Bearer fixture-a" {
					(
						StatusCode::TOO_MANY_REQUESTS,
						[(header::RETRY_AFTER, "120")],
						Json(json!({"error":"fixture"})),
					)
						.into_response()
				} else {
					Json(json!({"id":"msg_b","type":"message","content":[]})).into_response()
				}
			}
		}),
	))
	.await;
	let result = forward(
		state.clone(),
		key_headers(),
		json!({"model":"model","messages":[]}),
		"claude",
		false,
		&url,
	)
	.await;
	assert_eq!(result.status(), StatusCode::OK);
	assert_eq!(calls.load(Ordering::SeqCst), 2);
	let members = state
		.inner
		.store
		.lock()
		.unwrap()
		.pool_members("pool", Utc::now().timestamp())
		.unwrap();
	assert!(
		members
			.iter()
			.find(|m| m.subscription.id == "a")
			.unwrap()
			.cooldown_until
			>= Utc::now().timestamp() + 119
	);
	assert!(members.iter().all(|m| m.active == 0));
}

#[tokio::test]
async fn responses_support_json_clients_and_fragmented_sse() {
	let state = test_state();
	add_member(&state, "a", "codex");
	let url = mock(Router::new().route("/",post(|headers: HeaderMap,Json(input): Json<Value>| async move {
		assert_eq!(headers["chatgpt-account-id"],"test-account");
		assert_eq!(headers[header::AUTHORIZATION],"Bearer fixture-a");
		assert_eq!(input["stream"],true); assert_eq!(input["store"],false);
		assert_eq!(input["input"][0]["content"][0]["text"],"Hello");
		let content = "event: response.completed\r\ndata: {\"type\":\"response.completed\",\"response\":{\"id\":\"resp_test\",\"object\":\"response\",\"status\":\"completed\",\"output\":[],\"usage\":{\"input_tokens\":3,\"output_tokens\":4}}}\r\n\r\n";
		let chunks = content.as_bytes().chunks(3).map(|c| Ok::<_,std::io::Error>(axum::body::Bytes::copy_from_slice(c))).collect::<Vec<_>>();
		([(header::CONTENT_TYPE,"text/event-stream")],Body::from_stream(futures_util::stream::iter(chunks)))
	}))).await;
	let response = forward(
		state.clone(),
		key_headers(),
		json!({"model":"model","input":"Hello"}),
		"codex",
		false,
		&url,
	)
	.await;
	assert_eq!(response.status(), StatusCode::OK);
	let output = body(response).await;
	assert_eq!(output["id"], "resp_test");
	assert_eq!(output["usage"]["output_tokens"], 4);
	let response = forward(
		state.clone(),
		key_headers(),
		json!({"model":"model","input":"Hello","stream":true}),
		"codex",
		false,
		&url,
	)
	.await;
	assert_eq!(
		response.headers()[header::CONTENT_TYPE],
		"text/event-stream"
	);
	assert_eq!(
		state
			.inner
			.store
			.lock()
			.unwrap()
			.pool_members("pool", Utc::now().timestamp())
			.unwrap()[0]
			.active,
		1
	);
	axum::body::to_bytes(response.into_body(), 1024 * 1024)
		.await
		.unwrap();
	// JSON and streamed requests each record the provider's usage once the response completes.
	let completed: Vec<_> = state
		.inner
		.store
		.lock()
		.unwrap()
		.search_audit(&Default::default())
		.unwrap()
		.into_iter()
		.filter(|e| e.decision == "complete")
		.collect();
	assert_eq!(completed.len(), 2);
	assert!(completed.iter().all(|e| e.telemetry.provider == "openai"
		&& e.telemetry.input_tokens == Some(3)
		&& e.telemetry.output_tokens == Some(4)));
	assert_eq!(
		state
			.inner
			.store
			.lock()
			.unwrap()
			.pool_members("pool", Utc::now().timestamp())
			.unwrap()[0]
			.active,
		0
	);
}

#[tokio::test]
async fn request_and_streaming_governance_cannot_be_spoofed() {
	let state = test_state();
	add_member(&state, "a", "claude");
	{
		let store = state.inner.store.lock().unwrap();
		store
			.set_guardrail_mode(guardrail::Mode::Enforce, "owner")
			.unwrap();
		store
			.put_document(&Document {
				kind: "settings".into(),
				id: "global".into(),
				body: json!({"sovereignMode":false}),
				revision: 1,
				updated_at: String::new(),
				updated_by: "owner".into(),
				active: true,
			})
			.unwrap();
	}
	let url = mock(Router::new().route("/",post(|| async {
		([(header::CONTENT_TYPE,"text/event-stream")],"event: content_block_delta\ndata: {\"type\":\"content_block_delta\",\"delta\":{\"type\":\"text_delta\",\"text\":\"fixture@\"}}\n\nevent: content_block_delta\ndata: {\"type\":\"content_block_delta\",\"delta\":{\"type\":\"text_delta\",\"text\":\"example.com\"}}\n\n")
	}))).await;
	let mut headers = key_headers();
	headers.insert("x-magpie-actor", "outsider".parse().unwrap());
	let response = forward(
		state.clone(),
		headers.clone(),
		json!({"model":"model","messages":[{"role":"user","content":"fixture@example.com"}]}),
		"claude",
		false,
		&url,
	)
	.await;
	assert_eq!(response.status(), StatusCode::FORBIDDEN);
	let mut marked = headers.clone();
	marked.insert("x-magpie-marking", "PROTECTED".parse().unwrap());
	marked.insert("x-magpie-region", "au".parse().unwrap());
	let protected = forward(
		state.clone(),
		marked,
		json!({"model":"model","messages":[{"role":"user","content":"Hello"}]}),
		"claude",
		false,
		&url,
	)
	.await;
	assert_eq!(
		protected.status(),
		StatusCode::FORBIDDEN,
		"a client cannot route a PROTECTED request to a global subscription by spoofing its region"
	);
	let response = forward(
		state.clone(),
		headers,
		json!({"model":"model","stream":true,"messages":[{"role":"user","content":"Hello"}]}),
		"claude",
		false,
		&url,
	)
	.await;
	assert_eq!(response.status(), StatusCode::OK);
	let bytes = axum::body::to_bytes(response.into_body(), 1024 * 1024)
		.await
		.unwrap();
	let sse = String::from_utf8(bytes.to_vec()).unwrap();
	assert!(sse.contains("fixture@"));
	assert!(!sse.contains("example.com"));
	assert!(sse.contains("event: error\n"));
	let events = state
		.inner
		.store
		.lock()
		.unwrap()
		.search_audit(&AuditQuery {
			limit: 100,
			..Default::default()
		})
		.unwrap();
	assert!(
		events
			.iter()
			.filter(|e| e.kind == "decision")
			.all(|e| e.actor == "colleague")
	);
}

#[test]
fn cooldowns_respect_both_windows_and_retry_after() {
	let mut h = HeaderMap::new();
	h.insert("x-codex-primary-used-percent", "100".parse().unwrap());
	h.insert("x-codex-primary-reset-after-seconds", "30".parse().unwrap());
	h.insert("x-codex-secondary-used-percent", "100".parse().unwrap());
	h.insert(
		"x-codex-secondary-reset-after-seconds",
		"120".parse().unwrap(),
	);
	assert_eq!(cooldown(&h, 100, false), 220);
	h.insert(header::RETRY_AFTER, "300".parse().unwrap());
	assert_eq!(cooldown(&h, 100, true), 400);
	assert!(cooldown(&HeaderMap::new(), 100, true) > 100);
}

#[tokio::test]
async fn key_scope_and_public_routes_require_pool_authentication() {
	let state = test_state();
	add_member(&state, "a", "claude");
	let base = mock(crate::router(state.clone())).await;
	let client = reqwest::Client::new();
	let (_, unassigned) = session(&state, "outsider", vec![]);
	assert_eq!(
		client
			.get(format!("{base}/api/magpie/pools"))
			.headers(unassigned)
			.send()
			.await
			.unwrap()
			.status(),
		StatusCode::FORBIDDEN
	);
	assert_eq!(
		client
			.get(format!("{base}/pool/v1/models"))
			.send()
			.await
			.unwrap()
			.status(),
		StatusCode::UNAUTHORIZED
	);
	let models = client
		.get(format!("{base}/pool/v1/models"))
		.bearer_auth("mp_pool_test")
		.send()
		.await
		.unwrap()
		.json::<Value>()
		.await
		.unwrap();
	assert_eq!(models["data"][0]["id"], "model");
	let denied = client
		.post(format!("{base}/pool/v1/messages"))
		.bearer_auth("mp_pool_test")
		.json(&json!({"model":"other","messages":[]}))
		.send()
		.await
		.unwrap();
	assert_eq!(denied.status(), StatusCode::FORBIDDEN);
	let (_, cookie) = session(&state, "colleague", vec![Role::Viewer]);
	let csrf = client.post(format!("{base}/api/magpie/pools")).headers(cookie).header(header::ORIGIN,"https://untrusted.example").json(&json!({"action":"issueKey","poolId":"pool","name":"forged","models":["model"],"expiresInDays":30})).send().await.unwrap();
	assert_eq!(csrf.status(), StatusCode::FORBIDDEN);
	let (_, viewer) = session(&state, "colleague", vec![Role::Viewer]);
	let issued = client.post(format!("{base}/api/magpie/pools")).headers(viewer.clone()).header(header::ORIGIN,&base)
		.json(&json!({"action":"issueKey","poolId":"pool","name":"viewer-self-service","models":["model"],"expiresInDays":30}))
		.send().await.unwrap();
	assert_eq!(issued.status(), StatusCode::OK);
	assert!(
		issued.json::<Value>().await.unwrap()["token"]
			.as_str()
			.unwrap()
			.starts_with("mp_pool_")
	);
	let forbidden = client
		.post(format!("{base}/api/magpie/pools"))
		.headers(viewer)
		.header(header::ORIGIN, &base)
		.json(&json!({"action":"savePool","name":"forbidden","shared":true,"enabled":true}))
		.send()
		.await
		.unwrap();
	assert_eq!(forbidden.status(), StatusCode::FORBIDDEN);
	let malformed = client
		.post(format!("{base}/pool/v1/messages"))
		.bearer_auth("mp_pool_test")
		.header(header::CONTENT_TYPE, "application/json")
		.body("{")
		.send()
		.await
		.unwrap();
	assert_eq!(malformed.status(), StatusCode::BAD_REQUEST);
	assert_eq!(
		malformed.json::<Value>().await.unwrap()["error"]["type"],
		"invalid_request_error"
	);
}

#[tokio::test]
#[allow(clippy::result_large_err)]
async fn pool_names_are_unique_and_only_owners_delete_pools() {
	let state = test_state();
	let (owner, _) = session(&state, "owner", vec![Role::Admin]);
	let (outsider, _) = session(&state, "outsider", vec![Role::Admin]);
	let save = |session: &SessionRecord, id: Option<&str>, name: &str| {
		mutate(
			&state,
			&state.inner.store.lock().unwrap(),
			session,
			true,
			true,
			Change::SavePool {
				id: id.map(Into::into),
				name: name.into(),
				shared: true,
				enabled: true,
			},
		)
	};
	assert_eq!(
		save(&outsider, None, " team ").unwrap_err().status(),
		StatusCode::CONFLICT
	);
	save(&owner, Some("pool"), "Team").unwrap();
	let other = save(&owner, None, "Other").unwrap();
	assert_eq!(
		save(&owner, other["id"].as_str(), "TEAM")
			.unwrap_err()
			.status(),
		StatusCode::CONFLICT
	);
	let delete = |session: &SessionRecord| {
		mutate(
			&state,
			&state.inner.store.lock().unwrap(),
			session,
			true,
			true,
			Change::DeletePool { id: "pool".into() },
		)
	};
	assert_eq!(
		delete(&outsider).unwrap_err().status(),
		StatusCode::FORBIDDEN
	);
	delete(&owner).unwrap();
	assert!(
		state
			.inner
			.store
			.lock()
			.unwrap()
			.subscription_pool("pool")
			.unwrap()
			.is_none()
	);
	save(&outsider, None, "Team").unwrap();
}
