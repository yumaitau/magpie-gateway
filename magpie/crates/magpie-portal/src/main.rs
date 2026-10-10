use std::time::Duration;

use magpie::identity::{
	BreakGlassConfig, OidcProviderKind, RoleMap, admin_password_problem, hash_password,
};

fn env(name: &str) -> Option<String> {
	std::env::var(name).ok().filter(|value| !value.is_empty())
}

fn flag(name: &str) -> bool {
	env(name).as_deref() == Some("1")
}

fn fail(message: &str) -> ! {
	eprintln!("{message}");
	std::process::exit(1);
}

#[tokio::main]
async fn main() {
	let database = env("MAGPIE_DATABASE_URL").unwrap_or_else(|| "magpie.sqlite".into());
	let demo = flag("MAGPIE_DEMO");
	let mock_idp = flag("MAGPIE_MOCK_IDP");
	let secret = magpie::resolve_session_secret(env("MAGPIE_SESSION_SECRET").as_deref(), demo)
		.unwrap_or_else(|error| fail(&error.to_string()));

	// MAGPIE_ADMIN_PASSWORD turns on the local administrator. MAGPIE_BREAK_GLASS=1
	// is the older switch and now needs MAGPIE_BREAK_GLASS_PASSWORD as well.
	let mut break_glass = BreakGlassConfig::default();
	let password = match (env("MAGPIE_ADMIN_PASSWORD"), flag("MAGPIE_BREAK_GLASS")) {
		(Some(password), _) => {
			break_glass.username = env("MAGPIE_ADMIN_USER").unwrap_or_else(|| "admin".into());
			Some(password)
		},
		(None, true) => Some(env("MAGPIE_BREAK_GLASS_PASSWORD").unwrap_or_else(|| {
			fail("MAGPIE_BREAK_GLASS=1 needs MAGPIE_BREAK_GLASS_PASSWORD. There is no default password.")
		})),
		(None, false) => None,
	};
	if let Some(password) = password {
		if let Some(problem) = admin_password_problem(&password) {
			fail(problem);
		}
		break_glass.enabled = true;
		break_glass.password_hash =
			hash_password(&password).unwrap_or_else(|error| fail(&error.to_string()));
	}

	// MAGPIE_ENTRA_TENANT_ID is a shortcut for the Entra v2.0 issuer.
	let (oidc_kind, issuer) = match (env("MAGPIE_ENTRA_TENANT_ID"), env("MAGPIE_OIDC_ISSUER")) {
		(Some(tenant), _) => (OidcProviderKind::Entra, tenant),
		(None, Some(issuer)) => (OidcProviderKind::Generic, issuer),
		(None, None) if mock_idp => (
			OidcProviderKind::Generic,
			"https://login.example/mock".into(),
		),
		(None, None) => (OidcProviderKind::Generic, String::new()),
	};
	let client_id = env("MAGPIE_OIDC_CLIENT_ID").unwrap_or_else(|| {
		if mock_idp {
			"magpie".into()
		} else {
			String::new()
		}
	});
	let mut role_map = RoleMap::default();
	role_map.extend_from(env);

	let guardrail_mode = env("MAGPIE_GUARDRAIL_MODE")
		.unwrap_or_else(|| "enforce".into())
		.parse::<magpie::guardrail::Mode>()
		.unwrap_or_else(|error| fail(&error.to_string()));
	println!("Magpie guardrail mode: {}", guardrail_mode.as_str());
	let config = magpie_portal::Config {
		guardrail_mode,
		database_path: database,
		cookie_secure: env("MAGPIE_COOKIE_SECURE").as_deref() != Some("0"),
		demo,
		seed_examples: flag("MAGPIE_SEED_EXAMPLES"),
		mock_idp,
		oidc_kind,
		issuer,
		audience: env("MAGPIE_OIDC_AUDIENCE").unwrap_or_else(|| client_id.clone()),
		client_id,
		client_secret: env("MAGPIE_OIDC_CLIENT_SECRET").unwrap_or_default(),
		public_url: env("MAGPIE_PUBLIC_URL").unwrap_or_default(),
		role_map,
		webhook_token: env("MAGPIE_WEBHOOK_TOKEN").unwrap_or_default(),
		bootstrap_models: env("MAGPIE_BOOTSTRAP_MODELS")
			.unwrap_or_default()
			.split(',')
			.filter_map(|item| {
				let (name, region) = item.trim().split_once('=')?;
				Some((name.trim().to_string(), region.trim().to_string()))
			})
			.filter(|(name, _)| !name.is_empty())
			.collect(),
		bootstrap_protected: env("MAGPIE_APPROVED_PROTECTED")
			.unwrap_or_default()
			.split(',')
			.map(|item| item.trim().to_string())
			.filter(|item| !item.is_empty())
			.collect(),
		bootstrap_owner: env("MAGPIE_BOOTSTRAP_OWNER").unwrap_or_default(),
		hmac_secret: secret.clone(),
		session_secret: secret,
		scim_token: env("MAGPIE_SCIM_TOKEN").unwrap_or_default(),
		break_glass,
		idle: Duration::from_secs(30 * 60),
		absolute: Duration::from_secs(12 * 60 * 60),
		listen: env("MAGPIE_LISTEN").unwrap_or_else(|| "127.0.0.1:8088".into()),
	};
	if !config.oidc_enabled() && !config.break_glass.enabled && !demo {
		fail(
			"No way to sign in. Set MAGPIE_ADMIN_PASSWORD, or MAGPIE_OIDC_ISSUER (or MAGPIE_ENTRA_TENANT_ID) with MAGPIE_OIDC_CLIENT_ID.",
		);
	}
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
