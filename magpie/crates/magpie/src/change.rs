//! Versioned config changes, optional second-person approval, and rollback.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use serde_json::Value;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum RevisionStatus {
	Pending,
	Active,
	Rejected,
	RolledBack,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct Revision {
	pub id: String,
	pub kind: String,
	pub doc_id: String,
	pub actor: String,
	pub created_at: DateTime<Utc>,
	pub before: Value,
	pub after: Value,
	pub status: RevisionStatus,
	pub approver: Option<String>,
}

pub fn summarise_diff(before: &Value, after: &Value) -> Vec<String> {
	let mut lines = Vec::new();
	walk("", before, after, &mut lines);
	lines
}

fn walk(path: &str, before: &Value, after: &Value, lines: &mut Vec<String>) {
	if before == after {
		return;
	}
	match (before, after) {
		(Value::Object(left), Value::Object(right)) => {
			let mut keys: Vec<&String> = left.keys().chain(right.keys()).collect();
			keys.sort();
			keys.dedup();
			for key in keys {
				let next = if path.is_empty() {
					format!("/{key}")
				} else {
					format!("{path}/{key}")
				};
				walk(
					&next,
					left.get(key).unwrap_or(&Value::Null),
					right.get(key).unwrap_or(&Value::Null),
					lines,
				);
			}
		},
		_ => lines.push(format!("{path}: {before} -> {after}")),
	}
}

pub fn propose(
	kind: &str,
	doc_id: &str,
	actor: &str,
	before: Value,
	after: Value,
	require_approval: bool,
	now: DateTime<Utc>,
) -> Revision {
	Revision {
		id: crate::new_id(),
		kind: kind.to_string(),
		doc_id: doc_id.to_string(),
		actor: actor.to_string(),
		created_at: now,
		before,
		after,
		status: if require_approval {
			RevisionStatus::Pending
		} else {
			RevisionStatus::Active
		},
		approver: None,
	}
}

pub fn approve(revision: &mut Revision, approver: &str) -> crate::Result<()> {
	if revision.status != RevisionStatus::Pending {
		return Err(crate::Error::Msg(
			"Only a pending revision can be approved.".into(),
		));
	}
	if approver == revision.actor {
		return Err(crate::Error::Msg(
			"The person who wrote the change cannot approve it.".into(),
		));
	}
	if approver.trim().is_empty() {
		return Err(crate::Error::Msg("An approver is required.".into()));
	}
	revision.status = RevisionStatus::Active;
	revision.approver = Some(approver.to_string());
	Ok(())
}

pub fn rollback_of(latest_active: &Revision, actor: &str, now: DateTime<Utc>) -> Revision {
	Revision {
		id: crate::new_id(),
		kind: latest_active.kind.clone(),
		doc_id: latest_active.doc_id.clone(),
		actor: actor.to_string(),
		created_at: now,
		before: latest_active.after.clone(),
		after: latest_active.before.clone(),
		status: RevisionStatus::Active,
		approver: None,
	}
}

#[cfg(test)]
mod tests {
	use super::*;
	use serde_json::json;

	#[test]
	fn second_person_and_diff_and_rollback() {
		let before = json!({"action": "log"});
		let after = json!({"action": "block"});
		let mut revision = propose("dlp", "rule-1", "alex", before, after, true, Utc::now());
		assert_eq!(revision.status, RevisionStatus::Pending);
		assert!(approve(&mut revision, "alex").is_err());
		approve(&mut revision, "sam").unwrap();
		assert_eq!(revision.status, RevisionStatus::Active);
		let diff = summarise_diff(&revision.before, &revision.after);
		assert!(diff.iter().any(|line| line.contains("block")));
		let rolled = rollback_of(&revision, "sam", Utc::now());
		assert_eq!(rolled.after, json!({"action": "log"}));
	}
}
