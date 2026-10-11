//! Local credentials and single-use account links, isolated from policy exports.
use crate::{Error, Result, audit::AuditEvent, rbac::PortalUser, store::Store};
use chrono::Utc;
use rusqlite::{OptionalExtension, params};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

#[derive(Clone, Serialize, Deserialize)]
pub struct LocalAccount {
	pub subject: String,
	pub login: String,
	pub name: String,
	pub email: String,
	pub password_hash: String,
	pub recovery: bool,
}
#[derive(Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AccountLink {
	pub id: String,
	pub kind: String,
	pub subject: String,
	pub email: String,
	pub name: String,
	pub groups: Vec<String>,
	pub expires_at: String,
}
pub fn token_hash(token: &str) -> String {
	format!("{:x}", Sha256::digest(token.as_bytes()))
}
pub fn password_problem(password: &str) -> Option<&'static str> {
	if password.len() > 72 {
		return Some("Use at most 72 UTF-8 bytes for your password.");
	}
	if password.chars().count() < 16 {
		Some("Use at least 16 characters for your password.")
	} else {
		crate::identity::admin_password_problem(password)
	}
}
pub fn verify(password: &str, hash: &str) -> bool {
	password.len() <= 72 && bcrypt::verify(password, hash).unwrap_or(false)
}
impl Store {
	pub fn migrate_accounts(&self) -> Result<()> {
		self.conn.execute_batch("CREATE TABLE IF NOT EXISTS local_accounts (subject TEXT PRIMARY KEY, login TEXT UNIQUE NOT NULL, body TEXT NOT NULL);
  CREATE TABLE IF NOT EXISTS account_links (token_hash TEXT PRIMARY KEY, id TEXT UNIQUE NOT NULL, subject TEXT NOT NULL, email TEXT NOT NULL, body TEXT NOT NULL);")?;
		Ok(())
	}
	pub fn local_account(&self, subject: &str) -> Result<Option<LocalAccount>> {
		let body: Option<String> = self
			.conn
			.query_row(
				"SELECT body FROM local_accounts WHERE subject=?1",
				params![subject],
				|r| r.get(0),
			)
			.optional()?;
		body.map(|s| Ok(serde_json::from_str(&s)?)).transpose()
	}
	pub fn local_login(&self, login: &str) -> Result<Option<LocalAccount>> {
		let body: Option<String> = self
			.conn
			.query_row(
				"SELECT body FROM local_accounts WHERE login=?1",
				params![login.trim().to_lowercase()],
				|r| r.get(0),
			)
			.optional()?;
		body.map(|s| Ok(serde_json::from_str(&s)?)).transpose()
	}
	pub fn bootstrap_local_account(&self, account: &LocalAccount) -> Result<()> {
		self.conn.execute(
			"INSERT OR IGNORE INTO local_accounts (subject,login,body) VALUES (?1,?2,?3)",
			params![
				account.subject,
				account.login.to_lowercase(),
				serde_json::to_string(account)?
			],
		)?;
		Ok(())
	}
	pub fn account_links(&self) -> Result<Vec<AccountLink>> {
		let mut stmt = self.conn.prepare("SELECT body FROM account_links")?;
		let rows = stmt.query_map([], |r| r.get::<_, String>(0))?;
		rows.map(|r| Ok(serde_json::from_str(&r?)?)).collect()
	}
	pub fn issue_account_link(&self, link: &AccountLink, token: &str, actor: &str) -> Result<()> {
		let tx = self.conn.unchecked_transaction()?;
		self.conn.execute(
			"DELETE FROM account_links WHERE email=?1 OR subject=?2",
			params![link.email, link.subject],
		)?;
		self.conn.execute(
			"INSERT INTO account_links (token_hash,id,subject,email,body) VALUES (?1,?2,?3,?4,?5)",
			params![
				token_hash(token),
				link.id,
				link.subject,
				link.email,
				serde_json::to_string(link)?
			],
		)?;
		self.append_audit(&AuditEvent::new(
			"account",
			actor,
			"",
			"",
			"",
			"allow",
			"Account link created.",
			serde_json::json!({"kind":link.kind,"subject":link.subject,"id":link.id}),
		))?;
		tx.commit()?;
		Ok(())
	}
	pub fn revoke_account_link(&self, id: &str, actor: &str) -> Result<()> {
		self
			.conn
			.execute("DELETE FROM account_links WHERE id=?1", params![id])?;
		self.append_audit(&AuditEvent::new(
			"account",
			actor,
			"",
			"",
			"",
			"allow",
			"Account link revoked.",
			serde_json::json!({"id":id}),
		))?;
		Ok(())
	}
	pub fn change_local_password(&self, subject: &str, expected: &str, hash: &str) -> Result<()> {
		let tx = self.conn.unchecked_transaction()?;
		let mut account = self.local_account(subject)?.ok_or(Error::Unauthorised)?;
		if account.password_hash != expected {
			return Err(Error::Unauthorised);
		}
		account.password_hash = hash.into();
		self.conn.execute(
			"UPDATE local_accounts SET body=?2 WHERE subject=?1",
			params![subject, serde_json::to_string(&account)?],
		)?;
		self.delete_sessions_for_subject(subject)?;
		self.conn.execute(
			"DELETE FROM account_links WHERE subject=?1",
			params![subject],
		)?;
		self.append_audit(&AuditEvent::new(
			"account",
			subject,
			"",
			"",
			"",
			"allow",
			"Password changed; sessions and recovery links revoked.",
			serde_json::json!({}),
		))?;
		tx.commit()?;
		Ok(())
	}
	pub fn redeem_account_link(&self, token: &str, password_hash: &str) -> Result<()> {
		let tx = self.conn.unchecked_transaction()?;
		let body: Option<String> = self
			.conn
			.query_row(
				"SELECT body FROM account_links WHERE token_hash=?1",
				params![token_hash(token)],
				|r| r.get(0),
			)
			.optional()?;
		let link: AccountLink = serde_json::from_str(&body.ok_or(Error::Unauthorised)?)?;
		if chrono::DateTime::parse_from_rfc3339(&link.expires_at)
			.map_or(true, |expiry| expiry <= Utc::now())
		{
			return Err(Error::Unauthorised);
		}
		if self.portal_user(&link.subject)?.is_some_and(|u| u.disabled) {
			return Err(Error::Unauthorised);
		}
		let mut account = if link.kind == "invite" {
			if self.local_login(&link.email)?.is_some() {
				return Err(Error::Unauthorised);
			}
			let mut access = self.access_control()?.ok_or(Error::Forbidden)?;
			for group_id in &link.groups {
				let group = access
					.groups
					.iter_mut()
					.find(|g| &g.id == group_id)
					.ok_or(Error::Forbidden)?;
				if !group.members.contains(&link.subject) {
					group.members.push(link.subject.clone());
				}
			}
			access.validate()?;
			self.conn.execute(
				"UPDATE runtime_settings SET value=?1 WHERE name='access_control'",
				params![serde_json::to_string(&access)?],
			)?;
			self.remember_portal_user(&PortalUser {
				subject: link.subject.clone(),
				name: link.name.clone(),
				email: link.email.clone(),
				last_login: String::new(),
				disabled: false,
			})?;
			LocalAccount {
				subject: link.subject.clone(),
				login: link.email.clone(),
				name: link.name.clone(),
				email: link.email.clone(),
				password_hash: String::new(),
				recovery: false,
			}
		} else {
			self
				.local_account(&link.subject)?
				.ok_or(Error::Unauthorised)?
		};
		account.password_hash = password_hash.into();
		self.conn.execute("INSERT INTO local_accounts (subject,login,body) VALUES (?1,?2,?3) ON CONFLICT(subject) DO UPDATE SET body=excluded.body",params![account.subject,account.login,serde_json::to_string(&account)?])?;
		self.delete_sessions_for_subject(&link.subject)?;
		self.conn.execute(
			"DELETE FROM account_links WHERE subject=?1",
			params![link.subject],
		)?;
		self.append_audit(&AuditEvent::new(
			"account",
			&link.subject,
			"",
			"",
			"",
			"allow",
			"Account link redeemed; existing sessions revoked.",
			serde_json::json!({"kind":link.kind,"groups":link.groups}),
		))?;
		tx.commit()?;
		Ok(())
	}
}

#[cfg(test)]
mod tests {
	use super::*;
	fn setup() -> Store {
		let s = Store::open_memory().unwrap();
		s.set_access_control(
			&crate::rbac::AccessControl::defaults(&Default::default()),
			"test",
		)
		.unwrap();
		s
	}
	fn link() -> AccountLink {
		AccountLink {
			id: "link-1".into(),
			kind: "invite".into(),
			subject: "local:test".into(),
			email: "test@example.com.au".into(),
			name: "Tester".into(),
			groups: vec![],
			expires_at: (Utc::now() + chrono::Duration::hours(1)).to_rfc3339(),
		}
	}
	#[test]
	fn expired_revoked_and_replaced_links_cannot_create_accounts() {
		let s = setup();
		let mut l = link();
		l.expires_at = (Utc::now() - chrono::Duration::hours(1)).to_rfc3339();
		s.issue_account_link(&l, "expired", "owner").unwrap();
		assert!(s.redeem_account_link("expired", "hash").is_err());
		l = link();
		s.issue_account_link(&l, "first", "owner").unwrap();
		s.issue_account_link(&l, "second", "owner").unwrap();
		assert!(s.redeem_account_link("first", "hash").is_err());
		s.revoke_account_link(&l.id, "owner").unwrap();
		assert!(s.redeem_account_link("second", "hash").is_err());
		assert!(s.local_login(&l.email).unwrap().is_none());
	}
	#[test]
	fn bootstrap_never_overwrites_changed_password_and_reset_revokes_sessions() {
		let s = setup();
		let a = LocalAccount {
			subject: "admin".into(),
			login: "admin".into(),
			name: "Admin".into(),
			email: "admin@local.invalid".into(),
			password_hash: "original".into(),
			recovery: true,
		};
		s.bootstrap_local_account(&a).unwrap();
		s.put_session("s", "admin", "{}", "2099").unwrap();
		s.change_local_password("admin", "original", "updated")
			.unwrap();
		s.bootstrap_local_account(&a).unwrap();
		assert_eq!(
			s.local_login("admin").unwrap().unwrap().password_hash,
			"updated"
		);
		assert!(s.get_session("s").unwrap().is_none());
		assert!(
			s.change_local_password("admin", "original", "stale")
				.is_err()
		);
	}
	#[test]
	fn token_grant_is_atomic_and_single_use() {
		let s = setup();
		let mut l = link();
		l.groups = vec!["deleted-group".into()];
		s.issue_account_link(&l, "token", "owner").unwrap();
		assert!(s.redeem_account_link("token", "hash").is_err());
		assert!(s.local_login(&l.email).unwrap().is_none());
		l.groups.clear();
		s.issue_account_link(&l, "token", "owner").unwrap();
		s.redeem_account_link("token", "hash").unwrap();
		assert!(s.redeem_account_link("token", "other").is_err());
		assert!(!s.local_login(&l.email).unwrap().unwrap().recovery);
	}
}
