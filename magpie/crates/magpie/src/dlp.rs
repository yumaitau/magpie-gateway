//! DLP actions over recogniser hits.
//!
//! Tokenise stores an HMAC prefix, not a reversible token. Redact and mask
//! drop the matched characters. Hits returned to the UI never include the
//! raw match.

use hmac::{Hmac, Mac};
use serde::{Deserialize, Serialize};
use sha2::Sha256;

use crate::au::{self, AuHit, IdentifierKind};
use crate::presidio::PresidioEntity;
use crate::pspf::{self, Marking};
use crate::secrets::{self, SecretHit, SecretKind};

type HmacSha256 = Hmac<Sha256>;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum Action {
	Allow,
	Log,
	Redact,
	Mask,
	Tokenise,
	Block,
	RequireApproval,
}

impl Action {
	pub fn rank(self) -> u8 {
		match self {
			Self::Allow => 0,
			Self::Log => 1,
			Self::Redact => 2,
			Self::Mask => 3,
			Self::Tokenise => 4,
			Self::RequireApproval => 5,
			Self::Block => 6,
		}
	}

	pub fn as_str(self) -> &'static str {
		match self {
			Self::Allow => "allow",
			Self::Log => "log",
			Self::Redact => "redact",
			Self::Mask => "mask",
			Self::Tokenise => "tokenise",
			Self::RequireApproval => "require approval",
			Self::Block => "block",
		}
	}
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum HitKind {
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
	Marking,
	Caveat,
	Secret,
	Presidio,
}

impl From<IdentifierKind> for HitKind {
	fn from(kind: IdentifierKind) -> Self {
		match kind {
			IdentifierKind::Abn => Self::Abn,
			IdentifierKind::Acn => Self::Acn,
			IdentifierKind::Tfn => Self::Tfn,
			IdentifierKind::Medicare => Self::Medicare,
			IdentifierKind::Crn => Self::Crn,
			IdentifierKind::Passport => Self::Passport,
			IdentifierKind::PassportMrz => Self::PassportMrz,
			IdentifierKind::DriverLicence => Self::DriverLicence,
			IdentifierKind::Bsb => Self::Bsb,
			IdentifierKind::AccountNumber => Self::AccountNumber,
			IdentifierKind::Phone => Self::Phone,
			IdentifierKind::Address => Self::Address,
		}
	}
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct DlpRule {
	pub id: String,
	pub kind: HitKind,
	pub action: Action,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct PublicHit {
	pub kind: HitKind,
	pub action: Action,
	pub start: usize,
	pub end: usize,
	pub preview: String,
	pub note: String,
}

#[derive(Debug, Clone)]
struct Span {
	start: usize,
	end: usize,
	kind: HitKind,
	action: Action,
	note: String,
	replacement: String,
}

#[derive(Debug, Clone)]
pub struct DlpOutcome {
	pub action: Action,
	pub output: String,
	pub hits: Vec<PublicHit>,
	pub highest_marking: Option<Marking>,
}

pub fn default_rules() -> Vec<DlpRule> {
	let kinds = [
		HitKind::Abn,
		HitKind::Acn,
		HitKind::Tfn,
		HitKind::Medicare,
		HitKind::Crn,
		HitKind::Passport,
		HitKind::PassportMrz,
		HitKind::DriverLicence,
		HitKind::Bsb,
		HitKind::AccountNumber,
		HitKind::Phone,
		HitKind::Address,
		HitKind::Secret,
		HitKind::Presidio,
	];
	kinds
		.into_iter()
		.map(|kind| DlpRule {
			id: format!("default-{}", kind_name(kind)),
			kind,
			action: Action::Redact,
		})
		.chain([
			DlpRule {
				id: "default-marking".to_string(),
				kind: HitKind::Marking,
				action: Action::Log,
			},
			DlpRule {
				id: "default-caveat".to_string(),
				kind: HitKind::Caveat,
				action: Action::Log,
			},
		])
		.collect()
}

fn kind_name(kind: HitKind) -> &'static str {
	match kind {
		HitKind::Abn => "abn",
		HitKind::Acn => "acn",
		HitKind::Tfn => "tfn",
		HitKind::Medicare => "medicare",
		HitKind::Crn => "crn",
		HitKind::Passport => "passport",
		HitKind::PassportMrz => "passport-mrz",
		HitKind::DriverLicence => "driver-licence",
		HitKind::Bsb => "bsb",
		HitKind::AccountNumber => "account-number",
		HitKind::Phone => "phone",
		HitKind::Address => "address",
		HitKind::Marking => "marking",
		HitKind::Caveat => "caveat",
		HitKind::Secret => "secret",
		HitKind::Presidio => "presidio",
	}
}

fn action_for(rules: &[DlpRule], kind: HitKind) -> Action {
	rules
		.iter()
		.find(|rule| rule.kind == kind)
		.map(|rule| rule.action)
		.unwrap_or(Action::Log)
}

pub fn inspect(
	text: &str,
	metadata: &std::collections::BTreeMap<String, String>,
	rules: &[DlpRule],
	presidio: &[PresidioEntity],
	token_key: &[u8],
) -> DlpOutcome {
	let rules = if rules.is_empty() {
		default_rules()
	} else {
		rules.to_vec()
	};
	let mut spans = Vec::new();
	for hit in au::scan(text) {
		push_au(
			&mut spans,
			text,
			&hit,
			action_for(&rules, HitKind::from(hit.kind)),
			token_key,
		);
	}
	for hit in secrets::scan(text) {
		push_secret(
			&mut spans,
			text,
			&hit,
			action_for(&rules, HitKind::Secret),
			token_key,
		);
	}
	let text_markings = pspf::scan_text(text);
	let meta_markings = pspf::scan_metadata(metadata);
	let highest = text_markings.highest().max(meta_markings.highest());
	for hit in text_markings.markings {
		let action = action_for(&rules, HitKind::Marking);
		spans.push(span_from_slice(
			text,
			hit.start,
			hit.end,
			HitKind::Marking,
			action,
			hit.marking.as_str(),
			token_key,
		));
	}
	for caveat in text_markings.caveats {
		spans.push(Span {
			start: 0,
			end: 0,
			kind: HitKind::Caveat,
			action: action_for(&rules, HitKind::Caveat),
			note: caveat.as_str().to_string(),
			replacement: caveat.as_str().to_string(),
		});
	}
	for entity in presidio {
		let end = entity.end.min(text.len());
		let start = entity.start.min(end);
		let action = action_for(&rules, HitKind::Presidio);
		spans.push(span_from_slice(
			text,
			start,
			end,
			HitKind::Presidio,
			action,
			&format!("presidio:{}", entity.entity_type),
			token_key,
		));
	}
	spans.sort_by_key(|span| (span.start, std::cmp::Reverse(span.end)));
	let mut chosen: Vec<Span> = Vec::new();
	for span in spans {
		if span.end > span.start
			&& chosen
				.iter()
				.any(|earlier| span.start < earlier.end && span.end > earlier.start)
		{
			continue;
		}
		chosen.push(span);
	}
	let action = chosen
		.iter()
		.map(|span| span.action)
		.max_by_key(|action| action.rank())
		.unwrap_or(Action::Allow);
	let output = if matches!(action, Action::Block | Action::RequireApproval) {
		String::new()
	} else {
		apply_replacements(text, &chosen)
	};
	let hits = chosen
		.iter()
		.map(|span| PublicHit {
			kind: span.kind,
			action: span.action,
			start: span.start,
			end: span.end,
			preview: preview(&span.replacement, span.action),
			note: span.note.clone(),
		})
		.collect();
	DlpOutcome {
		action,
		output,
		hits,
		highest_marking: highest,
	}
}

fn preview(replacement: &str, action: Action) -> String {
	match action {
		Action::Allow | Action::Log => "[logged, text withheld]".to_string(),
		_ => replacement.to_string(),
	}
}

fn push_au(spans: &mut Vec<Span>, text: &str, hit: &AuHit, action: Action, token_key: &[u8]) {
	spans.push(span_from_slice(
		text,
		hit.start,
		hit.end,
		HitKind::from(hit.kind),
		action,
		hit.note,
		token_key,
	));
}

fn push_secret(
	spans: &mut Vec<Span>,
	text: &str,
	hit: &SecretHit,
	action: Action,
	token_key: &[u8],
) {
	let note = match hit.kind {
		SecretKind::PrivateKey => "private key",
		SecretKind::AwsAccessKey => "cloud access key shape",
		SecretKind::GitHubToken => "source host token shape",
		SecretKind::SlackToken => "chat token shape",
		SecretKind::BearerOrApiKey => "api key assignment",
		SecretKind::Jwt => "jwt shape",
	};
	spans.push(span_from_slice(
		text,
		hit.start,
		hit.end,
		HitKind::Secret,
		action,
		note,
		token_key,
	));
}

fn span_from_slice(
	text: &str,
	start: usize,
	end: usize,
	kind: HitKind,
	action: Action,
	note: &str,
	token_key: &[u8],
) -> Span {
	let matched = text.get(start..end).unwrap_or("");
	let replacement = match action {
		Action::Redact | Action::Block | Action::RequireApproval => "[redacted]".to_string(),
		Action::Mask => mask(matched),
		Action::Tokenise => tokenise(kind, matched, token_key),
		Action::Allow | Action::Log => String::new(),
	};
	Span {
		start,
		end,
		kind,
		action,
		note: note.to_string(),
		replacement,
	}
}

fn mask(value: &str) -> String {
	let chars: Vec<char> = value.chars().collect();
	if chars.len() <= 2 {
		return "*".repeat(chars.len());
	}
	let mut out = "*".repeat(chars.len() - 2);
	out.push(chars[chars.len() - 2]);
	out.push(chars[chars.len() - 1]);
	out
}

fn tokenise(kind: HitKind, value: &str, key: &[u8]) -> String {
	// An empty key used to fall back to a well-known HMAC key. That made
	// every token guessable. Redact instead of minting one.
	if key.is_empty() {
		return "[redacted]".to_string();
	}
	let mut mac = HmacSha256::new_from_slice(key).expect("hmac key");
	mac.update(kind_name(kind).as_bytes());
	mac.update(b":");
	mac.update(value.as_bytes());
	let bytes = mac.finalize().into_bytes();
	format!(
		"TOK_{}_{}",
		kind_name(kind).to_ascii_uppercase(),
		hex8(&bytes[..4])
	)
}

fn hex8(bytes: &[u8]) -> String {
	const HEX: &[u8; 16] = b"0123456789abcdef";
	let mut out = String::new();
	for byte in bytes {
		out.push(HEX[(byte >> 4) as usize] as char);
		out.push(HEX[(byte & 0x0f) as usize] as char);
	}
	out
}

fn apply_replacements(text: &str, spans: &[Span]) -> String {
	let mut ordered: Vec<&Span> = spans
		.iter()
		.filter(|span| span.end > span.start && !span.replacement.is_empty())
		.collect();
	ordered.sort_by_key(|span| span.start);
	let mut out = String::new();
	let mut cursor = 0;
	for span in ordered {
		if span.start < cursor {
			continue;
		}
		out.push_str(&text[cursor..span.start]);
		out.push_str(&span.replacement);
		cursor = span.end;
	}
	out.push_str(&text[cursor..]);
	out
}

#[cfg(test)]
mod tests {
	use super::*;

	#[test]
	fn redacts_abn_and_keeps_the_raw_value_out_of_the_hit() {
		let text = "Please file ABN 12 000 000 041 today.";
		let outcome = inspect(
			text,
			&Default::default(),
			&default_rules(),
			&[],
			b"test-key",
		);
		assert!(outcome.output.contains("[redacted]"));
		assert!(!outcome.output.contains("12 000 000 041"));
		assert!(outcome.hits.iter().any(|hit| hit.kind == HitKind::Abn));
		let encoded = serde_json::to_string(&outcome.hits).unwrap();
		assert!(!encoded.contains("12000000041"));
		assert!(!encoded.contains("12 000 000 041"));
	}

	#[test]
	fn tokenise_is_stable_and_not_the_original() {
		let mut rules = default_rules();
		for rule in &mut rules {
			if rule.kind == HitKind::Acn {
				rule.action = Action::Tokenise;
			}
		}
		let text = "ACN 123 456 780";
		let first = inspect(text, &Default::default(), &rules, &[], b"test-key");
		let second = inspect(text, &Default::default(), &rules, &[], b"test-key");
		assert_eq!(first.output, second.output);
		assert!(first.output.contains("TOK_ACN_"));
		assert!(!first.output.contains("123"));
	}

	#[test]
	fn empty_token_key_does_not_use_a_well_known_key() {
		let mut rules = default_rules();
		for rule in &mut rules {
			if rule.kind == HitKind::Acn {
				rule.action = Action::Tokenise;
			}
		}
		let text = "ACN 123 456 780";
		let empty = inspect(text, &Default::default(), &rules, &[], b"");
		let example = inspect(
			text,
			&Default::default(),
			&rules,
			&[],
			b"magpie-example-key",
		);
		assert!(!empty.output.contains("TOK_"), "{}", empty.output);
		assert!(empty.output.contains("[redacted]"));
		assert!(example.output.contains("TOK_ACN_"));
		assert_ne!(empty.output, example.output);
	}

	#[test]
	fn block_wins_over_redact() {
		let mut rules = default_rules();
		for rule in &mut rules {
			if rule.kind == HitKind::Secret {
				rule.action = Action::Block;
			}
		}
		let text = "ABN 12 000 000 041 AKIAIOSFODNN7EXAMPLE";
		let outcome = inspect(text, &Default::default(), &rules, &[], b"k");
		assert_eq!(outcome.action, Action::Block);
		assert!(outcome.output.is_empty());
	}
}
