use std::time::Duration;

use magpie::identity::{BreakGlassConfig, OidcProviderKind, hash_password};

#[tokio::main]
async fn main() {
	let database = std::env::var("MAGPIE_DATABASE_URL").unwrap_or_else(|_| "magpie.sqlite".into());
	let demo = std::env::var("MAGPIE_DEMO").ok().as_deref() == Some("1");
	let supplied = std::env::var("MAGPIE_SESSION_SECRET").ok();
	let secret = magpie::resolve_session_secret(supplied.as_deref(), demo).unwrap_or_else(|error| {
		eprintln!("{error}");
		std::process::exit(1);
	});
	let mut break_glass = BreakGlassConfig::default();
	if std::env::var("MAGPIE_BREAK_GLASS").ok().as_deref() == Some("1") {
		let password =
			std::env::var("MAGPIE_BREAK_GLASS_PASSWORD").unwrap_or_else(|_| "change-me".into());
		break_glass.enabled = true;
		break_glass.password_hash = hash_password(&password).expect("hash");
	}
	let config = magpie_portal::Config {
		database_path: database,
		cookie_secure: std::env::var("MAGPIE_COOKIE_SECURE").ok().as_deref() != Some("0"),
		demo,
		seed_examples: std::env::var("MAGPIE_SEED_EXAMPLES").ok().as_deref() == Some("1"),
		mock_idp: std::env::var("MAGPIE_MOCK_IDP").ok().as_deref() == Some("1"),
		oidc_kind: OidcProviderKind::Generic,
		issuer: std::env::var("MAGPIE_OIDC_ISSUER")
			.unwrap_or_else(|_| "https://login.example/mock".into()),
		client_id: std::env::var("MAGPIE_OIDC_CLIENT_ID").unwrap_or_else(|_| "magpie".into()),
		audience: std::env::var("MAGPIE_OIDC_AUDIENCE").unwrap_or_else(|_| "magpie".into()),
		hmac_secret: secret.clone(),
		session_secret: secret,
		scim_token: std::env::var("MAGPIE_SCIM_TOKEN").unwrap_or_default(),
		break_glass,
		idle: Duration::from_secs(30 * 60),
		absolute: Duration::from_secs(12 * 60 * 60),
		listen: std::env::var("MAGPIE_LISTEN").unwrap_or_else(|_| "127.0.0.1:8088".into()),
	};
	let listen = config.listen.clone();
	let state = magpie_portal::state(config).expect("database");
	let app = magpie_portal::router(state);
	let listener = tokio::net::TcpListener::bind(&listen).await.expect("bind");
	println!(
		"Magpie Gateway portal listening on {}",
		listener.local_addr().expect("addr")
	);
	axum::serve(listener, app).await.expect("serve");
}
