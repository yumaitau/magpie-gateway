//! Credential-shaped strings in tool arguments and outputs.
//!
//! These are shape checks. A match is not proof that the value is a live secret.

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum SecretKind {
	PrivateKey,
	AwsAccessKey,
	GitHubToken,
	SlackToken,
	BearerOrApiKey,
	Jwt,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SecretHit {
	pub kind: SecretKind,
	pub start: usize,
	pub end: usize,
}

pub fn scan(text: &str) -> Vec<SecretHit> {
	let patterns: &[(&str, SecretKind)] = &[
		(
			r"-----BEGIN [A-Z ]*PRIVATE KEY-----",
			SecretKind::PrivateKey,
		),
		(r"\bAKIA[0-9A-Z]{16}\b", SecretKind::AwsAccessKey),
		(r"\bgithub_pat_[A-Za-z0-9_]{20,}\b", SecretKind::GitHubToken),
		(r"\bghp_[A-Za-z0-9]{20,}\b", SecretKind::GitHubToken),
		(r"\bxox[baprs]-[A-Za-z0-9-]{10,}\b", SecretKind::SlackToken),
		(
			r#"(?i)\b(?:api[_-]?key|token|secret)\b\s*[:=]\s*['"]?[A-Za-z0-9_-]{16,}"#,
			SecretKind::BearerOrApiKey,
		),
		(
			r"\beyJ[A-Za-z0-9_-]{8,}\.[A-Za-z0-9_-]{8,}\.[A-Za-z0-9_-]{8,}",
			SecretKind::Jwt,
		),
	];
	let mut hits = Vec::new();
	for (pattern, kind) in patterns {
		let expression = regex::Regex::new(pattern).expect("secret pattern");
		for found in expression.find_iter(text) {
			hits.push(SecretHit {
				kind: *kind,
				start: found.start(),
				end: found.end(),
			});
		}
	}
	hits.sort_by_key(|hit| hit.start);
	hits
}

#[cfg(test)]
mod tests {
	use super::*;

	#[test]
	fn finds_key_shapes_and_ignores_ordinary_words() {
		let text = "-----BEGIN PRIVATE KEY-----\nsecret\nAKIAIOSFODNN7EXAMPLE token=abcdefghijklmnopqrstuvwxyz eyJhbGciOiJIUzI1NiJ9.eyJzdWIiOiIxIn0.s3cr3ts3cr3ts3cr3t";
		let hits = scan(text);
		assert!(hits.iter().any(|hit| hit.kind == SecretKind::PrivateKey));
		assert!(hits.iter().any(|hit| hit.kind == SecretKind::AwsAccessKey));
		assert!(
			hits
				.iter()
				.any(|hit| hit.kind == SecretKind::BearerOrApiKey)
		);
		assert!(hits.iter().any(|hit| hit.kind == SecretKind::Jwt));
		assert!(scan("the secret of good tea").is_empty());
	}
}
