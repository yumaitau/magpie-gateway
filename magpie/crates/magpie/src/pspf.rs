//! Protective markings from the current PSPF guidance.
//!
//! Sources used on 9 October 2026:
//! - Australian Government Style Manual, "Security classifications and
//!   protective markings", updated 23 September 2026. It says it matches the
//!   2026 PSPF, sections 9.2 to 9.6.
//!   <https://www.stylemanual.gov.au/writing-and-designing-content/security-classifications-and-protective-markings>
//! - PSPF Policy 8 (2018.8) still appears in search indexes and uses
//!   "or higher". The 2026 files were not fully opened. Where the two
//!   differ, the code follows the Style Manual and says so.
//!
//! OFFICIAL and UNOFFICIAL are not security classifications. The order
//! below is Magpie's operational order so "and above" has a meaning. It is
//! not a number printed in the PSPF.

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum Marking {
	Unofficial = 0,
	Official = 1,
	OfficialSensitive = 2,
	Protected = 3,
	Secret = 4,
	TopSecret = 5,
}

impl Marking {
	pub fn as_str(self) -> &'static str {
		match self {
			Self::Unofficial => "UNOFFICIAL",
			Self::Official => "OFFICIAL",
			Self::OfficialSensitive => "OFFICIAL: Sensitive",
			Self::Protected => "PROTECTED",
			Self::Secret => "SECRET",
			Self::TopSecret => "TOP SECRET",
		}
	}

	pub fn is_security_classification(self) -> bool {
		matches!(
			self,
			Self::OfficialSensitive | Self::Protected | Self::Secret | Self::TopSecret
		)
	}

	pub fn at_least(self, floor: Self) -> bool {
		self as u8 >= floor as u8
	}

	pub fn parse(text: &str) -> Option<Self> {
		match text.trim() {
			"UNOFFICIAL" => Some(Self::Unofficial),
			"OFFICIAL" => Some(Self::Official),
			"OFFICIAL: Sensitive" | "OFFICIAL:Sensitive" => Some(Self::OfficialSensitive),
			"PROTECTED" => Some(Self::Protected),
			"SECRET" => Some(Self::Secret),
			"TOP SECRET" => Some(Self::TopSecret),
			_ => None,
		}
	}
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum Caveat {
	ExclusiveFor,
	Cabinet,
	Austeo,
	Agao,
	Rel,
}

impl Caveat {
	pub fn as_str(self) -> &'static str {
		match self {
			Self::ExclusiveFor => "EXCLUSIVE FOR",
			Self::Cabinet => "CABINET",
			Self::Austeo => "AUSTEO",
			Self::Agao => "AGAO",
			Self::Rel => "REL",
		}
	}
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum InfoMarker {
	LegalPrivilege,
	LegislativeSecrecy,
	PersonalPrivacy,
}

impl InfoMarker {
	pub fn as_str(self) -> &'static str {
		match self {
			Self::LegalPrivilege => "Legal Privilege",
			Self::LegislativeSecrecy => "Legislative Secrecy",
			Self::PersonalPrivacy => "Personal Privacy",
		}
	}
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MarkingHit {
	pub marking: Marking,
	pub start: usize,
	pub end: usize,
	/// True when the match was `OFFICIAL:Sensitive` without the Style Manual space.
	pub spacing_variant: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProtectiveMarking {
	pub markings: Vec<MarkingHit>,
	pub caveats: Vec<Caveat>,
	pub info_markers: Vec<InfoMarker>,
}

impl ProtectiveMarking {
	pub fn highest(&self) -> Option<Marking> {
		self.markings.iter().map(|hit| hit.marking).max()
	}
}

const MARKING_PATTERNS: &[(&str, Marking, bool)] = &[
	("OFFICIAL: Sensitive", Marking::OfficialSensitive, false),
	("OFFICIAL:Sensitive", Marking::OfficialSensitive, true),
	("TOP SECRET", Marking::TopSecret, false),
	("PROTECTED", Marking::Protected, false),
	("SECRET", Marking::Secret, false),
	("UNOFFICIAL", Marking::Unofficial, false),
	("OFFICIAL", Marking::Official, false),
];

fn occupied(hits: &[MarkingHit], start: usize, end: usize) -> bool {
	hits.iter().any(|hit| start < hit.end && end > hit.start)
}

pub fn scan_text(text: &str) -> ProtectiveMarking {
	let mut markings = Vec::new();
	for (pattern, marking, variant) in MARKING_PATTERNS {
		let mut from = 0;
		while let Some(found) = text[from..].find(pattern) {
			let start = from + found;
			let end = start + pattern.len();
			if boundary_ok(text, start, end) && !occupied(&markings, start, end) {
				markings.push(MarkingHit {
					marking: *marking,
					start,
					end,
					spacing_variant: *variant,
				});
			}
			from = end;
		}
	}
	markings.sort_by_key(|hit| hit.start);
	ProtectiveMarking {
		markings,
		caveats: scan_caveats(text),
		info_markers: scan_info_markers(text),
	}
}

fn boundary_ok(text: &str, start: usize, end: usize) -> bool {
	let before = if start == 0 {
		true
	} else {
		!text.as_bytes()[start - 1].is_ascii_alphanumeric()
	};
	let after = if end >= text.len() {
		true
	} else {
		!text.as_bytes()[end].is_ascii_alphanumeric()
	};
	before && after
}

fn scan_caveats(text: &str) -> Vec<Caveat> {
	let mut found = Vec::new();
	for (pattern, caveat) in [
		("EXCLUSIVE FOR", Caveat::ExclusiveFor),
		("CABINET", Caveat::Cabinet),
		("AUSTEO", Caveat::Austeo),
		("AGAO", Caveat::Agao),
	] {
		if let Some(start) = text.find(pattern) {
			let end = start + pattern.len();
			if boundary_ok(text, start, end) {
				found.push(caveat);
			}
		}
	}
	if regex::Regex::new(r"\bREL\b").expect("rel").is_match(text) {
		found.push(Caveat::Rel);
	}
	found
}

fn scan_info_markers(text: &str) -> Vec<InfoMarker> {
	let mut found = Vec::new();
	for (pattern, marker) in [
		("Legal Privilege", InfoMarker::LegalPrivilege),
		("Legislative Secrecy", InfoMarker::LegislativeSecrecy),
		("Personal Privacy", InfoMarker::PersonalPrivacy),
	] {
		if text.contains(pattern) {
			found.push(marker);
		}
	}
	found
}

pub fn scan_metadata(metadata: &std::collections::BTreeMap<String, String>) -> ProtectiveMarking {
	let mut combined = ProtectiveMarking {
		markings: Vec::new(),
		caveats: Vec::new(),
		info_markers: Vec::new(),
	};
	for (key, value) in metadata {
		let key_lower = key.to_ascii_lowercase();
		if key_lower.contains("marking")
			|| key_lower.contains("classification")
			|| key_lower.contains("caveat")
		{
			if let Some(marking) = Marking::parse(value) {
				combined.markings.push(MarkingHit {
					marking,
					start: 0,
					end: 0,
					spacing_variant: value == "OFFICIAL:Sensitive",
				});
			}
			let scanned = scan_text(value);
			combined.caveats.extend(scanned.caveats);
			combined.info_markers.extend(scanned.info_markers);
			if combined.markings.is_empty() {
				combined.markings.extend(scanned.markings);
			}
		}
	}
	combined
}

#[cfg(test)]
mod tests {
	use super::*;

	#[test]
	fn sensitive_does_not_also_count_as_plain_official() {
		let found = scan_text("OFFICIAL: Sensitive briefing");
		assert_eq!(found.markings.len(), 1);
		assert_eq!(found.highest(), Some(Marking::OfficialSensitive));
		assert!(found.highest().unwrap().is_security_classification());
	}

	#[test]
	fn plain_official_is_not_a_security_classification() {
		let found = scan_text("OFFICIAL record");
		assert_eq!(found.highest(), Some(Marking::Official));
		assert!(!Marking::Official.is_security_classification());
		assert!(!Marking::Official.at_least(Marking::OfficialSensitive));
	}

	#[test]
	fn protected_and_above_and_caveats() {
		let found = scan_text("PROTECTED//AUSTEO and CABINET, REL AUS/NZL");
		assert!(found.highest().unwrap().at_least(Marking::Protected));
		assert!(found.caveats.contains(&Caveat::Austeo));
		assert!(found.caveats.contains(&Caveat::Cabinet));
		assert!(found.caveats.contains(&Caveat::Rel));
	}

	#[test]
	fn secretariat_is_not_secret() {
		let found = scan_text("the secretariat met");
		assert!(found.markings.is_empty());
	}

	#[test]
	fn metadata_marking_is_read() {
		let mut metadata = std::collections::BTreeMap::new();
		metadata.insert("x-protective-marking".to_string(), "PROTECTED".to_string());
		let found = scan_metadata(&metadata);
		assert_eq!(found.highest(), Some(Marking::Protected));
	}

	#[test]
	fn info_markers_are_not_classifications() {
		let found = scan_text("Personal Privacy");
		assert!(found.markings.is_empty());
		assert_eq!(found.info_markers, vec![InfoMarker::PersonalPrivacy]);
	}
}
