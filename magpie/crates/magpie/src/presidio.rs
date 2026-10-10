//! Presidio analyzer JSON, merged by the DLP engine.
//!
//! The portal calls the community images under ghcr.io/data-privacy-stack
//! when an analyzer URL is configured. This module only builds and parses
//! the JSON so the checksum recognisers do not depend on Presidio being up.
//! Microsoft's mcr.microsoft.com/presidio images are not referenced. Upstream
//! Presidio says those tags are no longer updated.

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PresidioEntity {
	pub entity_type: String,
	pub start: usize,
	pub end: usize,
	pub score: f32,
}

pub fn analyzer_request(text: &str) -> serde_json::Value {
	serde_json::json!({
		"text": text,
		"language": "en"
	})
}

pub fn anonymizer_request(text: &str, entities: &[PresidioEntity]) -> serde_json::Value {
	let analyzer_results: Vec<serde_json::Value> = entities
		.iter()
		.map(|entity| {
			serde_json::json!({
				"entity_type": entity.entity_type,
				"start": entity.start,
				"end": entity.end,
				"score": entity.score
			})
		})
		.collect();
	serde_json::json!({
		"text": text,
		"analyzer_results": analyzer_results,
		"anonymizers": {
			"DEFAULT": { "type": "replace", "new_value": "[redacted]" }
		}
	})
}

pub fn parse_analyzer_response(body: &str) -> crate::Result<Vec<PresidioEntity>> {
	let value: serde_json::Value = serde_json::from_str(body)?;
	let items = value
		.as_array()
		.ok_or_else(|| crate::Error::Msg("Presidio analyzer response was not a list".into()))?;
	let mut entities = Vec::new();
	for item in items {
		let entity_type = item
			.get("entity_type")
			.and_then(|value| value.as_str())
			.unwrap_or("UNKNOWN")
			.to_string();
		let start = item
			.get("start")
			.and_then(|value| value.as_u64())
			.unwrap_or(0) as usize;
		let end = item
			.get("end")
			.and_then(|value| value.as_u64())
			.unwrap_or(0) as usize;
		let score = item
			.get("score")
			.and_then(|value| value.as_f64())
			.unwrap_or(0.0) as f32;
		entities.push(PresidioEntity {
			entity_type,
			start,
			end,
			score,
		});
	}
	Ok(entities)
}

pub const ANALYZER_IMAGE: &str = "ghcr.io/data-privacy-stack/presidio-analyzer:latest";
pub const ANONYMIZER_IMAGE: &str = "ghcr.io/data-privacy-stack/presidio-anonymizer:latest";

#[cfg(test)]
mod tests {
	use super::*;

	#[test]
	fn parses_an_analyzer_list() {
		let body = r#"[{"entity_type":"EMAIL_ADDRESS","start":0,"end":7,"score":0.8}]"#;
		let entities = parse_analyzer_response(body).unwrap();
		assert_eq!(entities[0].entity_type, "EMAIL_ADDRESS");
		assert_eq!(entities[0].end, 7);
	}

	#[test]
	fn images_are_the_community_registry() {
		assert!(ANALYZER_IMAGE.starts_with("ghcr.io/data-privacy-stack/"));
		assert!(!ANALYZER_IMAGE.contains("mcr.microsoft.com"));
		assert!(!ANONYMIZER_IMAGE.contains("mcr.microsoft.com"));
	}
}
