//! Subscription pools live outside policy documents, exports and revision history.
//! Credentials arrive here encrypted by the portal; issued keys are stored as hashes.
use crate::{Result, accounts::token_hash, audit::AuditEvent, store::Store};
use rusqlite::{OptionalExtension, params};
use serde::{Deserialize, Serialize};

pub const SCHEMA: &str = r#"
CREATE TABLE IF NOT EXISTS subscription_pools (id TEXT PRIMARY KEY, body TEXT NOT NULL);
CREATE TABLE IF NOT EXISTS pool_subscriptions (
 id TEXT PRIMARY KEY, pool_id TEXT NOT NULL, fingerprint TEXT NOT NULL UNIQUE, body TEXT NOT NULL,
 last_used INTEGER NOT NULL DEFAULT 0, cooldown_until INTEGER NOT NULL DEFAULT 0,
 status TEXT NOT NULL DEFAULT 'ready'
);
CREATE TABLE IF NOT EXISTS pool_keys (
 id TEXT PRIMARY KEY, pool_id TEXT NOT NULL, token_hash TEXT NOT NULL UNIQUE, body TEXT NOT NULL
);
CREATE TABLE IF NOT EXISTS pool_leases (
 id TEXT PRIMARY KEY, subscription_id TEXT NOT NULL, expires_at INTEGER NOT NULL
);
CREATE INDEX IF NOT EXISTS pool_subscription_members ON pool_subscriptions(pool_id);
CREATE INDEX IF NOT EXISTS pool_lease_members ON pool_leases(subscription_id);
"#;

#[derive(Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Pool {
	pub id: String,
	pub name: String,
	pub owner: String,
	pub shared: bool,
	pub enabled: bool,
}

#[cfg(test)]
mod tests;

impl Pool {
	pub fn visible_to(&self, subject: &str) -> bool {
		self.owner == subject || self.shared
	}
}

#[derive(Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Subscription {
	pub id: String,
	pub pool_id: String,
	pub name: String,
	/// `claude` speaks Messages; `codex` speaks Responses.
	pub provider: String,
	pub models: Vec<String>,
	pub enabled: bool,
	pub max_parallel: u32,
	pub credential: String,
}

#[derive(Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PoolKey {
	pub id: String,
	pub pool_id: String,
	pub owner: String,
	pub name: String,
	pub models: Vec<String>,
	pub expires_at: i64,
	pub revoked: bool,
}

pub struct Member {
	pub subscription: Subscription,
	pub last_used: i64,
	pub cooldown_until: i64,
	pub status: String,
	pub active: u32,
}

impl Store {
	pub fn subscription_pools(&self) -> Result<Vec<Pool>> {
		let mut q = self
			.conn
			.prepare("SELECT body FROM subscription_pools ORDER BY id")?;
		q.query_map([], |r| r.get::<_, String>(0))?
			.map(|r| Ok(serde_json::from_str(&r?)?))
			.collect()
	}

	pub fn subscription_pool(&self, id: &str) -> Result<Option<Pool>> {
		let body: Option<String> = self
			.conn
			.query_row(
				"SELECT body FROM subscription_pools WHERE id=?1",
				[id],
				|r| r.get(0),
			)
			.optional()?;
		body.map(|s| Ok(serde_json::from_str(&s)?)).transpose()
	}

	pub fn save_subscription_pool(&self, pool: &Pool, actor: &str) -> Result<()> {
		let tx = self.conn.unchecked_transaction()?;
		self.conn.execute(
			"INSERT INTO subscription_pools VALUES (?1,?2) ON CONFLICT(id) DO UPDATE SET body=excluded.body",
			params![pool.id, serde_json::to_string(pool)?],
		)?;
		self.pool_audit(actor, "pool saved", &pool.id)?;
		tx.commit()?;
		Ok(())
	}

	/// Remove a pool with its subscriptions and keys; refused while requests hold leases.
	pub fn delete_subscription_pool(&self, id: &str, actor: &str) -> Result<()> {
		let tx = self.conn.unchecked_transaction()?;
		self.conn.execute(
			"DELETE FROM pool_leases WHERE expires_at<=?1",
			[chrono::Utc::now().timestamp()],
		)?;
		if self.conn.query_row(
			"SELECT COUNT(*) FROM pool_leases WHERE subscription_id IN (SELECT id FROM pool_subscriptions WHERE pool_id=?1)",
			[id],
			|r| r.get::<_, i64>(0),
		)? > 0
		{
			return Err(crate::Error::Msg(
				"Wait for active requests before deleting the pool.".into(),
			));
		}
		self
			.conn
			.execute("DELETE FROM pool_keys WHERE pool_id=?1", [id])?;
		self
			.conn
			.execute("DELETE FROM pool_subscriptions WHERE pool_id=?1", [id])?;
		self
			.conn
			.execute("DELETE FROM subscription_pools WHERE id=?1", [id])?;
		self.pool_audit(actor, "pool deleted", id)?;
		tx.commit()?;
		Ok(())
	}

	pub fn pool_members(&self, pool_id: &str, now: i64) -> Result<Vec<Member>> {
		let mut q = self.conn.prepare(
			"SELECT body,last_used,cooldown_until,status,
 (SELECT COUNT(*) FROM pool_leases WHERE subscription_id=s.id AND expires_at>?2)
 FROM pool_subscriptions s WHERE pool_id=?1 ORDER BY last_used,id",
		)?;
		let rows = q.query_map(params![pool_id, now], |r| {
			Ok((
				r.get::<_, String>(0)?,
				r.get(1)?,
				r.get(2)?,
				r.get(3)?,
				r.get(4)?,
			))
		})?;
		rows
			.map(|r| {
				let (body, last_used, cooldown_until, status, active) = r?;
				Ok(Member {
					subscription: serde_json::from_str(&body)?,
					last_used,
					cooldown_until,
					status,
					active,
				})
			})
			.collect()
	}

	pub fn save_pool_subscription(
		&self,
		subscription: &Subscription,
		fingerprint: &str,
		actor: &str,
	) -> Result<()> {
		let tx = self.conn.unchecked_transaction()?;
		// Take the write lock before checking leases so reconnect cannot race a claim.
		self.conn.execute(
			"DELETE FROM pool_leases WHERE expires_at<=?1",
			[chrono::Utc::now().timestamp()],
		)?;
		let previous: Option<String> = self
			.conn
			.query_row(
				"SELECT body FROM pool_subscriptions WHERE id=?1",
				[&subscription.id],
				|r| r.get(0),
			)
			.optional()?;
		let reconnect = previous
			.map(|s| serde_json::from_str::<Subscription>(&s))
			.transpose()?
			.is_none_or(|s| s.credential != subscription.credential);
		if reconnect
			&& self.conn.query_row(
				"SELECT COUNT(*) FROM pool_leases WHERE subscription_id=?1",
				[&subscription.id],
				|r| r.get::<_, i64>(0),
			)? > 0
		{
			return Err(crate::Error::Msg(
				"Wait for active requests before reconnecting credentials.".into(),
			));
		}
		self.conn.execute(
			"INSERT INTO pool_subscriptions(id,pool_id,fingerprint,body) VALUES (?1,?2,?3,?4)
 ON CONFLICT(id) DO UPDATE SET fingerprint=excluded.fingerprint, body=excluded.body",
			params![
				subscription.id,
				subscription.pool_id,
				fingerprint,
				serde_json::to_string(subscription)?
			],
		)?;
		if reconnect {
			self.conn.execute(
				"UPDATE pool_subscriptions SET status='ready',cooldown_until=0 WHERE id=?1",
				[&subscription.id],
			)?;
		}
		self.pool_audit(actor, "subscription saved", &subscription.id)?;
		tx.commit()?;
		Ok(())
	}

	pub fn remove_pool_subscription(&self, id: &str, pool_id: &str, actor: &str) -> Result<()> {
		let tx = self.conn.unchecked_transaction()?;
		self.conn.execute(
			"DELETE FROM pool_subscriptions WHERE id=?1 AND pool_id=?2",
			params![id, pool_id],
		)?;
		self.pool_audit(actor, "subscription removed", id)?;
		tx.commit()?;
		Ok(())
	}

	pub fn pool_keys(&self, pool_id: &str) -> Result<Vec<PoolKey>> {
		let mut q = self
			.conn
			.prepare("SELECT body FROM pool_keys WHERE pool_id=?1 ORDER BY id")?;
		q.query_map([pool_id], |r| r.get::<_, String>(0))?
			.map(|r| Ok(serde_json::from_str(&r?)?))
			.collect()
	}

	pub fn issue_pool_key(&self, key: &PoolKey, token: &str, actor: &str) -> Result<()> {
		let tx = self.conn.unchecked_transaction()?;
		self.conn.execute(
			"INSERT INTO pool_keys VALUES (?1,?2,?3,?4)",
			params![
				key.id,
				key.pool_id,
				token_hash(token),
				serde_json::to_string(key)?
			],
		)?;
		self.pool_audit(actor, "key issued", &key.id)?;
		tx.commit()?;
		Ok(())
	}

	pub fn authenticate_pool_key(&self, token: &str, now: i64) -> Result<Option<(PoolKey, Pool)>> {
		let body: Option<String> = self
			.conn
			.query_row(
				"SELECT body FROM pool_keys WHERE token_hash=?1",
				[token_hash(token)],
				|r| r.get(0),
			)
			.optional()?;
		let Some(body) = body else { return Ok(None) };
		let key: PoolKey = serde_json::from_str(&body)?;
		if key.revoked || key.expires_at <= now {
			return Ok(None);
		}
		let Some(pool) = self.subscription_pool(&key.pool_id)? else {
			return Ok(None);
		};
		if !pool.enabled || !pool.visible_to(&key.owner) {
			return Ok(None);
		}
		// Disabling an account or removing it also immediately disables its keys.
		if !self.portal_user(&key.owner)?.is_some_and(|u| !u.disabled) {
			return Ok(None);
		}
		Ok(Some((key, pool)))
	}

	pub fn revoke_pool_key(&self, mut key: PoolKey, actor: &str) -> Result<()> {
		key.revoked = true;
		let tx = self.conn.unchecked_transaction()?;
		self.conn.execute(
			"UPDATE pool_keys SET body=?2 WHERE id=?1",
			params![key.id, serde_json::to_string(&key)?],
		)?;
		self.pool_audit(actor, "key revoked", &key.id)?;
		tx.commit()?;
		Ok(())
	}

	/// Claim and recheck under a SQLite write transaction, including across processes.
	pub fn claim_pool_subscription(
		&self,
		pool_id: &str,
		provider: &str,
		model: &str,
		excluded: &[String],
		now: i64,
	) -> Result<Option<(Subscription, String)>> {
		let tx = self.conn.unchecked_transaction()?;
		self
			.conn
			.execute("DELETE FROM pool_leases WHERE expires_at<=?1", [now])?;
		let mut members = self.pool_members(pool_id, now)?;
		members.sort_by_key(|m| (m.active, m.last_used));
		let member = members.into_iter().find(|m| {
			let s = &m.subscription;
			s.enabled
				&& s.provider == provider
				&& s.models.iter().any(|v| v == model)
				&& !excluded.contains(&s.id)
				&& m.status != "reauthenticate"
				&& m.cooldown_until <= now
				&& m.active < s.max_parallel
		});
		let result = if let Some(m) = member {
			let lease = crate::new_id();
			self.conn.execute(
				"INSERT INTO pool_leases VALUES (?1,?2,?3)",
				params![lease, m.subscription.id, now + 660],
			)?;
			self.conn.execute("UPDATE pool_subscriptions SET last_used=MAX(?2,COALESCE((SELECT MAX(last_used) FROM pool_subscriptions),0)+1) WHERE id=?1", params![m.subscription.id,now])?;
			Some((m.subscription, lease))
		} else {
			None
		};
		tx.commit()?;
		Ok(result)
	}

	pub fn release_pool_lease(&self, id: &str) -> Result<()> {
		self
			.conn
			.execute("DELETE FROM pool_leases WHERE id=?1", [id])?;
		Ok(())
	}

	pub fn pool_subscription_status(&self, id: &str, status: &str, cooldown: i64) -> Result<()> {
		self.conn.execute(
			"UPDATE pool_subscriptions SET status=CASE
 WHEN status='reauthenticate' OR ?2='reauthenticate' THEN 'reauthenticate'
 WHEN MAX(cooldown_until,?3)>?4 THEN 'limited' ELSE ?2 END,
 cooldown_until=MAX(cooldown_until,?3) WHERE id=?1",
			params![id, status, cooldown, chrono::Utc::now().timestamp()],
		)?;
		Ok(())
	}

	/// Refresh only the credential belonging to this lease; a concurrent reconnect wins.
	pub fn refresh_pool_credential(&self, subscription: &Subscription, old: &str) -> Result<bool> {
		let tx = self.conn.unchecked_transaction()?;
		self.conn.execute(
			"DELETE FROM pool_leases WHERE expires_at<=?1",
			[chrono::Utc::now().timestamp()],
		)?;
		let body: Option<String> = self
			.conn
			.query_row(
				"SELECT body FROM pool_subscriptions WHERE id=?1",
				[&subscription.id],
				|r| r.get(0),
			)
			.optional()?;
		let Some(body) = body else { return Ok(false) };
		let mut current: Subscription = serde_json::from_str(&body)?;
		if current.credential != old {
			return Ok(false);
		}
		current.credential = subscription.credential.clone();
		let changed = self.conn.execute(
			"UPDATE pool_subscriptions SET body=?2 WHERE id=?1 AND body=?3",
			params![current.id, serde_json::to_string(&current)?, body],
		)? == 1;
		tx.commit()?;
		Ok(changed)
	}

	fn pool_audit(&self, actor: &str, reason: &str, id: &str) -> Result<()> {
		self.append_audit(&AuditEvent::new(
			"subscription-pool",
			actor,
			"",
			"",
			"",
			"change",
			reason,
			serde_json::json!({"id":id}),
		))
	}
}
