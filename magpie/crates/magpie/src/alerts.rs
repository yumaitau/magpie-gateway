//! Durable alert destinations and delivery queue, separate from policy exports.
use crate::{Result, audit::AuditEvent, store::Store};
use rusqlite::params;
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};

pub const SCHEMA: &str = r#"
CREATE TABLE IF NOT EXISTS alert_destinations (id TEXT PRIMARY KEY, body TEXT NOT NULL);
CREATE TABLE IF NOT EXISTS alert_cursor (id INTEGER PRIMARY KEY CHECK(id=1), position INTEGER NOT NULL);
INSERT OR IGNORE INTO alert_cursor VALUES (1, (SELECT COALESCE(MAX(rowid),0) FROM audit_events));
CREATE TABLE IF NOT EXISTS alert_deliveries (
 id TEXT PRIMARY KEY, destination TEXT NOT NULL, event_id TEXT NOT NULL, body TEXT NOT NULL,
 status TEXT NOT NULL DEFAULT 'pending', attempts INTEGER NOT NULL DEFAULT 0,
 next_attempt INTEGER NOT NULL DEFAULT 0, result TEXT NOT NULL DEFAULT '',
 created_at TEXT NOT NULL, UNIQUE(destination,event_id)
);
CREATE INDEX IF NOT EXISTS alert_pending ON alert_deliveries(status,next_attempt);
"#;

#[derive(Clone, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct Destination {
	pub id: String,
	pub after_event: i64,
	pub name: String,
	pub kind: String,
	pub trigger: String,
	pub enabled: bool,
	pub url: String,
	pub bearer: String,
	pub recipient: String,
	pub smtp_host: String,
	pub smtp_port: u16,
	pub smtp_username: String,
	pub smtp_password: String,
	pub smtp_from: String,
}
impl Destination {
	pub fn matches(&self, event: &AuditEvent) -> bool {
		if !self.enabled || event.kind == "alerts" {
			return false;
		}
		let decision = event
			.detail
			.get("wouldAction")
			.and_then(Value::as_str)
			.unwrap_or(&event.decision);
		match self.trigger.as_str() {
			"security" => {
				event
					.detail
					.get("hits")
					.and_then(Value::as_array)
					.is_some_and(|hits| !hits.is_empty())
					|| matches!(
						decision,
						"block"
							| "deny"
							| "redact"
							| "tokenise"
							| "tokenize"
							| "mask"
							| "reject"
							| "error"
							| "require approval"
					)
			},
			"blocked" => matches!(decision, "block" | "deny" | "reject" | "require approval"),
			"changes" => matches!(
				event.kind.as_str(),
				"settings" | "policy" | "access" | "config" | "gateway-config" | "account"
			),
			_ => false,
		}
	}
}
#[derive(Clone)]
pub struct Delivery {
	pub id: String,
	pub destination: String,
	pub body: Value,
	pub status: String,
	pub attempts: i64,
	pub result: String,
	pub created_at: String,
}

/// Deliberately exclude prompt text, actor identity and untrusted free-form reasons.
pub fn payload(event: &AuditEvent) -> Value {
	json!({"id":event.id,"at":event.at,"kind":event.kind,"decision":event.decision,
 "wouldAction":event.detail.get("wouldAction").filter(|v| v.is_string())})
}
impl Store {
	pub fn alert_destinations(&self) -> Result<Vec<Destination>> {
		let mut q = self
			.conn
			.prepare("SELECT body FROM alert_destinations ORDER BY id")?;
		let rows = q.query_map([], |r| r.get::<_, String>(0))?;
		rows.map(|r| Ok(serde_json::from_str(&r?)?)).collect()
	}
	pub fn save_alert_destination(&self, d: &Destination, actor: &str) -> Result<()> {
		let tx = self.conn.unchecked_transaction()?;
		let mut d = d.clone();
		d.after_event = match self.alert_destinations()?.iter().find(|old| old.id == d.id) {
			Some(old) if old.enabled && d.enabled => old.after_event,
			_ => self
				.conn
				.query_row("SELECT COALESCE(MAX(rowid),0) FROM audit_events", [], |r| {
					r.get(0)
				})?,
		};
		self.conn.execute(
			"INSERT INTO alert_destinations VALUES (?1,?2) ON CONFLICT(id) DO UPDATE SET body=excluded.body",
			params![d.id, serde_json::to_string(&d)?],
		)?;
		if !d.enabled {
			self.conn.execute("UPDATE alert_deliveries SET status='cancelled',result='Destination disabled' WHERE destination=?1 AND status='pending'",[&d.id])?;
		}
		self.append_audit(&AuditEvent::new(
			"alerts",
			actor,
			"",
			"",
			"",
			"allow",
			"Alert destination saved",
			json!({"id":d.id,"kind":d.kind,"enabled":d.enabled}),
		))?;
		tx.commit()?;
		Ok(())
	}
	pub fn delete_alert_destination(&self, id: &str, actor: &str) -> Result<()> {
		let tx = self.conn.unchecked_transaction()?;
		self
			.conn
			.execute("DELETE FROM alert_destinations WHERE id=?1", [id])?;
		self.conn.execute("UPDATE alert_deliveries SET status='cancelled',result='Destination removed' WHERE destination=?1 AND status='pending'",[id])?;
		self.append_audit(&AuditEvent::new(
			"alerts",
			actor,
			"",
			"",
			"",
			"allow",
			"Alert destination removed",
			json!({"id":id}),
		))?;
		tx.commit()?;
		Ok(())
	}
	pub fn enqueue_alert(&self, destination: &str, event: &AuditEvent) -> Result<()> {
		self.conn.execute("INSERT OR IGNORE INTO alert_deliveries(id,destination,event_id,body,created_at) VALUES (?1,?2,?3,?4,?5)",params![crate::new_id(),destination,event.id,payload(event).to_string(),event.at.to_rfc3339()])?;
		Ok(())
	}
	pub fn collect_alerts(&self) -> Result<()> {
		let tx = self.conn.unchecked_transaction()?;
		let destinations = self.alert_destinations()?;
		let mut q=self.conn.prepare("SELECT id,at,kind,actor,agent_id,tool,model,decision,reason,detail,rowid FROM audit_events WHERE rowid>(SELECT position FROM alert_cursor WHERE id=1) ORDER BY rowid LIMIT 200")?;
		let mut rows = q.query([])?;
		while let Some(row) = rows.next()? {
			let event = crate::store::read_audit(row)?;
			for d in &destinations {
				if row.get::<_, i64>(10)? > d.after_event && d.matches(&event) {
					self.enqueue_alert(&d.id, &event)?;
				}
			}
			self.conn.execute(
				"UPDATE alert_cursor SET position=?1 WHERE id=1",
				[row.get::<_, i64>(10)?],
			)?;
		}
		drop(rows);
		drop(q);
		tx.commit()?;
		Ok(())
	}
	pub fn alert_deliveries(&self, pending: bool) -> Result<Vec<Delivery>> {
		let sql = if pending {
			"SELECT id,destination,body,status,attempts,result,created_at FROM alert_deliveries WHERE status='pending' AND next_attempt<=unixepoch() ORDER BY created_at LIMIT 20"
		} else {
			"SELECT id,destination,body,status,attempts,result,created_at FROM alert_deliveries ORDER BY created_at DESC LIMIT 50"
		};
		let mut q = self.conn.prepare(sql)?;
		let mut rows = q.query([])?;
		let mut result = vec![];
		while let Some(r) = rows.next()? {
			let body: String = r.get(2)?;
			result.push(Delivery {
				id: r.get(0)?,
				destination: r.get(1)?,
				body: serde_json::from_str(&body)?,
				status: r.get(3)?,
				attempts: r.get(4)?,
				result: r.get(5)?,
				created_at: r.get(6)?,
			});
		}
		Ok(result)
	}
	pub fn alert_is_pending(&self, id: &str) -> Result<bool> {
		Ok(self.conn.query_row(
			"SELECT EXISTS(SELECT 1 FROM alert_deliveries WHERE id=?1 AND status='pending')",
			[id],
			|row| row.get(0),
		)?)
	}
	pub fn finish_alert(&self, id: &str, attempt: i64, error: Option<&str>) -> Result<()> {
		let status = if error.is_none() {
			"sent"
		} else if attempt >= 5 {
			"failed"
		} else {
			"pending"
		};
		self.conn.execute("UPDATE alert_deliveries SET status=?2,attempts=?3,result=?4,next_attempt=unixepoch()+?5 WHERE id=?1 AND status='pending'",params![id,status,attempt,error.unwrap_or("Delivered"),30_i64*2_i64.pow(attempt.min(5) as u32)])?;
		Ok(())
	}
}
#[cfg(test)]
mod tests {
	use super::*;
	#[test]
	fn triggers_cover_native_dlp_actions_and_log_only_findings() {
		let d = Destination {
			trigger: "security".into(),
			enabled: true,
			..Default::default()
		};
		for action in [
			crate::dlp::Action::Block,
			crate::dlp::Action::RequireApproval,
			crate::dlp::Action::Redact,
			crate::dlp::Action::Mask,
			crate::dlp::Action::Tokenise,
		] {
			let e = AuditEvent::new(
				"decision",
				"",
				"",
				"",
				"",
				"log",
				"",
				json!({"wouldAction":action.as_str()}),
			);
			assert!(d.matches(&e), "{}", action.as_str());
		}
		assert!(d.matches(&AuditEvent::new(
			"decision",
			"",
			"",
			"",
			"",
			"log",
			"",
			json!({"hits":[{}],"wouldAction":"log"})
		)));
	}

	#[test]
	fn new_destinations_skip_backlog_and_queue_survives_restart() {
		let path = std::env::temp_dir().join(format!("magpie-alert-{}.sqlite", crate::new_id()));
		let d = Destination {
			id: "new".into(),
			trigger: "security".into(),
			enabled: true,
			..Default::default()
		};
		{
			let s = Store::open_path(path.to_str().unwrap()).unwrap();
			for _ in 0..250 {
				s.append_audit(&AuditEvent::new(
					"guardrail",
					"",
					"",
					"",
					"",
					"block",
					"",
					json!({}),
				))
				.unwrap();
			}
			s.save_alert_destination(&d, "admin").unwrap();
			s.append_audit(&AuditEvent::new(
				"guardrail",
				"",
				"",
				"",
				"",
				"block",
				"",
				json!({}),
			))
			.unwrap();
			s.collect_alerts().unwrap();
			s.collect_alerts().unwrap();
			assert_eq!(s.alert_deliveries(true).unwrap().len(), 1);
		}
		{
			let s = Store::open_path(path.to_str().unwrap()).unwrap();
			s.collect_alerts().unwrap();
			assert_eq!(s.alert_deliveries(true).unwrap().len(), 1);
			let mut disabled = s.alert_destinations().unwrap().remove(0);
			disabled.enabled = false;
			s.save_alert_destination(&disabled, "admin").unwrap();
			s.append_audit(&AuditEvent::new(
				"guardrail",
				"",
				"",
				"",
				"",
				"block",
				"",
				json!({}),
			))
			.unwrap();
			disabled.enabled = true;
			s.save_alert_destination(&disabled, "admin").unwrap();
			s.collect_alerts().unwrap();
			assert!(s.alert_deliveries(true).unwrap().is_empty());
		}
		std::fs::remove_file(path).unwrap();
	}

	#[test]
	fn durable_queue_filters_redacts_and_retries() {
		let s = Store::open_memory().unwrap();
		let d = Destination {
			id: "one".into(),
			kind: "webhook".into(),
			trigger: "security".into(),
			enabled: true,
			..Default::default()
		};
		s.save_alert_destination(&d, "admin").unwrap();
		let e = AuditEvent::new(
			"guardrail",
			"secret@example.com",
			"",
			"",
			"",
			"allow",
			"private prompt",
			json!({"wouldAction":"block","prompt":"private"}),
		);
		s.append_audit(&e).unwrap();
		s.collect_alerts().unwrap();
		s.collect_alerts().unwrap();
		let jobs = s.alert_deliveries(true).unwrap();
		assert_eq!(jobs.len(), 1);
		assert!(!jobs[0].body.to_string().contains("private"));
		assert!(!jobs[0].body.to_string().contains("secret"));
		s.finish_alert(&jobs[0].id, 1, Some("HTTP 500")).unwrap();
		assert!(s.alert_deliveries(true).unwrap().is_empty());
		s.finish_alert(&jobs[0].id, 5, Some("HTTP 500")).unwrap();
		assert_eq!(s.alert_deliveries(false).unwrap()[0].status, "failed");
		assert!(s.export_documents().unwrap().is_empty());
	}
	#[test]
	fn disabling_cancels_pending_and_allow_does_not_alert() {
		let s = Store::open_memory().unwrap();
		let mut d = Destination {
			id: "one".into(),
			trigger: "blocked".into(),
			enabled: true,
			..Default::default()
		};
		s.save_alert_destination(&d, "admin").unwrap();
		let mut e = AuditEvent::new("guardrail", "a", "", "", "", "allow", "", json!({}));
		assert!(!d.matches(&e));
		e.decision = "block".into();
		s.append_audit(&e).unwrap();
		s.collect_alerts().unwrap();
		d.enabled = false;
		s.save_alert_destination(&d, "admin").unwrap();
		assert!(s.alert_deliveries(true).unwrap().is_empty());
		assert_eq!(s.alert_deliveries(false).unwrap()[0].status, "cancelled");
	}
}
