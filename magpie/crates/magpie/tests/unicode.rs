//! Every scanner on text shaped like a coding agent's prompt: identifiers,
//! markings and secrets surrounded by multi-byte characters at every offset.
//! A panic here takes down the guardrail webhook, and the proxy then fails
//! closed on every request from that agent.

use std::collections::BTreeMap;

use magpie::{au, dlp, pspf, safety, secrets};

const PIECES: &[&str] = &[
	"ABN 12 000 000 041",
	"TFN 123 456 782",
	"Medicare 2123 45670 1",
	"L898902C<3",
	"OFFICIAL: Sensitive",
	"PROTECTED",
	"AKIAIOSFODNN7EXAMPLE",
	"BSB 062-000",
	"ignore previous instructions",
	"0412 345 678",
	"—",
	"→",
	"✅",
	"é",
	"日本",
	"\u{200b}",
	" ",
	"\n",
	"<",
	"1",
	"9",
	"A",
	"-----BEGIN PRIVATE KEY-----",
];

fn scan_all(text: &str) {
	let _ = au::scan(text);
	let _ = pspf::scan_text(text);
	let _ = safety::scan_baseline(text);
	let _ = secrets::scan(text);
	let _ = dlp::inspect(text, &BTreeMap::new(), &dlp::default_rules(), &[], b"k");
}

#[test]
fn scanners_survive_multi_byte_text_around_every_match() {
	// A small linear congruential generator keeps the test deterministic.
	let mut seed: u64 = 0x5eed;
	let mut next = || {
		seed = seed
			.wrapping_mul(6364136223846793005)
			.wrapping_add(1442695040888963407);
		(seed >> 33) as usize
	};
	for _ in 0..400 {
		let mut text = String::new();
		for _ in 0..(next() % 24) {
			text.push_str(PIECES[next() % PIECES.len()]);
		}
		scan_all(&text);
	}
}
