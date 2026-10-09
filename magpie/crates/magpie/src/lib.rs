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

pub fn new_id() -> String {
	uuid::Uuid::new_v4().to_string()
}
