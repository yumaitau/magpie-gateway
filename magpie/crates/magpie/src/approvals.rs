//! Approval queue for MCP servers, tools and models.
//!
//! Unapproved names are denied. Approval produces the CEL allow list the
//! proxy can attach, and a record that includes the DTA use-case fields.

use chrono::NaiveDate;
use serde::{Deserialize, Serialize};

use crate::policy::tool_allow_list;
use crate::register::UseCase;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum ItemKind {
	McpServer,
	Tool,
	Model,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum ApprovalStatus {
	Pending,
	Approved,
	Rejected,
	Expired,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct Approval {
	pub id: String,
	pub kind: ItemKind,
	pub name: String,
	pub business_owner: String,
	pub purpose: String,
	pub classification: String,
	pub expires_on: String,
	pub status: ApprovalStatus,
	pub use_case: UseCase,
}

pub fn validate(approval: &Approval) -> crate::Result<()> {
	if approval.name.trim().is_empty()
		|| approval.business_owner.trim().is_empty()
		|| approval.purpose.trim().is_empty()
	{
		return Err(crate::Error::Msg(
			"An approval needs a name, a business owner and a purpose.".into(),
		));
	}
	if approval.classification.trim().is_empty() {
		return Err(crate::Error::Msg(
			"An approval needs a data classification.".into(),
		));
	}
	NaiveDate::parse_from_str(&approval.expires_on, "%Y-%m-%d")
		.map_err(|_| crate::Error::Msg("Expiry must be a date in YYYY-MM-DD form.".into()))?;
	if approval.use_case.reference.trim().is_empty()
		|| approval.use_case.owner_email.trim().is_empty()
	{
		return Err(crate::Error::Msg(
			"The use-case register entry needs an agency identifier and an owner email.".into(),
		));
	}
	Ok(())
}

pub fn effective_status(approval: &Approval, today: NaiveDate) -> ApprovalStatus {
	if approval.status != ApprovalStatus::Approved {
		return approval.status;
	}
	match NaiveDate::parse_from_str(&approval.expires_on, "%Y-%m-%d") {
		Ok(expiry) if expiry >= today => ApprovalStatus::Approved,
		_ => ApprovalStatus::Expired,
	}
}

pub fn is_approved(items: &[Approval], kind: ItemKind, name: &str, today: NaiveDate) -> bool {
	items.iter().any(|item| {
		item.kind == kind
			&& item.name == name
			&& effective_status(item, today) == ApprovalStatus::Approved
	})
}

pub fn cel_for_approved_tools(items: &[Approval], today: NaiveDate) -> String {
	let names: Vec<String> = items
		.iter()
		.filter(|item| {
			item.kind == ItemKind::Tool && effective_status(item, today) == ApprovalStatus::Approved
		})
		.map(|item| item.name.clone())
		.collect();
	tool_allow_list(&names)
}

#[cfg(test)]
mod tests {
	use super::*;
	use crate::register::UseCase;

	fn sample(status: ApprovalStatus, expires_on: &str) -> Approval {
		Approval {
			id: "example".into(),
			kind: ItemKind::Tool,
			name: "example_search".into(),
			business_owner: "Alex Nguyen".into(),
			purpose: "Example search over approved notes.".into(),
			classification: "OFFICIAL".into(),
			expires_on: expires_on.into(),
			status,
			use_case: UseCase::example(),
		}
	}

	#[test]
	fn pending_and_expired_tools_are_not_approved() {
		let today = NaiveDate::from_ymd_opt(2026, 10, 9).unwrap();
		let items = vec![
			sample(ApprovalStatus::Pending, "2027-01-01"),
			sample(ApprovalStatus::Approved, "2026-01-01"),
			sample(ApprovalStatus::Approved, "invalid"),
		];
		assert!(!is_approved(
			&items,
			ItemKind::Tool,
			"example_search",
			today
		));
		assert_eq!(cel_for_approved_tools(&items, today), "false");
	}

	#[test]
	fn approved_tool_generates_an_allow_list() {
		let today = NaiveDate::from_ymd_opt(2026, 10, 9).unwrap();
		let items = vec![sample(ApprovalStatus::Approved, "2027-01-01")];
		assert!(is_approved(&items, ItemKind::Tool, "example_search", today));
		assert_eq!(
			cel_for_approved_tools(&items, today),
			"mcp.tool.name in ['example_search']"
		);
	}
}
