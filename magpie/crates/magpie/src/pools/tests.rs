use super::*;

#[test]
fn late_success_cannot_clear_another_requests_cooldown() {
	let store = setup();
	let mut subscription = member("a", "claude", 2);
	store
		.save_pool_subscription(&subscription, "a", "owner")
		.unwrap();
	let now = chrono::Utc::now().timestamp();
	store
		.pool_subscription_status("a", "limited", now + 120)
		.unwrap();
	store.pool_subscription_status("a", "ready", 0).unwrap();
	let account = store.pool_members("pool", now).unwrap().remove(0);
	assert_eq!(account.status, "limited");
	assert_eq!(account.cooldown_until, now + 120);
	store
		.pool_subscription_status("a", "reauthenticate", 0)
		.unwrap();
	store.pool_subscription_status("a", "ready", 0).unwrap();
	assert_eq!(
		store.pool_members("pool", now).unwrap()[0].status,
		"reauthenticate"
	);
	subscription.credential = "reconnected".into();
	store
		.save_pool_subscription(&subscription, "a", "owner")
		.unwrap();
	assert_eq!(store.pool_members("pool", now).unwrap()[0].status, "ready");
}
#[test]
fn independent_connections_cannot_overfill_an_account() {
	let path = std::env::temp_dir().join(format!("magpie-pool-{}.sqlite", crate::new_id()));
	let store = Store::open_path(path.to_str().unwrap()).unwrap();
	store
		.save_pool_subscription(&member("a", "claude", 1), "a", "owner")
		.unwrap();
	drop(store);
	let barrier = std::sync::Arc::new(std::sync::Barrier::new(2));
	let tasks = (0..2)
		.map(|_| {
			let path = path.clone();
			let barrier = barrier.clone();
			std::thread::spawn(move || {
				let store = Store::open_path(path.to_str().unwrap()).unwrap();
				barrier.wait();
				store
					.claim_pool_subscription("pool", "claude", "model", &[], 100)
					.unwrap()
					.is_some()
			})
		})
		.collect::<Vec<_>>();
	let claimed = tasks
		.into_iter()
		.map(|t| t.join().unwrap())
		.filter(|claimed| *claimed)
		.count();
	assert_eq!(claimed, 1);
	let reopened = Store::open_path(path.to_str().unwrap()).unwrap();
	assert_eq!(reopened.pool_members("pool", 100).unwrap()[0].active, 1);
	drop(reopened);
	std::fs::remove_file(path).unwrap();
}
fn setup() -> Store {
	let store = Store::open_memory().unwrap();
	for subject in ["owner", "colleague", "outsider"] {
		store
			.remember_portal_user(&crate::rbac::PortalUser {
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
}
fn member(id: &str, provider: &str, max_parallel: u32) -> Subscription {
	Subscription {
		id: id.into(),
		pool_id: "pool".into(),
		name: id.into(),
		provider: provider.into(),
		models: vec!["model".into()],
		enabled: true,
		max_parallel,
		credential: "encrypted".into(),
	}
}
fn key() -> PoolKey {
	PoolKey {
		id: "key".into(),
		pool_id: "pool".into(),
		owner: "colleague".into(),
		name: "Coding".into(),
		models: vec!["model".into()],
		expires_at: 1000,
		revoked: false,
	}
}
#[test]
fn keys_respect_sharing_revocation_expiry_and_account_disabling() {
	let store = setup();
	store
		.issue_pool_key(&key(), "test-pool-token", "colleague")
		.unwrap();
	assert!(store.authenticate_pool_key("wrong", 100).unwrap().is_none());
	assert!(
		store
			.authenticate_pool_key("test-pool-token", 100)
			.unwrap()
			.is_some()
	);
	assert!(
		store
			.authenticate_pool_key("test-pool-token", 1000)
			.unwrap()
			.is_none()
	);
	let mut pool = store.subscription_pool("pool").unwrap().unwrap();
	pool.shared = false;
	store.save_subscription_pool(&pool, "owner").unwrap();
	assert!(
		store
			.authenticate_pool_key("test-pool-token", 100)
			.unwrap()
			.is_none()
	);
	pool.shared = true;
	pool.enabled = false;
	store.save_subscription_pool(&pool, "owner").unwrap();
	assert!(
		store
			.authenticate_pool_key("test-pool-token", 100)
			.unwrap()
			.is_none()
	);
	pool.enabled = true;
	store.save_subscription_pool(&pool, "owner").unwrap();
	store.conn.execute("UPDATE portal_users SET body=json_set(body,'$.disabled',json('true')) WHERE subject='colleague'", []).unwrap();
	assert!(
		store
			.authenticate_pool_key("test-pool-token", 100)
			.unwrap()
			.is_none()
	);
	store.conn.execute("UPDATE portal_users SET body=json_set(body,'$.disabled',json('false')) WHERE subject='colleague'", []).unwrap();
	store.revoke_pool_key(key(), "colleague").unwrap();
	assert!(
		store
			.authenticate_pool_key("test-pool-token", 100)
			.unwrap()
			.is_none()
	);
}
#[test]
fn capacity_is_reserved_until_release_or_expiry() {
	let store = setup();
	store
		.save_pool_subscription(&member("a", "claude", 1), "a", "owner")
		.unwrap();
	let (_, lease) = store
		.claim_pool_subscription("pool", "claude", "model", &[], 100)
		.unwrap()
		.unwrap();
	assert!(
		store
			.claim_pool_subscription("pool", "claude", "model", &[], 100)
			.unwrap()
			.is_none()
	);
	store.release_pool_lease(&lease).unwrap();
	assert!(
		store
			.claim_pool_subscription("pool", "claude", "model", &[], 100)
			.unwrap()
			.is_some()
	);
	assert!(
		store
			.claim_pool_subscription("pool", "claude", "model", &[], 761)
			.unwrap()
			.is_some()
	);
}
#[test]
fn scheduling_spreads_load_and_avoids_exhausted_or_mismatched_accounts() {
	let store = setup();
	for id in ["a", "b"] {
		store
			.save_pool_subscription(&member(id, "claude", 2), id, "owner")
			.unwrap();
	}
	let (first, lease) = store
		.claim_pool_subscription("pool", "claude", "model", &[], 100)
		.unwrap()
		.unwrap();
	store.release_pool_lease(&lease).unwrap();
	let (second, _) = store
		.claim_pool_subscription("pool", "claude", "model", &[], 100)
		.unwrap()
		.unwrap();
	assert_ne!(
		first.id, second.id,
		"rotation must work within the same second"
	);
	assert!(
		store
			.claim_pool_subscription("pool", "codex", "model", &[], 100)
			.unwrap()
			.is_none()
	);
	assert!(
		store
			.claim_pool_subscription("pool", "claude", "wrong-model", &[], 100)
			.unwrap()
			.is_none()
	);
	store.pool_subscription_status("a", "limited", 200).unwrap();
	store
		.pool_subscription_status("b", "reauthenticate", 0)
		.unwrap();
	assert!(
		store
			.claim_pool_subscription("pool", "claude", "model", &[], 100)
			.unwrap()
			.is_none()
	);
	assert_eq!(
		store
			.claim_pool_subscription("pool", "claude", "model", &[], 200)
			.unwrap()
			.unwrap()
			.0
			.id,
		"a"
	);
}
#[test]
fn edits_preserve_quota_state_and_reconnect_resets_it() {
	let store = setup();
	let mut s = member("a", "claude", 1);
	store
		.save_pool_subscription(&s, "fingerprint", "owner")
		.unwrap();
	store.pool_subscription_status("a", "limited", 500).unwrap();
	s.enabled = false;
	store
		.save_pool_subscription(&s, "fingerprint", "owner")
		.unwrap();
	s.enabled = true;
	store
		.save_pool_subscription(&s, "fingerprint", "owner")
		.unwrap();
	assert!(
		store
			.claim_pool_subscription("pool", "claude", "model", &[], 100)
			.unwrap()
			.is_none()
	);
	s.credential = "new encrypted credentials".into();
	store
		.save_pool_subscription(&s, "fingerprint", "owner")
		.unwrap();
	assert!(
		store
			.claim_pool_subscription("pool", "claude", "model", &[], 100)
			.unwrap()
			.is_some()
	);
}
#[test]
fn secrets_are_separate_from_policy_exports_and_audit() {
	let store = setup();
	let mut s = member("a", "claude", 1);
	s.credential = "ciphertext-only".into();
	store
		.save_pool_subscription(&s, "fingerprint", "owner")
		.unwrap();
	store
		.issue_pool_key(&key(), "raw-issued-key", "colleague")
		.unwrap();
	let body: String = store
		.conn
		.query_row("SELECT body FROM pool_keys", [], |r| r.get(0))
		.unwrap();
	assert!(!body.contains("raw-issued-key"));
	let audit = serde_json::to_string(
		&store
			.search_audit(&crate::audit::AuditQuery {
				limit: 100,
				..Default::default()
			})
			.unwrap(),
	)
	.unwrap();
	assert!(!audit.contains("ciphertext-only") && !audit.contains("raw-issued-key"));
	assert!(
		store
			.list_documents("subscription-pool")
			.unwrap()
			.is_empty()
	);
	assert!(
		store
			.save_pool_subscription(&member("b", "claude", 1), "fingerprint", "owner")
			.is_err(),
		"one account cannot enter the pool twice"
	);
}
#[test]
fn deleting_a_pool_waits_for_leases_and_removes_members_and_keys() {
	let store = setup();
	store
		.save_pool_subscription(&member("a", "claude", 1), "a", "owner")
		.unwrap();
	store
		.issue_pool_key(&key(), "test-pool-token", "colleague")
		.unwrap();
	let now = chrono::Utc::now().timestamp();
	let (_, lease) = store
		.claim_pool_subscription("pool", "claude", "model", &[], now)
		.unwrap()
		.unwrap();
	assert!(store.delete_subscription_pool("pool", "owner").is_err());
	assert!(store.subscription_pool("pool").unwrap().is_some());
	store.release_pool_lease(&lease).unwrap();
	store.delete_subscription_pool("pool", "owner").unwrap();
	assert!(store.subscription_pool("pool").unwrap().is_none());
	assert!(store.pool_members("pool", now).unwrap().is_empty());
	assert!(store.pool_keys("pool").unwrap().is_empty());
	assert!(
		store
			.authenticate_pool_key("test-pool-token", 0)
			.unwrap()
			.is_none()
	);
}
