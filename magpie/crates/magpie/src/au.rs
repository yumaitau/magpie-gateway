//! Australian identifier recognisers.
//!
//! Checksums are implemented only where an official algorithm was found.
//! Everything else is a format or labelled-field match, and the code says so.

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum IdentifierKind {
	Abn,
	Acn,
	Tfn,
	Medicare,
	Crn,
	Passport,
	PassportMrz,
	DriverLicence,
	Bsb,
	AccountNumber,
	Phone,
	Address,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum Confidence {
	/// Checksum matched a published algorithm.
	Checksum,
	/// Shape matched a published format. No checksum exists, or none was present.
	Format,
	/// A labelled field with no published format for that jurisdiction.
	Labelled,
	/// Street, state and postcode sitting together. Not a geocoder.
	Heuristic,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AuHit {
	pub kind: IdentifierKind,
	pub start: usize,
	pub end: usize,
	pub confidence: Confidence,
	pub note: &'static str,
}

/// ABN modulus 89. Source: Australian Business Register, "Format of the ABN"
/// <https://abr.business.gov.au/Help/AbnFormat>.
pub fn abn_valid(digits: &[u8]) -> bool {
	if digits.len() != 11 || digits.iter().any(|d| *d > 9) {
		return false;
	}
	let weights = [10, 1, 3, 5, 7, 9, 11, 13, 15, 17, 19];
	let mut sum = 0i32;
	for (index, weight) in weights.iter().enumerate() {
		let mut value = digits[index] as i32;
		if index == 0 {
			value -= 1;
		}
		sum += value * weight;
	}
	sum % 89 == 0
}

/// ACN modified modulus 10. Source: ASIC Datastream Messages Specification
/// Version 2.80, Appendix B.
pub fn acn_valid(digits: &[u8]) -> bool {
	if digits.len() != 9 || digits.iter().any(|d| *d > 9) {
		return false;
	}
	let weights = [8, 7, 6, 5, 4, 3, 2, 1];
	let sum: i32 = digits[..8]
		.iter()
		.zip(weights)
		.map(|(digit, weight)| *digit as i32 * weight)
		.sum();
	let check = (10 - (sum % 10)) % 10;
	check == digits[8] as i32
}

/// Medicare card check digit. Source: Australian Digital Health Agency,
/// "Use of HIs in Health Software Systems, Conformance Requirements" v3.0,
/// Appendix E (a NEHTA-era profile). This is not a Services Australia page.
/// The first digit must be 2 to 6. The ninth digit is the check digit.
/// The tenth is the issue number. An eleventh individual reference number
/// is optional and is not part of the checksum.
pub fn medicare_valid(digits: &[u8]) -> bool {
	if digits.len() != 10 && digits.len() != 11 {
		return false;
	}
	if digits.iter().any(|d| *d > 9) || !(2..=6).contains(&digits[0]) {
		return false;
	}
	let weights = [1, 3, 7, 9, 1, 3, 7, 9];
	let sum: i32 = digits[..8]
		.iter()
		.zip(weights)
		.map(|(digit, weight)| *digit as i32 * weight)
		.sum();
	sum % 10 == digits[8] as i32
}

/// ICAO Doc 9303 Part 3 check digit. Weights 7, 3, 1. Digits are themselves,
/// A to Z are 10 to 35, and `<` is 0.
pub fn mrz_check_digit(data: &str) -> Option<u8> {
	let weights = [7u32, 3, 1];
	let mut sum = 0u32;
	for (index, character) in data.chars().enumerate() {
		let value = match character {
			'0'..='9' => character as u32 - '0' as u32,
			'A'..='Z' => character as u32 - 'A' as u32 + 10,
			'<' => 0,
			_ => return None,
		};
		sum += value * weights[index % 3];
	}
	Some((sum % 10) as u8)
}

#[derive(Debug, Clone, Copy)]
pub struct Jurisdiction {
	pub code: &'static str,
	pub name: &'static str,
	pub checksum: &'static str,
	pub format: &'static str,
	pub citation: &'static str,
}

/// What each jurisdiction publishes. Patterns are not invented to fill gaps.
pub fn driver_licence_jurisdictions() -> &'static [Jurisdiction] {
	&[
		Jurisdiction {
			code: "VIC",
			name: "Victoria",
			checksum: "No official checksum was found.",
			format: "Usually nine digits.",
			citation: "https://www.vic.gov.au/dgs-proof-identity-verification-user-guide",
		},
		Jurisdiction {
			code: "NSW",
			name: "New South Wales",
			checksum: "No official checksum was found.",
			format: "Service NSW describes a licence number and a separate card number. It does not state a digit length in the pages reviewed.",
			citation: "https://www.service.nsw.gov.au/guide/data-breaches",
		},
		Jurisdiction {
			code: "QLD",
			name: "Queensland",
			checksum: "No official checksum was found.",
			format: "No official format statement was found.",
			citation: "https://austroads.gov.au/drivers-and-vehicles/registration-and-licensing/australian-driver-licensing",
		},
		Jurisdiction {
			code: "SA",
			name: "South Australia",
			checksum: "No official checksum was found.",
			format: "No official format statement was found.",
			citation: "https://austroads.gov.au/drivers-and-vehicles/registration-and-licensing/australian-driver-licensing",
		},
		Jurisdiction {
			code: "WA",
			name: "Western Australia",
			checksum: "No official checksum was found.",
			format: "No official format statement was found.",
			citation: "https://austroads.gov.au/drivers-and-vehicles/registration-and-licensing/australian-driver-licensing",
		},
		Jurisdiction {
			code: "TAS",
			name: "Tasmania",
			checksum: "No official checksum was found.",
			format: "No official format statement was found.",
			citation: "https://austroads.gov.au/drivers-and-vehicles/registration-and-licensing/australian-driver-licensing",
		},
		Jurisdiction {
			code: "NT",
			name: "Northern Territory",
			checksum: "No official checksum was found.",
			format: "No official format statement was found.",
			citation: "https://austroads.gov.au/drivers-and-vehicles/registration-and-licensing/australian-driver-licensing",
		},
		Jurisdiction {
			code: "ACT",
			name: "Australian Capital Territory",
			checksum: "No official checksum was found.",
			format: "No official format statement was found.",
			citation: "https://austroads.gov.au/drivers-and-vehicles/registration-and-licensing/australian-driver-licensing",
		},
	]
}

struct Run {
	digits: String,
	start: usize,
	end: usize,
	separated: bool,
}

fn digit_runs(text: &str) -> Vec<Run> {
	let bytes = text.as_bytes();
	let mut runs = Vec::new();
	let mut index = 0;
	while index < bytes.len() {
		if !bytes[index].is_ascii_digit() {
			index += 1;
			continue;
		}
		let start = index;
		let mut digits = String::new();
		let mut separated = false;
		while index < bytes.len() {
			if bytes[index].is_ascii_digit() {
				digits.push(bytes[index] as char);
				index += 1;
				continue;
			}
			let separator = bytes[index] == b' ' || bytes[index] == b'-';
			let next_digit = index + 1 < bytes.len() && bytes[index + 1].is_ascii_digit();
			if separator && next_digit && !digits.is_empty() && digits.len() < 12 {
				separated = true;
				index += 1;
				continue;
			}
			break;
		}
		let end = index;
		if (6..=11).contains(&digits.len()) {
			runs.push(Run {
				digits,
				start,
				end,
				separated,
			});
		}
	}
	runs
}

fn window(text: &str, start: usize, end: usize) -> String {
	let mut from = start.saturating_sub(64);
	while !text.is_char_boundary(from) {
		from -= 1;
	}
	let mut to = (end + 64).min(text.len());
	while !text.is_char_boundary(to) {
		to += 1;
	}
	text[from..to].to_ascii_lowercase()
}

fn contains_any(window: &str, words: &[&str]) -> bool {
	words.iter().any(|word| window.contains(word))
}

fn push_hit(
	hits: &mut Vec<AuHit>,
	kind: IdentifierKind,
	run: &Run,
	confidence: Confidence,
	note: &'static str,
) {
	if hits
		.iter()
		.any(|hit| hit.start == run.start && hit.end == run.end && hit.kind == kind)
	{
		return;
	}
	hits.push(AuHit {
		kind,
		start: run.start,
		end: run.end,
		confidence,
		note,
	});
}

pub fn scan(text: &str) -> Vec<AuHit> {
	let mut hits = Vec::new();
	for run in digit_runs(text) {
		let around = window(text, run.start, run.end);
		let digits: Vec<u8> = run.digits.bytes().map(|byte| byte - b'0').collect();
		if digits.len() == 11
			&& abn_valid(&digits)
			&& (run.separated || contains_any(&around, &["abn", "australian business number"]))
		{
			push_hit(
				&mut hits,
				IdentifierKind::Abn,
				&run,
				Confidence::Checksum,
				"ABN modulus 89, ABR format note",
			);
		}
		if digits.len() == 9
			&& acn_valid(&digits)
			&& (run.separated || contains_any(&around, &["acn", "australian company number"]))
		{
			push_hit(
				&mut hits,
				IdentifierKind::Acn,
				&run,
				Confidence::Checksum,
				"ACN modified modulus 10, ASIC Datastream Appendix B",
			);
		}
		if (digits.len() == 10 || digits.len() == 11)
			&& medicare_valid(&digits)
			&& contains_any(&around, &["medicare"])
		{
			push_hit(
				&mut hits,
				IdentifierKind::Medicare,
				&run,
				Confidence::Checksum,
				"Medicare check digit from the ADHA conformance profile, Appendix E",
			);
		}
		if (digits.len() == 8 || digits.len() == 9) && contains_any(&around, &["tfn", "tax file"]) {
			push_hit(
				&mut hits,
				IdentifierKind::Tfn,
				&run,
				Confidence::Format,
				"TFN length only. The ATO does not publish the checksum.",
			);
		}
		if digits.len() == 6
			&& (run.separated || contains_any(&around, &["bsb"]))
			&& contains_any(&around, &["bsb", "bank state branch"])
		{
			push_hit(
				&mut hits,
				IdentifierKind::Bsb,
				&run,
				Confidence::Format,
				"BSB is six digits. AusPayNet publishes no account-number checksum.",
			);
		}
	}
	scan_crn(text, &mut hits);
	scan_passport(text, &mut hits);
	scan_mrz(text, &mut hits);
	scan_licence(text, &mut hits);
	scan_phone(text, &mut hits);
	scan_address(text, &mut hits);
	scan_account(text, &mut hits);
	hits.sort_by_key(|hit| (hit.start, hit.end));
	hits
}

fn scan_crn(text: &str, hits: &mut Vec<AuHit>) {
	let lower = text.to_ascii_lowercase();
	if !lower.contains("crn")
		&& !lower.contains("centrelink")
		&& !lower.contains("customer reference")
	{
		return;
	}
	let bytes = text.as_bytes();
	for run in digit_runs(text) {
		if run.digits.len() != 9 || run.end >= bytes.len() || !bytes[run.end].is_ascii_alphabetic() {
			continue;
		}
		let end = run.end + 1;
		let around = window(text, run.start, end);
		if contains_any(&around, &["crn", "centrelink", "customer reference"]) {
			hits.push(AuHit {
				kind: IdentifierKind::Crn,
				start: run.start,
				end,
				confidence: Confidence::Format,
				note: "CRN is 9 digits and a letter. Services Australia does not publish a checksum.",
			});
		}
	}
}

fn scan_passport(text: &str, hits: &mut Vec<AuHit>) {
	let bytes = text.as_bytes();
	let mut index = 0;
	while index < bytes.len() {
		if bytes[index].is_ascii_alphabetic() && index + 9 <= bytes.len() {
			let letters = if index + 1 < bytes.len() && bytes[index + 1].is_ascii_alphabetic() {
				2
			} else {
				1
			};
			let digit_at = index + letters;
			if digit_at + 7 <= bytes.len()
				&& bytes[digit_at..digit_at + 7]
					.iter()
					.all(|b| b.is_ascii_digit())
			{
				let number_end = digit_at + 7;
				let around = window(text, index, number_end);
				if contains_any(&around, &["passport"]) {
					hits.push(AuHit {
						kind: IdentifierKind::Passport,
						start: index,
						end: number_end,
						confidence: Confidence::Format,
						note: "Printed passport number shape. DFAT cites ICAO 9303. The check digit applies to the MRZ, not this printed form.",
					});
				}
				if number_end + 1 < bytes.len()
					&& (bytes[number_end] == b'<' || bytes[number_end].is_ascii_digit())
				{
					let field_end = if bytes[number_end] == b'<' {
						number_end + 1
					} else {
						number_end
					};
					// 9-character MRZ field then a check digit.
					if field_end < bytes.len() && bytes[field_end].is_ascii_digit() {
						let field = &text[index..field_end];
						let padded = if field.chars().count() == 8 {
							format!("{field}<")
						} else {
							field.to_string()
						};
						if padded.chars().count() == 9
							&& mrz_check_digit(&padded.to_ascii_uppercase()) == Some(bytes[field_end] - b'0')
						{
							hits.push(AuHit {
								kind: IdentifierKind::PassportMrz,
								start: index,
								end: field_end + 1,
								confidence: Confidence::Checksum,
								note: "ICAO Doc 9303 modulus 10 check digit, weights 7, 3, 1.",
							});
						}
					}
				}
				index = number_end;
				continue;
			}
		}
		index += 1;
	}
}

fn scan_mrz(text: &str, hits: &mut Vec<AuHit>) {
	let bytes = text.as_bytes();
	let mut index = 0;
	while index + 10 <= bytes.len() {
		let field_bytes = &bytes[index..index + 9];
		let check = bytes[index + 9];
		// Check bytes first: slicing the str here could split a multi-byte character.
		let field_ok = field_bytes
			.iter()
			.all(|byte| byte.is_ascii_uppercase() || byte.is_ascii_digit() || *byte == b'<');
		let field = if field_ok {
			std::str::from_utf8(field_bytes).unwrap_or("")
		} else {
			""
		};
		if field_ok
			&& check.is_ascii_digit()
			&& field.contains('<')
			&& mrz_check_digit(field) == Some(check - b'0')
		{
			hits.push(AuHit {
				kind: IdentifierKind::PassportMrz,
				start: index,
				end: index + 10,
				confidence: Confidence::Checksum,
				note: "ICAO Doc 9303 modulus 10 check digit, weights 7, 3, 1.",
			});
			index += 10;
			continue;
		}
		index += 1;
	}
}

fn scan_licence(text: &str, hits: &mut Vec<AuHit>) {
	let lower = text.to_ascii_lowercase();
	if !lower.contains("licence") && !lower.contains("license") {
		return;
	}
	let vic = lower.contains("victoria") || lower.contains("vic");
	for run in digit_runs(text) {
		let around = window(text, run.start, run.end);
		if !contains_any(&around, &["licence", "license"]) {
			continue;
		}
		if vic && run.digits.len() == 9 {
			push_hit(
				hits,
				IdentifierKind::DriverLicence,
				&run,
				Confidence::Format,
				"Victoria usually uses nine digits. No official checksum was found.",
			);
		} else if contains_any(
			&around,
			&[
				"nsw",
				"qld",
				"sa",
				"wa",
				"tas",
				"nt",
				"act",
				"new south wales",
				"queensland",
			],
		) {
			push_hit(
				hits,
				IdentifierKind::DriverLicence,
				&run,
				Confidence::Labelled,
				"Labelled driver licence number. That jurisdiction has not published a checksum or a digit length we can rely on.",
			);
		}
	}
}

fn scan_phone(text: &str, hits: &mut Vec<AuHit>) {
	// ACMA numbering plan patterns. Require a separator or a +61 prefix so a
	// bare digit string is not treated as a phone number.
	let pattern = regex::Regex::new(
		r"(?x)
		(?:\+61[\s-]?\(?0?\)?[\s-]?[2-4789](?:[\s-]?\d){8})
		| (?:\b0[2378](?:[\s-]\d){8}\b)
		| (?:\b04(?:[\s-]?\d){8}\b)
		| (?:\b09(?:[\s-]?\d){8}\b)
		| (?:\b1300(?:[\s-]?\d){6}\b)
		| (?:\b1800(?:[\s-]?\d){6}\b)
		| (?:\b13(?:[\s-]\d){4}\b)
		| (?:\b180(?:[\s-]\d){4}\b)
		",
	)
	.expect("phone pattern");
	for found in pattern.find_iter(text) {
		hits.push(AuHit {
			kind: IdentifierKind::Phone,
			start: found.start(),
			end: found.end(),
			confidence: Confidence::Format,
			note: "ACMA numbering plan shape (geographic, mobile, 13/1300/180/1800, or 09).",
		});
	}
}

fn scan_address(text: &str, hits: &mut Vec<AuHit>) {
	let pattern = regex::Regex::new(
		r"(?i)\b(?:street|st|road|rd|avenue|ave|drive|dr|court|ct|parade|crescent|lane|place|terrace|highway|way)\b.{0,40}\b(?:NSW|VIC|QLD|SA|WA|TAS|NT|ACT)\b\s+\d{4}\b",
	)
	.expect("address pattern");
	for found in pattern.find_iter(text) {
		hits.push(AuHit {
			kind: IdentifierKind::Address,
			start: found.start(),
			end: found.end(),
			confidence: Confidence::Heuristic,
			note: "Street suffix, state and postcode. This is a heuristic, not an official address parser.",
		});
	}
}

fn scan_account(text: &str, hits: &mut Vec<AuHit>) {
	if !text.to_ascii_lowercase().contains("account") {
		return;
	}
	for run in digit_runs(text) {
		if (6..=10).contains(&run.digits.len()) {
			let around = window(text, run.start, run.end);
			if contains_any(&around, &["account"]) && !contains_any(&around, &["bsb"]) {
				push_hit(
					hits,
					IdentifierKind::AccountNumber,
					&run,
					Confidence::Format,
					"Account number length only. AusPayNet does not publish a checksum.",
				);
			}
		}
	}
}

#[cfg(test)]
mod tests {
	#[test]
	fn multi_byte_characters_never_split_a_slice() {
		// Coding agents send em dashes, arrows and emoji next to numbers.
		let base =
			"ABN 12 000 000 041 and passport L898902C<3 — see TFN 123 456 782 → Medicare 2123 45670 1 ✅";
		for at in 0..=base.len() {
			if !base.is_char_boundary(at) {
				continue;
			}
			for insert in ["—", "→", "✅", "é"] {
				let text = format!("{}{insert}{}", &base[..at], &base[at..]);
				let _ = super::scan(&text);
				let padded = format!("{}{text}", "—".repeat(30));
				let _ = super::scan(&padded);
			}
		}
	}

	use super::*;

	#[test]
	fn abn_vector_matches_the_abr_weights() {
		assert!(abn_valid(&[1, 2, 0, 0, 0, 0, 0, 0, 0, 4, 1]));
		assert!(abn_valid(&[1, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0]));
		assert!(!abn_valid(&[1, 2, 0, 0, 0, 0, 0, 0, 0, 4, 2]));
	}

	#[test]
	fn acn_vector_matches_the_asic_weights() {
		assert!(acn_valid(&[1, 2, 3, 4, 5, 6, 7, 8, 0]));
		assert!(!acn_valid(&[1, 2, 3, 4, 5, 6, 7, 8, 1]));
	}

	#[test]
	fn medicare_vector_matches_the_adha_weights() {
		assert!(medicare_valid(&[2, 4, 2, 8, 7, 7, 8, 1, 3, 1]));
		assert!(!medicare_valid(&[1, 4, 2, 8, 7, 7, 8, 1, 3, 1]));
		assert!(!medicare_valid(&[2, 4, 2, 8, 7, 7, 8, 1, 4, 1]));
	}

	#[test]
	fn icao_sample_check_digit() {
		assert_eq!(mrz_check_digit("L898902C<"), Some(3));
	}

	#[test]
	fn scanner_finds_published_checksums_and_skips_tfn_checksum() {
		let text = "Example ABN 12 000 000 041 and ACN 123 456 780 and Medicare 2428 7781 3 1. TFN 12345678. BSB 123-456 account 12345678.";
		let hits = scan(text);
		assert!(
			hits
				.iter()
				.any(|hit| hit.kind == IdentifierKind::Abn && hit.confidence == Confidence::Checksum)
		);
		assert!(hits.iter().any(|hit| hit.kind == IdentifierKind::Acn));
		assert!(hits.iter().any(|hit| hit.kind == IdentifierKind::Medicare));
		assert!(
			hits
				.iter()
				.any(|hit| hit.kind == IdentifierKind::Tfn && hit.confidence == Confidence::Format)
		);
		assert!(hits.iter().any(|hit| hit.kind == IdentifierKind::Bsb));
		assert!(hits.iter().any(|hit| hit.note.contains("does not publish")));
	}

	#[test]
	fn crn_passport_phone_address_and_victoria_licence() {
		let text = "Centrelink CRN 123 456 789A. Passport N1234567. Call 0412 345 678. 1 Example Street VIC 2600. Victoria driver licence 123456789.";
		let hits = scan(text);
		assert!(hits.iter().any(|hit| hit.kind == IdentifierKind::Crn));
		assert!(hits.iter().any(|hit| hit.kind == IdentifierKind::Passport));
		assert!(hits.iter().any(|hit| hit.kind == IdentifierKind::Phone));
		assert!(hits.iter().any(|hit| hit.kind == IdentifierKind::Address));
		assert!(
			hits.iter().any(
				|hit| hit.kind == IdentifierKind::DriverLicence && hit.confidence == Confidence::Format
			)
		);
	}

	#[test]
	fn mrz_checksum_is_detected() {
		let hits = scan("MRZ L898902C<3");
		assert!(
			hits
				.iter()
				.any(|hit| hit.kind == IdentifierKind::PassportMrz)
		);
	}

	#[test]
	fn every_state_and_territory_is_listed_without_a_made_up_checksum() {
		let rows = driver_licence_jurisdictions();
		assert_eq!(rows.len(), 8);
		assert!(
			rows
				.iter()
				.all(|row| row.checksum.contains("No official checksum"))
		);
		assert!(
			rows
				.iter()
				.any(|row| row.code == "VIC" && row.format.contains("nine"))
		);
	}
}
