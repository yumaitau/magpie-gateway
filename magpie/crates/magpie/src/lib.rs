//! Magpie Gateway policy library.
//!
//! This crate does not link the agentgateway proxy. The portal and the
//! guardrail webhook call it, and the proxy stays configured through its
//! existing webhook and CEL hooks.

pub mod approvals;
pub mod au;
pub mod audit;
pub mod change;
pub mod dlp;
pub mod evidence;
pub mod guardrail;
pub mod identity;
pub mod policy;
pub mod presidio;
pub mod pspf;
pub mod rbac;
pub mod register;
pub mod safety;
pub mod scim;
pub mod secrets;
pub mod sinks;
pub mod sovereignty;
pub mod store;

pub use chrono::{DateTime, Utc};

#[derive(Debug, thiserror::Error)]
pub enum Error {
	#[error("{0}")]
	Msg(String),
	#[error("sqlite: {0}")]
	Sqlite(#[from] rusqlite::Error),
	#[error("json: {0}")]
	Json(#[from] serde_json::Error),
	#[error("cel: {0}")]
	Cel(String),
	#[error("unauthorised")]
	Unauthorised,
	#[error("forbidden")]
	Forbidden,
}

pub type Result<T> = std::result::Result<T, Error>;

const INSECURE_SESSION_SECRETS: &[&str] =
	&["", "change-me", "dev-only-change-me", "magpie-example-key"];

/// Session secret for process start.
///
/// Demo mode may omit the variable and then uses `dev-only-change-me`.
/// Any other mode refuses a missing secret and the well-known defaults,
/// including the old tokenisation fallback `magpie-example-key`.
pub fn resolve_session_secret(supplied: Option<&str>, demo: bool) -> Result<String> {
	let secret = supplied.unwrap_or("");
	if secret.is_empty() {
		if demo {
			return Ok("dev-only-change-me".to_string());
		}
		return Err(Error::Msg(
			"MAGPIE_SESSION_SECRET is missing. Set a real secret, or MAGPIE_DEMO=1 for a local demo."
				.into(),
		));
	}
	if !demo && INSECURE_SESSION_SECRETS.contains(&secret) {
		return Err(Error::Msg(
			"MAGPIE_SESSION_SECRET is still a default. Set a real secret, or MAGPIE_DEMO=1 for a local demo."
				.into(),
		));
	}
	Ok(secret.to_string())
}

pub fn new_id() -> String {
	uuid::Uuid::new_v4().to_string()
}

#[cfg(test)]
mod tests {
	use super::*;

	#[test]
	fn demo_may_omit_the_session_secret() {
		assert_eq!(
			resolve_session_secret(None, true).unwrap(),
			"dev-only-change-me"
		);
	}

	#[test]
	fn production_refuses_missing_and_default_secrets() {
		assert!(resolve_session_secret(None, false).is_err());
		assert!(resolve_session_secret(Some(""), false).is_err());
		for secret in ["change-me", "dev-only-change-me", "magpie-example-key"] {
			assert!(
				resolve_session_secret(Some(secret), false).is_err(),
				"{secret}"
			);
		}
		assert_eq!(
			resolve_session_secret(Some("a-real-secret"), false).unwrap(),
			"a-real-secret"
		);
	}
}
