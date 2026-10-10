//! SQLite control-plane store. Postgres uses the same tables via the schema
//! in `POSTGRES_SCHEMA`. The portal build that talks to Postgres is selected
//! with a `postgres://` URL in the portal crate. This file's tested path is
//! SQLite, including the triggers that reject audit updates and deletes.

use chrono::{DateTime, Utc};
use rusqlite::{Connection, params};
use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::audit::{AuditEvent, AuditQuery};
use crate::change::{Revision, RevisionStatus};
use crate::{Error, Result};

pub const SQLITE_SCHEMA: &str = r#"
CREATE TABLE IF NOT EXISTS documents (
  kind TEXT NOT NULL,
  id TEXT NOT NULL,
  body TEXT NOT NULL,
  revision INTEGER NOT NULL,
  updated_at TEXT NOT NULL,
  updated_by TEXT NOT NULL,
  active INTEGER NOT NULL DEFAULT 1,
  PRIMARY KEY (kind, id)
);
CREATE TABLE IF NOT EXISTS revisions (
  id TEXT PRIMARY KEY,
  kind TEXT NOT NULL,
  doc_id TEXT NOT NULL,
  actor TEXT NOT NULL,
  created_at TEXT NOT NULL,
  before_json TEXT NOT NULL,
  after_json TEXT NOT NULL,
  status TEXT NOT NULL,
  approver TEXT
);
CREATE TABLE IF NOT EXISTS audit_events (
  id TEXT PRIMARY KEY,
  at TEXT NOT NULL,
  kind TEXT NOT NULL,
  actor TEXT NOT NULL,
  agent_id TEXT NOT NULL,
  tool TEXT NOT NULL,
  model TEXT NOT NULL,
  decision TEXT NOT NULL,
  reason TEXT NOT NULL,
  detail TEXT NOT NULL
);
CREATE TRIGGER IF NOT EXISTS audit_events_no_update
BEFORE UPDATE ON audit_events
BEGIN
  SELECT RAISE(ABORT, 'audit events are immutable');
END;
CREATE TRIGGER IF NOT EXISTS audit_events_no_delete
BEFORE DELETE ON audit_events
BEGIN
  SELECT RAISE(ABORT, 'audit events are immutable');
END;
CREATE TABLE IF NOT EXISTS sessions (
  id TEXT PRIMARY KEY,
  subject TEXT NOT NULL,
  body TEXT NOT NULL,
  absolute_expiry TEXT NOT NULL
);
"#;

pub const POSTGRES_SCHEMA: &str = r#"
CREATE TABLE IF NOT EXISTS documents (
  kind TEXT NOT NULL,
  id TEXT NOT NULL,
  body TEXT NOT NULL,
  revision INTEGER NOT NULL,
  updated_at TEXT NOT NULL,
  updated_by TEXT NOT NULL,
  active INTEGER NOT NULL DEFAULT 1,
  PRIMARY KEY (kind, id)
);
CREATE TABLE IF NOT EXISTS revisions (
  id TEXT PRIMARY KEY,
  kind TEXT NOT NULL,
  doc_id TEXT NOT NULL,
  actor TEXT NOT NULL,
  created_at TEXT NOT NULL,
  before_json TEXT NOT NULL,
  after_json TEXT NOT NULL,
  status TEXT NOT NULL,
  approver TEXT
);
CREATE TABLE IF NOT EXISTS audit_events (
  id TEXT PRIMARY KEY,
  at TEXT NOT NULL,
  kind TEXT NOT NULL,
  actor TEXT NOT NULL,
  agent_id TEXT NOT NULL,
  tool TEXT NOT NULL,
  model TEXT NOT NULL,
  decision TEXT NOT NULL,
  reason TEXT NOT NULL,
  detail TEXT NOT NULL
);
CREATE OR REPLACE FUNCTION magpie_audit_immutable() RETURNS trigger AS $$
BEGIN
  RAISE EXCEPTION 'audit events are immutable';
END;
$$ LANGUAGE plpgsql;
DROP TRIGGER IF EXISTS audit_events_no_update ON audit_events;
CREATE TRIGGER audit_events_no_update BEFORE UPDATE ON audit_events
FOR EACH ROW EXECUTE FUNCTION magpie_audit_immutable();
DROP TRIGGER IF EXISTS audit_events_no_delete ON audit_events;
CREATE TRIGGER audit_events_no_delete BEFORE DELETE ON audit_events
FOR EACH ROW EXECUTE FUNCTION magpie_audit_immutable();
CREATE TABLE IF NOT EXISTS sessions (
  id TEXT PRIMARY KEY,
  subject TEXT NOT NULL,
  body TEXT NOT NULL,
  absolute_expiry TEXT NOT NULL
);
"#;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Document {
	pub kind: String,
	pub id: String,
	pub body: Value,
	pub revision: i64,
	pub updated_at: String,
	pub updated_by: String,
	pub active: bool,
}

pub struct Store {
	conn: Connection,
}

impl Store {
	pub fn open_memory() -> Result<Self> {
		let conn = Connection::open_in_memory()?;
		let store = Self { conn };
		store.migrate()?;
		Ok(store)
	}

	pub fn open_path(path: &str) -> Result<Self> {
		let conn = Connection::open(path)?;
		let store = Self { conn };
		store.migrate()?;
		Ok(store)
	}

	pub fn migrate(&self) -> Result<()> {
		self.conn.execute_batch(SQLITE_SCHEMA)?;
		Ok(())
	}

	pub fn put_document(&self, document: &Document) -> Result<()> {
		self.conn.execute(
			"INSERT INTO documents (kind, id, body, revision, updated_at, updated_by, active)
			 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)
			 ON CONFLICT(kind, id) DO UPDATE SET
			 body = excluded.body,
			 revision = excluded.revision,
			 updated_at = excluded.updated_at,
			 updated_by = excluded.updated_by,
			 active = excluded.active",
			params![
				document.kind,
				document.id,
				document.body.to_string(),
				document.revision,
				document.updated_at,
				document.updated_by,
				document.active as i64
			],
		)?;
		Ok(())
	}

	pub fn get_document(&self, kind: &str, id: &str) -> Result<Option<Document>> {
		let mut statement = self.conn.prepare(
			"SELECT kind, id, body, revision, updated_at, updated_by, active FROM documents WHERE kind = ?1 AND id = ?2",
		)?;
		let mut rows = statement.query(params![kind, id])?;
		if let Some(row) = rows.next()? {
			Ok(Some(read_document(row)?))
		} else {
			Ok(None)
		}
	}

	pub fn list_documents(&self, kind: &str) -> Result<Vec<Document>> {
		let mut statement = self.conn.prepare(
			"SELECT kind, id, body, revision, updated_at, updated_by, active FROM documents WHERE kind = ?1 ORDER BY id",
		)?;
		let mut rows = statement.query(params![kind])?;
		let mut documents = Vec::new();
		while let Some(row) = rows.next()? {
			documents.push(read_document(row)?);
		}
		Ok(documents)
	}

	pub fn append_revision(&self, revision: &Revision) -> Result<()> {
		self.conn.execute(
			"INSERT INTO revisions (id, kind, doc_id, actor, created_at, before_json, after_json, status, approver)
			 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)",
			params![
				revision.id,
				revision.kind,
				revision.doc_id,
				revision.actor,
				revision.created_at.to_rfc3339(),
				revision.before.to_string(),
				revision.after.to_string(),
				status_name(revision.status),
				revision.approver,
			],
		)?;
		Ok(())
	}

	pub fn update_revision(&self, revision: &Revision) -> Result<()> {
		self.conn.execute(
			"UPDATE revisions SET status = ?1, approver = ?2, after_json = ?3 WHERE id = ?4",
			params![
				status_name(revision.status),
				revision.approver,
				revision.after.to_string(),
				revision.id
			],
		)?;
		Ok(())
	}

	pub fn list_revisions(&self, kind: &str, doc_id: &str) -> Result<Vec<Revision>> {
		let mut statement = self.conn.prepare(
			"SELECT id, kind, doc_id, actor, created_at, before_json, after_json, status, approver
			 FROM revisions WHERE kind = ?1 AND doc_id = ?2 ORDER BY created_at",
		)?;
		let mut rows = statement.query(params![kind, doc_id])?;
		let mut revisions = Vec::new();
		while let Some(row) = rows.next()? {
			revisions.push(read_revision(row)?);
		}
		Ok(revisions)
	}

	pub fn get_revision(&self, id: &str) -> Result<Option<Revision>> {
		let mut statement = self.conn.prepare(
			"SELECT id, kind, doc_id, actor, created_at, before_json, after_json, status, approver FROM revisions WHERE id = ?1",
		)?;
		let mut rows = statement.query(params![id])?;
		if let Some(row) = rows.next()? {
			Ok(Some(read_revision(row)?))
		} else {
			Ok(None)
		}
	}

	pub fn append_audit(&self, event: &AuditEvent) -> Result<()> {
		self.conn.execute(
			"INSERT INTO audit_events (id, at, kind, actor, agent_id, tool, model, decision, reason, detail)
			 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10)",
			params![
				event.id,
				event.at.to_rfc3339(),
				event.kind,
				event.actor,
				event.agent_id,
				event.tool,
				event.model,
				event.decision,
				event.reason,
				event.detail.to_string()
			],
		)?;
		Ok(())
	}

	pub fn search_audit(&self, query: &AuditQuery) -> Result<Vec<AuditEvent>> {
		let mut statement = self.conn.prepare(
			"SELECT id, at, kind, actor, agent_id, tool, model, decision, reason, detail
			 FROM audit_events ORDER BY at DESC",
		)?;
		let mut rows = statement.query([])?;
		let mut events = Vec::new();
		let limit = if query.limit == 0 { 200 } else { query.limit };
		while let Some(row) = rows.next()? {
			let event = read_audit(row)?;
			if crate::audit::matches(&event, query) {
				events.push(event);
			}
			if events.len() >= limit {
				break;
			}
		}
		Ok(events)
	}

	pub fn prune_audit_before(&self, before: DateTime<Utc>) -> Result<usize> {
		// Retention deletes are a platform operation. The immutability trigger
		// blocks them on purpose. Operators export, then drop and recreate the
		// table in a maintenance window. This function reports how many rows
		// are older than the cutoff and does not delete them.
		let cutoff = before.to_rfc3339();
		let count: i64 = self.conn.query_row(
			"SELECT COUNT(*) FROM audit_events WHERE at < ?1",
			params![cutoff],
			|row| row.get(0),
		)?;
		Ok(count as usize)
	}

	pub fn put_session(
		&self,
		id: &str,
		subject: &str,
		body: &str,
		absolute_expiry: &str,
	) -> Result<()> {
		self.conn.execute(
			"INSERT INTO sessions (id, subject, body, absolute_expiry) VALUES (?1, ?2, ?3, ?4)
			 ON CONFLICT(id) DO UPDATE SET subject = excluded.subject, body = excluded.body, absolute_expiry = excluded.absolute_expiry",
			params![id, subject, body, absolute_expiry],
		)?;
		Ok(())
	}

	pub fn get_session(&self, id: &str) -> Result<Option<String>> {
		let mut statement = self
			.conn
			.prepare("SELECT body FROM sessions WHERE id = ?1")?;
		let mut rows = statement.query(params![id])?;
		if let Some(row) = rows.next()? {
			Ok(Some(row.get(0)?))
		} else {
			Ok(None)
		}
	}

	pub fn delete_session(&self, id: &str) -> Result<()> {
		self
			.conn
			.execute("DELETE FROM sessions WHERE id = ?1", params![id])?;
		Ok(())
	}

	pub fn delete_sessions_for_subject(&self, subject: &str) -> Result<usize> {
		let count = self
			.conn
			.execute("DELETE FROM sessions WHERE subject = ?1", params![subject])?;
		Ok(count)
	}

	pub fn export_documents(&self) -> Result<Vec<Document>> {
		let mut statement = self.conn.prepare(
			"SELECT kind, id, body, revision, updated_at, updated_by, active FROM documents ORDER BY kind, id",
		)?;
		let mut rows = statement.query([])?;
		let mut documents = Vec::new();
		while let Some(row) = rows.next()? {
			documents.push(read_document(row)?);
		}
		Ok(documents)
	}
}

fn read_document(row: &rusqlite::Row<'_>) -> Result<Document> {
	let body: String = row.get(2)?;
	Ok(Document {
		kind: row.get(0)?,
		id: row.get(1)?,
		body: serde_json::from_str(&body)?,
		revision: row.get(3)?,
		updated_at: row.get(4)?,
		updated_by: row.get(5)?,
		active: row.get::<_, i64>(6)? == 1,
	})
}

fn read_revision(row: &rusqlite::Row<'_>) -> Result<Revision> {
	let before: String = row.get(5)?;
	let after: String = row.get(6)?;
	let created: String = row.get(4)?;
	let status: String = row.get(7)?;
	Ok(Revision {
		id: row.get(0)?,
		kind: row.get(1)?,
		doc_id: row.get(2)?,
		actor: row.get(3)?,
		created_at: DateTime::parse_from_rfc3339(&created)
			.map_err(|error| Error::Msg(error.to_string()))?
			.with_timezone(&Utc),
		before: serde_json::from_str(&before)?,
		after: serde_json::from_str(&after)?,
		status: parse_status(&status)?,
		approver: row.get(8)?,
	})
}

fn read_audit(row: &rusqlite::Row<'_>) -> Result<AuditEvent> {
	let at: String = row.get(1)?;
	let detail: String = row.get(9)?;
	Ok(AuditEvent {
		id: row.get(0)?,
		at: DateTime::parse_from_rfc3339(&at)
			.map_err(|error| Error::Msg(error.to_string()))?
			.with_timezone(&Utc),
		kind: row.get(2)?,
		actor: row.get(3)?,
		agent_id: row.get(4)?,
		tool: row.get(5)?,
		model: row.get(6)?,
		decision: row.get(7)?,
		reason: row.get(8)?,
		detail: serde_json::from_str(&detail)?,
	})
}

fn status_name(status: RevisionStatus) -> &'static str {
	match status {
		RevisionStatus::Pending => "pending",
		RevisionStatus::Active => "active",
		RevisionStatus::Rejected => "rejected",
		RevisionStatus::RolledBack => "rolledBack",
	}
}

fn parse_status(value: &str) -> Result<RevisionStatus> {
	match value {
		"pending" => Ok(RevisionStatus::Pending),
		"active" => Ok(RevisionStatus::Active),
		"rejected" => Ok(RevisionStatus::Rejected),
		"rolledBack" => Ok(RevisionStatus::RolledBack),
		other => Err(Error::Msg(format!("Unknown revision status {other}."))),
	}
}

pub fn postgres_placeholder_note() -> &'static str {
	"Postgres DDL is in POSTGRES_SCHEMA. Runtime queries in this build use SQLite. Deploy Postgres by running that schema and pointing a later adapter at it, or use the sqlite file on a volume for a single node."
}

#[cfg(test)]
mod tests {
	use super::*;
	use serde_json::json;

	#[test]
	fn documents_revisions_and_immutable_audit() {
		let store = Store::open_memory().unwrap();
		let document = Document {
			kind: "policy".into(),
			id: "rule-1".into(),
			body: json!({"action": "redact"}),
			revision: 1,
			updated_at: "2026-10-09T00:00:00Z".into(),
			updated_by: "alex".into(),
			active: true,
		};
		store.put_document(&document).unwrap();
		assert_eq!(
			store
				.get_document("policy", "rule-1")
				.unwrap()
				.unwrap()
				.body["action"],
			"redact"
		);
		let revision = crate::change::propose(
			"policy",
			"rule-1",
			"alex",
			json!({"action": "log"}),
			json!({"action": "redact"}),
			false,
			Utc::now(),
		);
		store.append_revision(&revision).unwrap();
		assert_eq!(store.list_revisions("policy", "rule-1").unwrap().len(), 1);
		let event = AuditEvent::new("login", "alex", "", "", "", "allow", "signed in", json!({}));
		store.append_audit(&event).unwrap();
		let update = store.conn.execute(
			"UPDATE audit_events SET reason = 'tampered' WHERE id = ?1",
			params![event.id],
		);
		assert!(update.is_err());
		let delete = store
			.conn
			.execute("DELETE FROM audit_events WHERE id = ?1", params![event.id]);
		assert!(delete.is_err());
		assert_eq!(
			store
				.search_audit(&AuditQuery {
					kind: Some("login".into()),
					limit: 10,
					..AuditQuery::default()
				})
				.unwrap()
				.len(),
			1
		);
	}

	#[test]
	fn postgres_schema_covers_the_same_tables() {
		for table in ["documents", "revisions", "audit_events", "sessions"] {
			assert!(SQLITE_SCHEMA.contains(table));
			assert!(POSTGRES_SCHEMA.contains(table));
		}
		assert!(POSTGRES_SCHEMA.contains("audit events are immutable"));
		assert!(!postgres_placeholder_note().is_empty());
	}

	#[test]
	fn sessions_can_be_removed_for_back_channel_logout() {
		let store = Store::open_memory().unwrap();
		store
			.put_session("s1", "user-1", "{}", "2027-01-01T00:00:00Z")
			.unwrap();
		store
			.put_session("s2", "user-1", "{}", "2027-01-01T00:00:00Z")
			.unwrap();
		assert_eq!(store.delete_sessions_for_subject("user-1").unwrap(), 2);
		assert!(store.get_session("s1").unwrap().is_none());
	}
}
