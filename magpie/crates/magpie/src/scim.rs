//! A practical subset of SCIM 2.0 for users and groups.
//!
//! Covered: create, read, replace, delete, `userName eq` filters, startIndex
//! and count, and patch operations Entra and Okta use for `active`,
//! `displayName` and group `members`. This is not a full RFC 7644 implementation.

use serde_json::{Value, json};

use crate::{Error, Result};

#[derive(Debug, Clone, Default)]
pub struct Directory {
	pub users: Vec<Value>,
	pub groups: Vec<Value>,
}

impl Directory {
	pub fn create_user(&mut self, mut body: Value) -> Result<Value> {
		let user_name = body
			.get("userName")
			.and_then(|value| value.as_str())
			.ok_or_else(|| Error::Msg("SCIM user requires userName.".into()))?
			.to_string();
		if self
			.users
			.iter()
			.any(|user| user.get("userName").and_then(|v| v.as_str()) == Some(user_name.as_str()))
		{
			return Err(Error::Msg("That userName already exists.".into()));
		}
		let id = crate::new_id();
		body["id"] = json!(id);
		body["schemas"] = json!(["urn:ietf:params:scim:schemas:core:2.0:User"]);
		if body.get("active").is_none() {
			body["active"] = json!(true);
		}
		self.users.push(body.clone());
		Ok(body)
	}

	pub fn get_user(&self, id: &str) -> Option<&Value> {
		self
			.users
			.iter()
			.find(|user| user.get("id").and_then(|v| v.as_str()) == Some(id))
	}

	pub fn replace_user(&mut self, id: &str, mut body: Value) -> Result<Value> {
		let position = self
			.users
			.iter()
			.position(|user| user.get("id").and_then(|v| v.as_str()) == Some(id))
			.ok_or_else(|| Error::Msg("User not found.".into()))?;
		body["id"] = json!(id);
		self.users[position] = body.clone();
		Ok(body)
	}

	pub fn delete_user(&mut self, id: &str) -> Result<()> {
		let before = self.users.len();
		self
			.users
			.retain(|user| user.get("id").and_then(|v| v.as_str()) != Some(id));
		if self.users.len() == before {
			return Err(Error::Msg("User not found.".into()));
		}
		Ok(())
	}

	pub fn list_users(&self, filter: Option<&str>, start_index: usize, count: usize) -> Value {
		let matched: Vec<&Value> = self
			.users
			.iter()
			.filter(|user| filter_matches(user, filter))
			.collect();
		page("User", &matched, start_index, count)
	}

	pub fn patch_user(&mut self, id: &str, patch: &Value) -> Result<Value> {
		let user = self
			.users
			.iter_mut()
			.find(|user| user.get("id").and_then(|v| v.as_str()) == Some(id))
			.ok_or_else(|| Error::Msg("User not found.".into()))?;
		apply_patch(user, patch)?;
		Ok(user.clone())
	}

	pub fn create_group(&mut self, mut body: Value) -> Result<Value> {
		let display = body
			.get("displayName")
			.and_then(|value| value.as_str())
			.ok_or_else(|| Error::Msg("SCIM group requires displayName.".into()))?;
		if display.is_empty() {
			return Err(Error::Msg("SCIM group requires displayName.".into()));
		}
		let id = crate::new_id();
		body["id"] = json!(id);
		body["schemas"] = json!(["urn:ietf:params:scim:schemas:core:2.0:Group"]);
		if body.get("members").is_none() {
			body["members"] = json!([]);
		}
		self.groups.push(body.clone());
		Ok(body)
	}

	pub fn list_groups(&self, start_index: usize, count: usize) -> Value {
		let refs: Vec<&Value> = self.groups.iter().collect();
		page("Group", &refs, start_index, count)
	}

	pub fn get_group(&self, id: &str) -> Option<&Value> {
		self
			.groups
			.iter()
			.find(|group| group.get("id").and_then(|v| v.as_str()) == Some(id))
	}

	pub fn patch_group(&mut self, id: &str, patch: &Value) -> Result<Value> {
		let group = self
			.groups
			.iter_mut()
			.find(|group| group.get("id").and_then(|v| v.as_str()) == Some(id))
			.ok_or_else(|| Error::Msg("Group not found.".into()))?;
		apply_patch(group, patch)?;
		Ok(group.clone())
	}

	pub fn delete_group(&mut self, id: &str) -> Result<()> {
		let before = self.groups.len();
		self
			.groups
			.retain(|group| group.get("id").and_then(|v| v.as_str()) != Some(id));
		if self.groups.len() == before {
			return Err(Error::Msg("Group not found.".into()));
		}
		Ok(())
	}
}

fn filter_matches(user: &Value, filter: Option<&str>) -> bool {
	let Some(filter) = filter else {
		return true;
	};
	let filter = filter.trim();
	let Some(rest) = filter.strip_prefix("userName eq ") else {
		return false;
	};
	let wanted = rest.trim().trim_matches('"');
	user.get("userName").and_then(|value| value.as_str()) == Some(wanted)
}

fn page(resource: &str, items: &[&Value], start_index: usize, count: usize) -> Value {
	let start = start_index.max(1);
	let slice_from = start - 1;
	let taken: Vec<&Value> = items
		.iter()
		.copied()
		.skip(slice_from)
		.take(count.max(1))
		.collect();
	json!({
		"schemas": ["urn:ietf:params:scim:api:messages:2.0:ListResponse"],
		"totalResults": items.len(),
		"startIndex": start,
		"itemsPerPage": taken.len(),
		"Resources": taken,
		"resourceType": resource
	})
}

fn apply_patch(target: &mut Value, patch: &Value) -> Result<()> {
	let operations = patch
		.get("Operations")
		.and_then(|value| value.as_array())
		.ok_or_else(|| Error::Msg("SCIM patch requires Operations.".into()))?;
	for operation in operations {
		let op = operation
			.get("op")
			.and_then(|value| value.as_str())
			.unwrap_or("")
			.to_ascii_lowercase();
		let path = operation
			.get("path")
			.and_then(|value| value.as_str())
			.unwrap_or("");
		let value = operation.get("value").cloned().unwrap_or(Value::Null);
		match (op.as_str(), path) {
			("replace" | "add", "active") => target["active"] = value,
			("replace" | "add", "displayName") => target["displayName"] = value,
			("replace" | "add", "userName") => target["userName"] = value,
			("replace", "members") => target["members"] = value,
			("add", "members") => {
				let members = target
					.as_object_mut()
					.unwrap()
					.entry("members")
					.or_insert_with(|| json!([]));
				if let Some(list) = members.as_array_mut() {
					if let Some(incoming) = value.as_array() {
						list.extend(incoming.clone());
					} else if !value.is_null() {
						list.push(value);
					}
				}
			},
			("remove", "members") => target["members"] = json!([]),
			_ => {
				return Err(Error::Msg(format!(
					"Unsupported SCIM patch {op} on {path}."
				)));
			},
		}
	}
	Ok(())
}

pub fn service_provider_config() -> Value {
	json!({
		"schemas": ["urn:ietf:params:scim:schemas:core:2.0:ServiceProviderConfig"],
		"patch": { "supported": true },
		"bulk": { "supported": false, "maxOperations": 0, "maxPayloadSize": 0 },
		"filter": { "supported": true, "maxResults": 200 },
		"changePassword": { "supported": false },
		"sort": { "supported": false },
		"etag": { "supported": false },
		"authenticationSchemes": [{
			"type": "oauthbearertoken",
			"name": "Bearer",
			"description": "Bearer token configured on the portal."
		}]
	})
}

#[cfg(test)]
mod tests {
	use super::*;

	#[test]
	fn user_and_group_lifecycle_for_entra_style_patch() {
		let mut directory = Directory::default();
		let user = directory
			.create_user(json!({"userName": "alex@example.gov.au", "displayName": "Alex"}))
			.unwrap();
		let id = user["id"].as_str().unwrap().to_string();
		let listed = directory.list_users(Some("userName eq \"alex@example.gov.au\""), 1, 10);
		assert_eq!(listed["totalResults"], 1);
		directory
			.patch_user(
				&id,
				&json!({"Operations": [{"op": "replace", "path": "active", "value": false}]}),
			)
			.unwrap();
		assert_eq!(directory.get_user(&id).unwrap()["active"], false);
		let group = directory
			.create_group(json!({"displayName": "magpie-auditors"}))
			.unwrap();
		let group_id = group["id"].as_str().unwrap();
		directory
			.patch_group(
				group_id,
				&json!({"Operations": [{"op": "add", "path": "members", "value": [{"value": id}]}]}),
			)
			.unwrap();
		assert_eq!(
			directory.get_group(group_id).unwrap()["members"][0]["value"],
			id
		);
		directory.delete_user(&id).unwrap();
		directory.delete_group(group_id).unwrap();
	}
}
