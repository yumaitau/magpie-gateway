//! Portal identity: OIDC with PKCE, cookie sessions, role mapping, break-glass.
//!
//! SAML is intentionally not a login method. Signature verification is not
//! implemented, and accepting unsigned assertions would be worse than saying so.

use std::time::Duration;

use base64::Engine;
use base64::engine::general_purpose::URL_SAFE_NO_PAD;
use chrono::{DateTime, Utc};
use hmac::{Hmac, Mac};
use serde::{Deserialize, Serialize};
use sha2::Sha256;

use crate::{Error, Result};

pub use jsonwebtoken::jwk::JwkSet;

type HmacSha256 = Hmac<Sha256>;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum Role {
	Owner,
	Admin,
	PolicyEditor,
	Approver,
	Auditor,
	Viewer,
}

impl Role {
	pub fn as_str(self) -> &'static str {
		match self {
			Self::Owner => "Owner",
			Self::Admin => "Admin",
			Self::PolicyEditor => "Policy Editor",
			Self::Approver => "Approver",
			Self::Auditor => "Auditor",
			Self::Viewer => "Viewer",
		}
	}

	pub fn parse(value: &str) -> Option<Self> {
		match value {
			"Owner" | "owner" => Some(Self::Owner),
			"Admin" | "admin" => Some(Self::Admin),
			"Policy Editor" | "PolicyEditor" | "policy-editor" => Some(Self::PolicyEditor),
			"Approver" | "approver" => Some(Self::Approver),
			"Auditor" | "auditor" => Some(Self::Auditor),
			"Viewer" | "viewer" => Some(Self::Viewer),
			_ => None,
		}
	}
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum Permission {
	ReadGatewayConfig,
	EditGatewayModels,
	EditGatewayProviders,
	EditGatewayKeys,
	EditGatewayPolicies,
	EditGatewayMcp,
	EditGatewayTraffic,
	EditGatewaySettings,
	ReadGatewayLogs,
	RunGatewayCel,
	ReadDashboard,
	ReadCatalogue,
	ReadAudit,
	ExportAudit,
	EditPolicy,
	Approve,
	ManageSettings,
	ManageIdentity,
	UseBreakGlass,
	ChangeEnforcement,
	ManageAccess,
	ReadSettings,
	ReadPolicies,
	EditDlp,
	SubmitApproval,
	ImportPolicy,
	ExportPolicy,
}

impl Role {
	pub fn allows(self, permission: Permission) -> bool {
		use Permission::*;
		match self {
			Self::Owner => true,
			Self::Admin => !matches!(permission, UseBreakGlass),
			Self::PolicyEditor => matches!(
				permission,
				ReadDashboard
					| ReadCatalogue
					| ReadPolicies
					| EditPolicy
					| EditDlp
					| SubmitApproval
					| ExportPolicy
			),
			Self::Approver => matches!(
				permission,
				ReadDashboard | ReadCatalogue | ReadPolicies | Approve
			),
			Self::Auditor => matches!(
				permission,
				ReadDashboard | ReadCatalogue | ReadPolicies | ReadSettings | ReadAudit | ExportAudit
			),
			Self::Viewer => matches!(permission, ReadDashboard | ReadCatalogue),
		}
	}
}

pub fn authorise(roles: &[Role], permission: Permission) -> Result<()> {
	if roles.iter().any(|role| role.allows(permission)) {
		Ok(())
	} else {
		Err(Error::Forbidden)
	}
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RoleMap {
	pub owner_groups: Vec<String>,
	pub admin_groups: Vec<String>,
	pub policy_editor_groups: Vec<String>,
	pub approver_groups: Vec<String>,
	pub auditor_groups: Vec<String>,
	pub viewer_groups: Vec<String>,
}

impl Default for RoleMap {
	fn default() -> Self {
		Self {
			owner_groups: vec!["magpie-owners".into()],
			admin_groups: vec!["magpie-admins".into()],
			policy_editor_groups: vec!["magpie-policy-editors".into()],
			approver_groups: vec!["magpie-approvers".into()],
			auditor_groups: vec!["magpie-auditors".into()],
			viewer_groups: vec!["magpie-viewers".into()],
		}
	}
}

impl RoleMap {
	/// Add group IDs or app-role values from `MAGPIE_ROLE_*` settings.
	///
	/// Each value is a comma-separated list. The defaults stay, so an
	/// IdP group named `magpie-owners` still maps to Owner.
	pub fn extend_from(&mut self, lookup: impl Fn(&str) -> Option<String>) {
		let lists = [
			("MAGPIE_ROLE_OWNERS", &mut self.owner_groups),
			("MAGPIE_ROLE_ADMINS", &mut self.admin_groups),
			("MAGPIE_ROLE_POLICY_EDITORS", &mut self.policy_editor_groups),
			("MAGPIE_ROLE_APPROVERS", &mut self.approver_groups),
			("MAGPIE_ROLE_AUDITORS", &mut self.auditor_groups),
			("MAGPIE_ROLE_VIEWERS", &mut self.viewer_groups),
		];
		for (name, groups) in lists {
			if let Some(value) = lookup(name) {
				groups.extend(
					value
						.split(',')
						.map(str::trim)
						.filter(|item| !item.is_empty())
						.map(str::to_string),
				);
			}
		}
	}
}

pub fn roles_from_groups(groups: &[String], map: &RoleMap) -> Vec<Role> {
	let mut roles = Vec::new();
	let add = |roles: &mut Vec<Role>, role: Role, configured: &[String]| {
		if groups
			.iter()
			.any(|group| configured.iter().any(|item| item == group))
		{
			roles.push(role);
		}
	};
	add(&mut roles, Role::Owner, &map.owner_groups);
	add(&mut roles, Role::Admin, &map.admin_groups);
	add(&mut roles, Role::PolicyEditor, &map.policy_editor_groups);
	add(&mut roles, Role::Approver, &map.approver_groups);
	add(&mut roles, Role::Auditor, &map.auditor_groups);
	add(&mut roles, Role::Viewer, &map.viewer_groups);
	if roles.is_empty() {
		roles.push(Role::Viewer);
	}
	roles
}

#[derive(Debug, Clone)]
pub struct Pkce {
	pub verifier: String,
	pub challenge: String,
	pub state: String,
	pub nonce: String,
}

pub fn start_pkce() -> Pkce {
	let verifier = random_url_token(32);
	let digest = Sha256::digest(verifier.as_bytes());
	Pkce {
		challenge: URL_SAFE_NO_PAD.encode(digest),
		verifier,
		state: random_url_token(16),
		nonce: random_url_token(16),
	}
}

fn random_url_token(bytes: usize) -> String {
	let id = uuid::Uuid::new_v4();
	let mut raw = id.as_bytes().to_vec();
	while raw.len() < bytes {
		raw.extend_from_slice(uuid::Uuid::new_v4().as_bytes());
	}
	URL_SAFE_NO_PAD.encode(&raw[..bytes])
}

use sha2::Digest;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum OidcProviderKind {
	Entra,
	Okta,
	Google,
	Generic,
}

pub fn issuer_for(kind: OidcProviderKind, tenant_or_domain: &str) -> String {
	match kind {
		OidcProviderKind::Entra => format!("https://login.microsoftonline.com/{tenant_or_domain}/v2.0"),
		OidcProviderKind::Okta => format!("https://{tenant_or_domain}/oauth2/default"),
		OidcProviderKind::Google => "https://accounts.google.com".to_string(),
		OidcProviderKind::Generic => tenant_or_domain.trim_end_matches('/').to_string(),
	}
}

pub fn authorisation_url(
	endpoint: &str,
	client_id: &str,
	redirect_uri: &str,
	pkce: &Pkce,
	scopes: &str,
) -> String {
	let mut url = format!(
		"{endpoint}?response_type=code&client_id={}&redirect_uri={}&scope={}&state={}&nonce={}&code_challenge={}&code_challenge_method=S256",
		urlencoding_min(client_id),
		urlencoding_min(redirect_uri),
		urlencoding_min(scopes),
		urlencoding_min(&pkce.state),
		urlencoding_min(&pkce.nonce),
		urlencoding_min(&pkce.challenge),
	);
	if !endpoint.contains('?') && url.matches('?').count() > 1 {
		let _ = &mut url;
	}
	url
}

fn urlencoding_min(value: &str) -> String {
	let mut out = String::new();
	for byte in value.bytes() {
		match byte {
			b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => out.push(byte as char),
			_ => out.push_str(&format!("%{byte:02X}")),
		}
	}
	out
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct IdClaims {
	pub sub: String,
	pub email: Option<String>,
	pub name: Option<String>,
	/// Entra ID puts the sign-in name here and often omits `email`.
	#[serde(default, skip_serializing_if = "Option::is_none")]
	pub preferred_username: Option<String>,
	#[serde(default)]
	pub groups: Vec<String>,
	#[serde(default)]
	pub roles: Vec<String>,
	pub nonce: Option<String>,
	pub iss: String,
	pub aud: serde_json::Value,
	pub exp: usize,
	#[serde(default)]
	pub events: serde_json::Value,
}

pub fn issue_hs256(claims: &IdClaims, secret: &[u8]) -> Result<String> {
	let header = jsonwebtoken::Header::new(jsonwebtoken::Algorithm::HS256);
	jsonwebtoken::encode(
		&header,
		claims,
		&jsonwebtoken::EncodingKey::from_secret(secret),
	)
	.map_err(|error| Error::Msg(error.to_string()))
}

pub fn verify_hs256(
	token: &str,
	secret: &[u8],
	audience: &str,
	expected_nonce: Option<&str>,
) -> Result<IdClaims> {
	let mut validation = jsonwebtoken::Validation::new(jsonwebtoken::Algorithm::HS256);
	validation.set_audience(&[audience]);
	let data = jsonwebtoken::decode::<IdClaims>(
		token,
		&jsonwebtoken::DecodingKey::from_secret(secret),
		&validation,
	)
	.map_err(|error| Error::Msg(error.to_string()))?;
	if let Some(expected) = expected_nonce
		&& data.claims.nonce.as_deref() != Some(expected)
	{
		return Err(Error::Msg(
			"OIDC nonce did not match the login attempt.".into(),
		));
	}
	Ok(data.claims)
}

/// Verify an ID or logout token signed by an identity provider key.
///
/// Only asymmetric algorithms are accepted, so a token cannot be signed
/// with a published key used as an HMAC secret.
pub fn verify_with_jwks(
	token: &str,
	jwks: &JwkSet,
	issuer: &str,
	audience: &str,
	expected_nonce: Option<&str>,
) -> Result<IdClaims> {
	use jsonwebtoken::Algorithm::*;
	let header = jsonwebtoken::decode_header(token).map_err(|error| Error::Msg(error.to_string()))?;
	if !matches!(
		header.alg,
		RS256 | RS384 | RS512 | PS256 | PS384 | PS512 | ES256 | ES384 | EdDSA
	) {
		return Err(Error::Msg(format!(
			"ID token algorithm {:?} is not accepted.",
			header.alg
		)));
	}
	let jwk = match &header.kid {
		Some(kid) => jwks.find(kid),
		None if jwks.keys.len() == 1 => jwks.keys.first(),
		None => None,
	}
	.ok_or_else(|| Error::Msg("No identity provider key matches the token.".into()))?;
	let key =
		jsonwebtoken::DecodingKey::from_jwk(jwk).map_err(|error| Error::Msg(error.to_string()))?;
	let mut validation = jsonwebtoken::Validation::new(header.alg);
	validation.set_audience(&[audience]);
	validation.set_issuer(&[issuer]);
	let data = jsonwebtoken::decode::<IdClaims>(token, &key, &validation)
		.map_err(|error| Error::Msg(error.to_string()))?;
	if let Some(expected) = expected_nonce
		&& data.claims.nonce.as_deref() != Some(expected)
	{
		return Err(Error::Msg(
			"OIDC nonce did not match the login attempt.".into(),
		));
	}
	Ok(data.claims)
}

pub const BACK_CHANNEL_EVENT: &str = "http://schemas.openid.net/event/backchannel-logout";

pub fn logout_subject(claims: &IdClaims) -> Result<String> {
	let events = claims
		.events
		.as_object()
		.ok_or_else(|| Error::Msg("Logout token has no events claim.".into()))?;
	if !events.contains_key(BACK_CHANNEL_EVENT) {
		return Err(Error::Msg(
			"Logout token is not a back-channel logout.".into(),
		));
	}
	if claims.sub.is_empty() {
		return Err(Error::Msg("Logout token has no subject.".into()));
	}
	Ok(claims.sub.clone())
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct SessionRecord {
	pub session_id: String,
	pub subject: String,
	pub email: String,
	pub name: String,
	#[serde(default)]
	pub identity_groups: Vec<String>,
	pub roles: Vec<Role>,
	pub issued_at: DateTime<Utc>,
	pub last_seen: DateTime<Utc>,
	pub absolute_expiry: DateTime<Utc>,
	pub break_glass: bool,
}

#[derive(Clone)]
pub struct SessionCodec {
	secret: Vec<u8>,
	pub idle: Duration,
	pub absolute: Duration,
}

impl SessionCodec {
	pub fn new(secret: impl Into<Vec<u8>>, idle: Duration, absolute: Duration) -> Self {
		Self {
			secret: secret.into(),
			idle,
			absolute,
		}
	}

	pub fn issue(
		&self,
		subject: &str,
		email: &str,
		name: &str,
		roles: Vec<Role>,
		now: DateTime<Utc>,
		break_glass: bool,
	) -> Result<(SessionRecord, String)> {
		let record = SessionRecord {
			session_id: crate::new_id(),
			subject: subject.to_string(),
			email: email.to_string(),
			name: name.to_string(),
			roles,
			identity_groups: vec![],
			issued_at: now,
			last_seen: now,
			absolute_expiry: now
				+ chrono::Duration::from_std(self.absolute)
					.map_err(|error| Error::Msg(error.to_string()))?,
			break_glass,
		};
		let cookie = self.seal(&record)?;
		Ok((record, cookie))
	}

	pub fn open(&self, cookie: &str, now: DateTime<Utc>) -> Result<SessionRecord> {
		let record = self.unseal(cookie)?;
		if now > record.absolute_expiry {
			return Err(Error::Msg(
				"The session has passed its absolute timeout.".into(),
			));
		}
		let idle =
			chrono::Duration::from_std(self.idle).map_err(|error| Error::Msg(error.to_string()))?;
		if now - record.last_seen > idle {
			return Err(Error::Msg(
				"The session has passed its idle timeout.".into(),
			));
		}
		Ok(record)
	}

	pub fn touch(
		&self,
		mut record: SessionRecord,
		now: DateTime<Utc>,
	) -> Result<(SessionRecord, String)> {
		let _ = self.open(&self.seal(&record)?, now)?;
		record.last_seen = now;
		let cookie = self.seal(&record)?;
		Ok((record, cookie))
	}

	fn seal(&self, record: &SessionRecord) -> Result<String> {
		let body = serde_json::to_vec(record)?;
		let encoded = URL_SAFE_NO_PAD.encode(body);
		let mac = sign(&self.secret, encoded.as_bytes());
		Ok(format!("{encoded}.{mac}"))
	}

	fn unseal(&self, cookie: &str) -> Result<SessionRecord> {
		let (encoded, mac) = cookie.split_once('.').ok_or_else(|| Error::Unauthorised)?;
		let expected = sign(&self.secret, encoded.as_bytes());
		if !constant_time_eq(mac.as_bytes(), expected.as_bytes()) {
			return Err(Error::Unauthorised);
		}
		let body = URL_SAFE_NO_PAD
			.decode(encoded)
			.map_err(|_| Error::Unauthorised)?;
		Ok(serde_json::from_slice(&body)?)
	}
}

fn sign(secret: &[u8], body: &[u8]) -> String {
	let mut mac = HmacSha256::new_from_slice(secret).expect("hmac");
	mac.update(body);
	let bytes = mac.finalize().into_bytes();
	URL_SAFE_NO_PAD.encode(bytes)
}

pub fn constant_time_eq(left: &[u8], right: &[u8]) -> bool {
	if left.len() != right.len() {
		return false;
	}
	let mut diff = 0u8;
	for (a, b) in left.iter().zip(right) {
		diff |= a ^ b;
	}
	diff == 0
}

#[derive(Debug, Clone)]
pub struct BreakGlassConfig {
	pub enabled: bool,
	pub username: String,
	pub password_hash: String,
}

impl Default for BreakGlassConfig {
	fn default() -> Self {
		Self {
			enabled: false,
			username: "break-glass".into(),
			password_hash: String::new(),
		}
	}
}

const WEAK_ADMIN_PASSWORDS: &[&str] = &["change-me", "dev-only-change-me", "magpie-example-key"];

/// Why an admin password is refused, if it is.
pub fn admin_password_problem(password: &str) -> Option<&'static str> {
	if WEAK_ADMIN_PASSWORDS.contains(&password) {
		return Some("The admin password is a published default.");
	}
	if password.chars().count() < 16 {
		return Some("The admin password must be at least 16 characters.");
	}
	None
}

pub fn hash_password(password: &str) -> Result<String> {
	bcrypt::hash(password, bcrypt::DEFAULT_COST).map_err(|error| Error::Msg(error.to_string()))
}

pub fn verify_break_glass(config: &BreakGlassConfig, username: &str, password: &str) -> Result<()> {
	if !config.enabled {
		return Err(Error::Msg("Break-glass login is disabled.".into()));
	}
	if username != config.username || config.password_hash.is_empty() {
		return Err(Error::Unauthorised);
	}
	let ok = bcrypt::verify(password, &config.password_hash)
		.map_err(|error| Error::Msg(error.to_string()))?;
	if ok { Ok(()) } else { Err(Error::Unauthorised) }
}

#[cfg(test)]
mod tests {
	use super::*;

	#[test]
	fn pkce_challenge_is_s256_of_the_verifier() {
		let pkce = start_pkce();
		let digest = Sha256::digest(pkce.verifier.as_bytes());
		assert_eq!(pkce.challenge, URL_SAFE_NO_PAD.encode(digest));
		let url = authorisation_url(
			"https://login.example/authorize",
			"client",
			"https://portal.example/callback",
			&pkce,
			"openid profile email",
		);
		assert!(url.contains("code_challenge_method=S256"));
		assert!(url.contains(&pkce.state));
	}

	#[test]
	fn session_idle_and_absolute_timeouts() {
		let codec = SessionCodec::new(
			b"example-secret",
			Duration::from_secs(60),
			Duration::from_secs(600),
		);
		let now = Utc::now();
		let (record, cookie) = codec
			.issue(
				"user-1",
				"alex@example.gov.au",
				"Alex",
				vec![Role::Auditor],
				now,
				false,
			)
			.unwrap();
		assert!(
			codec
				.open(&cookie, now + chrono::Duration::seconds(30))
				.is_ok()
		);
		assert!(
			codec
				.open(&cookie, now + chrono::Duration::seconds(120))
				.is_err()
		);
		assert!(
			codec
				.open(
					&cookie,
					record.absolute_expiry + chrono::Duration::seconds(1)
				)
				.is_err()
		);
	}

	#[test]
	fn tampered_cookie_is_rejected() {
		let codec = SessionCodec::new(
			b"example-secret",
			Duration::from_secs(60),
			Duration::from_secs(600),
		);
		let (_, cookie) = codec
			.issue(
				"user-1",
				"a@example.gov.au",
				"A",
				vec![Role::Viewer],
				Utc::now(),
				false,
			)
			.unwrap();
		let mut chars: Vec<u8> = cookie.into_bytes();
		chars[0] ^= 0x1;
		let tampered = String::from_utf8(chars).unwrap();
		assert!(codec.open(&tampered, Utc::now()).is_err());
	}

	#[test]
	fn group_mapping_and_break_glass_default_off() {
		let roles = roles_from_groups(&["magpie-auditors".into()], &RoleMap::default());
		assert_eq!(roles, vec![Role::Auditor]);
		assert!(authorise(&roles, Permission::ExportAudit).is_ok());
		assert!(authorise(&roles, Permission::EditPolicy).is_err());
		let config = BreakGlassConfig::default();
		assert!(!config.enabled);
		assert!(verify_break_glass(&config, "break-glass", "nope").is_err());
	}

	#[test]
	fn hs256_nonce_and_back_channel_logout() {
		let claims = IdClaims {
			sub: "user-1".into(),
			email: Some("alex@example.gov.au".into()),
			name: Some("Alex".into()),
			preferred_username: None,
			groups: vec!["magpie-admins".into()],
			roles: vec![],
			nonce: Some("nonce-1".into()),
			iss: "https://login.example".into(),
			aud: serde_json::json!("magpie"),
			exp: (Utc::now() + chrono::Duration::minutes(5)).timestamp() as usize,
			events: serde_json::json!({}),
		};
		let token = issue_hs256(&claims, b"secret").unwrap();
		let verified = verify_hs256(&token, b"secret", "magpie", Some("nonce-1")).unwrap();
		assert_eq!(verified.sub, "user-1");
		assert!(verify_hs256(&token, b"secret", "magpie", Some("other")).is_err());
		let mut logout = claims.clone();
		logout.events = serde_json::json!({ BACK_CHANNEL_EVENT: {} });
		assert_eq!(logout_subject(&logout).unwrap(), "user-1");
	}

	#[test]
	fn role_map_extends_from_settings() {
		let mut map = RoleMap::default();
		map.extend_from(|name| match name {
			"MAGPIE_ROLE_OWNERS" => Some("1b2c-group, Magpie.Owner".into()),
			_ => None,
		});
		assert_eq!(
			roles_from_groups(&["Magpie.Owner".into()], &map),
			vec![Role::Owner]
		);
		assert_eq!(
			roles_from_groups(&["magpie-owners".into()], &map),
			vec![Role::Owner]
		);
		assert_eq!(
			roles_from_groups(&["other".into()], &map),
			vec![Role::Viewer]
		);
	}

	#[test]
	fn weak_admin_passwords_are_refused() {
		assert!(admin_password_problem("change-me").is_some());
		assert!(admin_password_problem("short").is_some());
		assert!(admin_password_problem("a-long-generated-password-1").is_none());
	}

	const TEST_RSA_PRIVATE: &str = include_str!("../tests/fixtures/oidc-test-key.pem");
	const TEST_RSA_N: &str = include_str!("../tests/fixtures/oidc-test-key.n");

	fn signed(claims: &IdClaims, kid: &str) -> String {
		let mut header = jsonwebtoken::Header::new(jsonwebtoken::Algorithm::RS256);
		header.kid = Some(kid.into());
		jsonwebtoken::encode(
			&header,
			claims,
			&jsonwebtoken::EncodingKey::from_rsa_pem(TEST_RSA_PRIVATE.as_bytes()).unwrap(),
		)
		.unwrap()
	}

	fn test_jwks() -> jsonwebtoken::jwk::JwkSet {
		serde_json::from_value(serde_json::json!({
			"keys": [{"kty": "RSA", "kid": "k1", "use": "sig", "alg": "RS256", "n": TEST_RSA_N.trim(), "e": "AQAB"}]
		}))
		.unwrap()
	}

	fn entra_claims() -> IdClaims {
		IdClaims {
			sub: "user-1".into(),
			email: None,
			name: Some("Alex".into()),
			preferred_username: Some("alex@example.com.au".into()),
			groups: vec![],
			roles: vec!["Magpie.Owner".into()],
			nonce: Some("nonce-1".into()),
			iss: "https://login.example/tenant/v2.0".into(),
			aud: serde_json::json!("client-1"),
			exp: (Utc::now() + chrono::Duration::minutes(5)).timestamp() as usize,
			events: serde_json::json!({}),
		}
	}

	#[test]
	fn jwks_verification_checks_key_issuer_audience_and_nonce() {
		let jwks = test_jwks();
		let issuer = "https://login.example/tenant/v2.0";
		let token = signed(&entra_claims(), "k1");
		let claims = verify_with_jwks(&token, &jwks, issuer, "client-1", Some("nonce-1")).unwrap();
		assert_eq!(
			claims.preferred_username.as_deref(),
			Some("alex@example.com.au")
		);
		assert!(verify_with_jwks(&token, &jwks, issuer, "client-2", Some("nonce-1")).is_err());
		assert!(verify_with_jwks(&token, &jwks, "https://evil.example", "client-1", None).is_err());
		assert!(verify_with_jwks(&token, &jwks, issuer, "client-1", Some("other")).is_err());
		assert!(
			verify_with_jwks(
				&signed(&entra_claims(), "k2"),
				&jwks,
				issuer,
				"client-1",
				None
			)
			.is_err()
		);
	}

	#[test]
	fn jwks_verification_refuses_hmac_tokens() {
		let mut claims = entra_claims();
		claims.nonce = None;
		let mut header = jsonwebtoken::Header::new(jsonwebtoken::Algorithm::HS256);
		header.kid = Some("k1".into());
		let token = jsonwebtoken::encode(
			&header,
			&claims,
			&jsonwebtoken::EncodingKey::from_secret(TEST_RSA_N.trim().as_bytes()),
		)
		.unwrap();
		let error = verify_with_jwks(
			&token,
			&test_jwks(),
			"https://login.example/tenant/v2.0",
			"client-1",
			None,
		)
		.unwrap_err();
		assert!(error.to_string().contains("not accepted"), "{error}");
	}

	#[test]
	fn entra_okta_and_google_issuers() {
		assert!(issuer_for(OidcProviderKind::Entra, "contoso").contains("login.microsoftonline.com"));
		assert!(issuer_for(OidcProviderKind::Okta, "example.okta.com").contains("okta.com"));
		assert_eq!(
			issuer_for(OidcProviderKind::Google, ""),
			"https://accounts.google.com"
		);
	}
}
