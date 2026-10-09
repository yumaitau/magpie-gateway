//! AI use-case register fields from the DTA Standard for accountability 2.0.
//!
//! The page https://www.digital.gov.au/ai/ai-in-government-policy/accountability
//! was last updated 2 December 2025. The keys match that list. Name and email
//! are split the same way as jusso-dev/crownguard `src/engine/aiRegister.ts`.
//! High-risk reviews are included. They are required when the inherent risk
//! is high. Magpie stores them either way so an export is not missing columns.

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct UseCase {
	pub name: String,
	pub reference: String,
	pub description: String,
	pub technology: String,
	pub lifecycle: String,
	pub technical_standard: String,
	pub domains: String,
	pub usage_patterns: String,
	pub owner_name: String,
	pub owner_email: String,
	pub criteria: String,
	pub inherent_risk: String,
	pub residual_risk: String,
	pub impact_assessment_date: String,
	pub last_review: String,
	pub next_review: String,
}

impl UseCase {
	pub fn example() -> Self {
		Self {
			name: "Example briefing assistant".into(),
			reference: "EXAMPLE-UC-001".into(),
			description: "Example only. Drafts a weekly briefing from approved internal notes.".into(),
			technology: "Generative AI".into(),
			lifecycle: "Discover".into(),
			technical_standard: "partially applied".into(),
			domains: "Corporate and shared services".into(),
			usage_patterns: "Internal productivity".into(),
			owner_name: "Alex Nguyen".into(),
			owner_email: "alex.nguyen@example.gov.au".into(),
			criteria: "Example criterion from Appendix C, not a real assessment.".into(),
			inherent_risk: "medium".into(),
			residual_risk: "low".into(),
			impact_assessment_date: "2026-09-01".into(),
			last_review: "2026-09-01".into(),
			next_review: "2027-03-01".into(),
		}
	}
}

pub const CSV_HEADERS: &[&str] = &[
	"name",
	"reference",
	"description",
	"technology",
	"lifecycle",
	"technicalStandard",
	"domains",
	"usagePatterns",
	"ownerName",
	"ownerEmail",
	"criteria",
	"inherentRisk",
	"residualRisk",
	"impactAssessmentDate",
	"lastReview",
	"nextReview",
];

pub fn to_csv(cases: &[UseCase]) -> String {
	let mut writer = csv::Writer::from_writer(vec![]);
	writer.write_record(CSV_HEADERS).expect("header");
	for case in cases {
		writer
			.write_record([
				case.name.as_str(),
				case.reference.as_str(),
				case.description.as_str(),
				case.technology.as_str(),
				case.lifecycle.as_str(),
				case.technical_standard.as_str(),
				case.domains.as_str(),
				case.usage_patterns.as_str(),
				case.owner_name.as_str(),
				case.owner_email.as_str(),
				case.criteria.as_str(),
				case.inherent_risk.as_str(),
				case.residual_risk.as_str(),
				case.impact_assessment_date.as_str(),
				case.last_review.as_str(),
				case.next_review.as_str(),
			])
			.expect("row");
	}
	String::from_utf8(writer.into_inner().expect("csv")).expect("utf8")
}

pub fn from_csv(text: &str) -> crate::Result<Vec<UseCase>> {
	let mut reader = csv::ReaderBuilder::new()
		.flexible(false)
		.from_reader(text.as_bytes());
	let headers = reader
		.headers()
		.map_err(|error| crate::Error::Msg(error.to_string()))?
		.clone();
	for expected in CSV_HEADERS {
		if !headers.iter().any(|header| header == *expected) {
			return Err(crate::Error::Msg(format!(
				"CSV is missing the '{expected}' column."
			)));
		}
	}
	let mut cases = Vec::new();
	for record in reader.records() {
		let record = record.map_err(|error| crate::Error::Msg(error.to_string()))?;
		let get = |name: &str| -> String {
			headers
				.iter()
				.position(|header| header == name)
				.and_then(|index| record.get(index))
				.unwrap_or("")
				.to_string()
		};
		cases.push(UseCase {
			name: get("name"),
			reference: get("reference"),
			description: get("description"),
			technology: get("technology"),
			lifecycle: get("lifecycle"),
			technical_standard: get("technicalStandard"),
			domains: get("domains"),
			usage_patterns: get("usagePatterns"),
			owner_name: get("ownerName"),
			owner_email: get("ownerEmail"),
			criteria: get("criteria"),
			inherent_risk: get("inherentRisk"),
			residual_risk: get("residualRisk"),
			impact_assessment_date: get("impactAssessmentDate"),
			last_review: get("lastReview"),
			next_review: get("nextReview"),
		});
	}
	Ok(cases)
}

#[cfg(test)]
mod tests {
	use super::*;

	#[test]
	fn csv_round_trip_keeps_the_dta_columns() {
		let exported = to_csv(&[UseCase::example()]);
		for header in CSV_HEADERS {
			assert!(exported.contains(header), "{header}");
		}
		let parsed = from_csv(&exported).unwrap();
		assert_eq!(parsed[0], UseCase::example());
	}
}
