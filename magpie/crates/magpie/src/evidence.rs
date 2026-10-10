//! Monthly evidence rows for the ISM, the Essential Eight, and the DTA AI policy.
//!
//! Every row says whether the link is direct or interpretive. A gateway does
//! not, on its own, satisfy an Essential Eight strategy or an ISM control.

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct EvidenceRow {
	pub framework: String,
	pub reference: String,
	pub source: String,
	pub magpie_control: String,
	pub observation: String,
	pub relationship: String,
}

#[derive(Debug, Clone, Default)]
pub struct MonthlyStats {
	pub requests: u64,
	pub blocks: u64,
	pub dlp_hits: u64,
	pub policy_changes: u64,
	pub logins: u64,
	pub approvals: u64,
	pub sovereign_denies: u64,
}

pub fn monthly_rows(stats: &MonthlyStats) -> Vec<EvidenceRow> {
	vec![
		EvidenceRow {
			framework: "ISM September 2026".into(),
			reference: "ISM-2159".into(),
			source: "https://www.cyber.gov.au/ism/2026-09".into(),
			magpie_control: "Audit event for each tool call, model request and decision.".into(),
			observation: format!("{} request events in the period.", stats.requests),
			relationship: "Interpretive. The September 2026 changes introduce ISM-2159, recommending that tool invocations, external requests and outputs of agentic AI applications be centrally logged. Magpie produces those events. Central collection is the sink the operator turns on.".into(),
		},
		EvidenceRow {
			framework: "ISM September 2026".into(),
			reference: "ISM-1988".into(),
			source: "https://www.cyber.gov.au/ism/2026-09/guidelines-for-security-assurance".into(),
			magpie_control: "Retention setting, default 365 days, prune function, searchable audit export.".into(),
			observation: "Retention is a setting. This report does not prove the logs were kept for 12 months.".into(),
			relationship: "Interpretive. ISM-1988 recommends searchable retention of at least 12 months. The product can be configured for that. It does not by itself meet the control.".into(),
		},
		EvidenceRow {
			framework: "ISM September 2026".into(),
			reference: "ISM-1985".into(),
			source: "https://www.cyber.gov.au/ism/2026-09/guidelines-for-security-assurance".into(),
			magpie_control: "SQLite triggers reject updates and deletes of audit events.".into(),
			observation: "The application refuses modification. A database administrator can still remove the file.".into(),
			relationship: "Interpretive. Protecting logs from unauthorised modification also depends on the platform.".into(),
		},
		EvidenceRow {
			framework: "Essential Eight (November 2023 model)".into(),
			reference: "Multi-factor authentication".into(),
			source: "https://www.cyber.gov.au/business-government/asds-cyber-security-frameworks/essential-eight/essential-eight-maturity-model-faq".into(),
			magpie_control: "Portal login is OIDC. Phishing-resistant MFA is the IdP's job.".into(),
			observation: format!("{} portal logins in the period.", stats.logins),
			relationship: "Interpretive. Essential Eight MFA maturity is assessed on the identity provider and the endpoints, not on this gateway. The FAQ still points at the November 2023 model.".into(),
		},
		EvidenceRow {
			framework: "Essential Eight (November 2023 model)".into(),
			reference: "Application control".into(),
			source: "https://www.cyber.gov.au/business-government/asds-cyber-security-frameworks/essential-eight/essential-eight-maturity-model-faq".into(),
			magpie_control: "Tool allow list and kill switch for agents. Windows App Control guidance is a separate document.".into(),
			observation: format!("{} blocked decisions.", stats.blocks),
			relationship: "Interpretive. Restricting MCP tools is not workstation application control. The Windows guide covers that separately and does not claim a maturity level.".into(),
		},
		EvidenceRow {
			framework: "Essential Eight (November 2023 model)".into(),
			reference: "Restrict administrative privileges".into(),
			source: "https://www.cyber.gov.au/business-government/asds-cyber-security-frameworks/essential-eight/essential-eight-maturity-model-faq".into(),
			magpie_control: "Portal RBAC. Break-glass is off by default and audited.".into(),
			observation: "Role checks are enforced on portal routes.".into(),
			relationship: "Interpretive. Portal roles are not a privileged-access workstation design.".into(),
		},
		EvidenceRow {
			framework: "DTA Policy for the responsible use of AI in government 2.0".into(),
			reference: "AI use case accountability".into(),
			source: "https://www.digital.gov.au/ai/ai-in-government-policy".into(),
			magpie_control: "Use-case register import and export using the Standard for accountability 2.0 fields.".into(),
			observation: format!("{} approval decisions.", stats.approvals),
			relationship: "Direct on the field list. The policy still requires the agency's own register, accountable official and impact assessment. Magpie stores what it is given.".into(),
		},
		EvidenceRow {
			framework: "Agentic AI addendum (4 June 2026)".into(),
			reference: "AGT.3 and AGT.7".into(),
			source: "https://www.digital.gov.au/policy/ai/agentic-ai-addendum".into(),
			magpie_control: "Default deny for unapproved tools, approval queue, kill switch.".into(),
			observation: format!("{} policy changes, {} blocks.", stats.policy_changes, stats.blocks),
			relationship: "Interpretive. The addendum is best-practice guidance. It asks for least privilege, high-impact approvals and kill switches. It does not name this product.".into(),
		},
		EvidenceRow {
			framework: "Agentic AI addendum (4 June 2026)".into(),
			reference: "AGT.4".into(),
			source: "https://www.digital.gov.au/policy/ai/agentic-ai-addendum-statements-data".into(),
			magpie_control: "Classification routing and sovereign mode.".into(),
			observation: format!("{} sovereign or residency denies, {} DLP hits.", stats.sovereign_denies, stats.dlp_hits),
			relationship: "Interpretive. AGT.4.1 is a must for authentication, encryption, audit, classification and sovereignty on agent exchange. Magpie covers classification routing, audit and residency checks. Encryption in transit is the deployment, not this report.".into(),
		},
		EvidenceRow {
			framework: "Agentic AI addendum (4 June 2026)".into(),
			reference: "AGT.8".into(),
			source: "https://www.digital.gov.au/policy/ai/agentic-ai-addendum-statements-monitor".into(),
			magpie_control: "Audit search, detection queries, DLP hit review.".into(),
			observation: format!("{} DLP hits.", stats.dlp_hits),
			relationship: "Interpretive. Monitoring for unauthorised tool use is a must. Queries are provided. Somebody still has to look at them.".into(),
		},
	]
}

pub fn to_csv(rows: &[EvidenceRow]) -> String {
	let mut writer = csv::Writer::from_writer(vec![]);
	writer
		.write_record([
			"framework",
			"reference",
			"source",
			"magpieControl",
			"observation",
			"relationship",
		])
		.expect("header");
	for row in rows {
		writer
			.write_record([
				row.framework.as_str(),
				row.reference.as_str(),
				row.source.as_str(),
				row.magpie_control.as_str(),
				row.observation.as_str(),
				row.relationship.as_str(),
			])
			.expect("row");
	}
	String::from_utf8(writer.into_inner().expect("csv")).expect("utf8")
}

pub fn to_pdf(title: &str, rows: &[EvidenceRow]) -> Vec<u8> {
	let mut lines = vec![
		title.to_string(),
		"Example operational report. Mappings are labelled.".to_string(),
		String::new(),
	];
	for row in rows {
		lines.push(format!("{} {}", row.framework, row.reference));
		lines.push(row.observation.clone());
		lines.push(format!("Relationship: {}", row.relationship));
		lines.push(format!("Source: {}", row.source));
		lines.push(String::new());
	}
	simple_pdf(&lines)
}

fn simple_pdf(lines: &[String]) -> Vec<u8> {
	let mut pages: Vec<String> = Vec::new();
	let mut current: Vec<String> = Vec::new();
	for line in lines {
		let wrapped = wrap(line, 90);
		for piece in wrapped {
			current.push(piece);
			if current.len() == 46 {
				pages.push(render_page(&current));
				current.clear();
			}
		}
	}
	if !current.is_empty() || pages.is_empty() {
		pages.push(render_page(&current));
	}
	assemble(&pages)
}

fn wrap(line: &str, width: usize) -> Vec<String> {
	if line.is_empty() {
		return vec![String::new()];
	}
	let mut out = Vec::new();
	let mut rest = line;
	while rest.len() > width {
		let mut cut = width;
		while cut > 0 && !rest.is_char_boundary(cut) {
			cut -= 1;
		}
		out.push(rest[..cut].to_string());
		rest = &rest[cut..];
	}
	out.push(rest.to_string());
	out
}

fn render_page(lines: &[String]) -> String {
	let mut body = String::from("BT\n/F1 10 Tf\n50 800 Td\n14 TL\n");
	for line in lines {
		body.push_str(&format!("({}) '\n", pdf_escape(line)));
	}
	body.push_str("ET\n");
	body
}

fn pdf_escape(text: &str) -> String {
	text
		.replace('\\', "\\\\")
		.replace('(', "\\(")
		.replace(')', "\\)")
}

fn assemble(pages: &[String]) -> Vec<u8> {
	let mut objects: Vec<Vec<u8>> = Vec::new();
	let page_count = pages.len();
	let mut kids = String::new();
	for index in 0..page_count {
		let object_number = 3 + index * 2;
		kids.push_str(&format!("{object_number} 0 R "));
	}
	objects.push(b"<< /Type /Catalog /Pages 2 0 R >>".to_vec());
	objects.push(format!("<< /Type /Pages /Count {page_count} /Kids [{kids}] >>").into_bytes());
	for page in pages {
		let contents = format!("<< /Length {} >>\nstream\n{}endstream", page.len(), page);
		objects.push(
			"<< /Type /Page /Parent 2 0 R /MediaBox [0 0 595 842] /Contents 0 0 R /Resources << /Font << /F1 0 0 R >> >> >>"
				.to_string()
				.into_bytes(),
		);
		objects.push(contents.into_bytes());
	}
	objects.push("<< /Type /Font /Subtype /Type1 /BaseFont /Helvetica >>".into());
	// Fix content and font references now that numbers are known.
	let font_number = objects.len();
	for (object_index, object) in objects.iter_mut().enumerate() {
		let text = String::from_utf8_lossy(object).to_string();
		if text.contains("/Type /Page ") {
			let content_number = object_index + 2;
			let fixed = text
				.replace(
					"/Contents 0 0 R",
					&format!("/Contents {content_number} 0 R"),
				)
				.replace("/F1 0 0 R", &format!("/F1 {font_number} 0 R"));
			*object = fixed.into_bytes();
		}
	}
	let mut output = Vec::new();
	output.extend_from_slice(b"%PDF-1.4\n");
	let mut offsets = vec![0usize];
	for (index, object) in objects.iter().enumerate() {
		offsets.push(output.len());
		let header = format!("{} 0 obj\n", index + 1);
		output.extend_from_slice(header.as_bytes());
		output.extend_from_slice(object);
		output.extend_from_slice(b"\nendobj\n");
	}
	let xref_at = output.len();
	let xref = format!("xref\n0 {}\n0000000000 65535 f \n", objects.len() + 1);
	output.extend_from_slice(xref.as_bytes());
	for offset in offsets.iter().skip(1) {
		output.extend_from_slice(format!("{offset:010} 00000 n \n").as_bytes());
	}
	let trailer = format!(
		"trailer << /Size {} /Root 1 0 R >>\nstartxref\n{xref_at}\n%%EOF\n",
		objects.len() + 1
	);
	output.extend_from_slice(trailer.as_bytes());
	output
}

#[cfg(test)]
mod tests {
	use super::*;

	#[test]
	fn report_labels_interpretive_rows_and_makes_a_pdf() {
		let rows = monthly_rows(&MonthlyStats {
			requests: 4,
			blocks: 1,
			dlp_hits: 2,
			..MonthlyStats::default()
		});
		assert!(rows.iter().any(|row| row.reference == "ISM-2159"));
		assert!(rows.iter().all(|row| {
			row
				.relationship
				.to_ascii_lowercase()
				.contains("interpretive")
				|| row.relationship.contains("Direct")
		}));
		let csv = to_csv(&rows);
		assert!(csv.contains("ISM-2159"));
		let pdf = to_pdf("Magpie Gateway evidence, October 2026", &rows);
		assert!(pdf.starts_with(b"%PDF-1.4"));
		let text = String::from_utf8_lossy(&pdf);
		assert!(text.contains("%%EOF"));
		assert!(text.contains("ISM-2159") || text.contains("ISM"));
	}
}
