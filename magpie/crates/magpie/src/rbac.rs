//! Persistent portal roles and groups. Grants are evaluated on every action.

use crate::identity::{Permission, Role, RoleMap, SessionRecord};
use crate::{Error, Result};
use serde::{Deserialize, Serialize};

pub const ACTIONS: &[(Permission, &str, &str)] = &[
	(Permission::ReadDashboard, "readDashboard", "View dashboard"),
	(
		Permission::ReadCatalogue,
		"readCatalogue",
		"View agents, providers and catalogue",
	),
	(
		Permission::ReadPolicies,
		"readPolicies",
		"View policies, DLP and approvals",
	),
	(
		Permission::EditPolicy,
		"editPolicy",
		"Edit and roll back policies",
	),
	(Permission::EditDlp, "editDlp", "Edit DLP rules"),
	(
		Permission::SubmitApproval,
		"submitApproval",
		"Submit approval requests",
	),
	(
		Permission::Approve,
		"approve",
		"Approve another person's changes",
	),
	(Permission::ReadAudit, "readAudit", "View audit log"),
	(Permission::ExportAudit, "exportAudit", "Export audit log"),
	(Permission::ReadSettings, "readSettings", "View settings"),
	(
		Permission::ChangeEnforcement,
		"changeEnforcement",
		"Change enforcement mode",
	),
	(
		Permission::ManageSettings,
		"manageSettings",
		"Change settings and kill switches",
	),
	(
		Permission::ImportPolicy,
		"importPolicy",
		"Import policy documents and registers",
	),
	(
		Permission::ExportPolicy,
		"exportPolicy",
		"Export policy documents and registers",
	),
	(
		Permission::ManageAccess,
		"manageAccess",
		"Manage users, groups and all role grants",
	),
];

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AccessRole {
	pub id: String,
	pub name: String,
	pub permissions: Vec<Permission>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UserGroup {
	pub id: String,
	pub name: String,
	pub roles: Vec<String>,
	pub members: Vec<String>,
	pub identity_groups: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PortalUser {
	pub subject: String,
	pub name: String,
	pub email: String,
	pub last_login: String,
	pub disabled: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AccessControl {
	pub roles: Vec<AccessRole>,
	pub groups: Vec<UserGroup>,
}

impl AccessControl {
	pub fn defaults(map: &RoleMap) -> Self {
		let definitions = [
			("owner", Role::Owner, &map.owner_groups),
			("admin", Role::Admin, &map.admin_groups),
			(
				"policy-editor",
				Role::PolicyEditor,
				&map.policy_editor_groups,
			),
			("approver", Role::Approver, &map.approver_groups),
			("auditor", Role::Auditor, &map.auditor_groups),
			("viewer", Role::Viewer, &map.viewer_groups),
		];
		Self {
			roles: definitions
				.iter()
				.map(|(id, role, _)| AccessRole {
					id: (*id).into(),
					name: role.as_str().into(),
					permissions: ACTIONS
						.iter()
						.filter(|(p, _, _)| role.allows(*p))
						.map(|(p, _, _)| *p)
						.collect(),
				})
				.collect(),
			groups: definitions
				.iter()
				.map(|(id, role, matches)| UserGroup {
					id: (*id).into(),
					name: format!("{} group", role.as_str()),
					roles: vec![(*id).into()],
					members: vec![],
					identity_groups: {
						let mut groups = (*matches).clone();
						// App-role values created by ./magpie.sh sso.
						let app_role = format!("Magpie.{}", role.as_str().replace(' ', ""));
						if !groups.contains(&app_role) {
							groups.push(app_role);
						}
						groups
					},
				})
				.collect(),
		}
	}

	pub fn roles_for(&self, session: &SessionRecord) -> Vec<&AccessRole> {
		self
			.roles
			.iter()
			.filter(|role| {
				if session.break_glass {
					return role.id == "owner";
				}
				self.groups.iter().any(|group| {
					group.roles.contains(&role.id)
						&& (group.members.contains(&session.subject)
							|| group
								.identity_groups
								.iter()
								.any(|id| session.identity_groups.contains(id)))
				})
			})
			.collect()
	}

	pub fn allows(&self, session: &SessionRecord, permission: Permission) -> bool {
		// The separately authenticated local recovery account cannot be locked out.
		session.break_glass
			|| self
				.roles_for(session)
				.iter()
				.any(|role| role.permissions.contains(&permission))
	}

	pub fn validate(&self) -> Result<()> {
		let fail = |message: &str| Error::Msg(message.into());
		for (index, role) in self.roles.iter().enumerate() {
			if !valid_id(&role.id) || role.name.trim().is_empty() || role.name.len() > 100 {
				return Err(fail(
					"Roles need a valid ID and a name of 1–100 characters.",
				));
			}
			if self.roles[..index].iter().any(|r| r.id == role.id)
				|| role
					.permissions
					.iter()
					.any(|p| !ACTIONS.iter().any(|(action, _, _)| p == action))
			{
				return Err(fail("Duplicate role or unknown action."));
			}
		}
		let owner = self
			.roles
			.iter()
			.find(|role| role.id == "owner")
			.ok_or_else(|| fail("The Owner role is required."))?;
		if ACTIONS
			.iter()
			.any(|(p, _, _)| !owner.permissions.contains(p))
		{
			return Err(fail("The Owner role must retain every action."));
		}
		for (index, group) in self.groups.iter().enumerate() {
			if !valid_id(&group.id)
				|| group.name.trim().is_empty()
				|| group.name.len() > 100
				|| self.groups[..index].iter().any(|g| g.id == group.id)
			{
				return Err(fail(
					"Groups need a unique ID and a name of 1–100 characters.",
				));
			}
			if group
				.roles
				.iter()
				.any(|id| !self.roles.iter().any(|r| &r.id == id))
			{
				return Err(fail("Remove the role from its groups before deleting it."));
			}
		}
		Ok(())
	}
}

pub fn valid_id(id: &str) -> bool {
	!id.is_empty()
		&& id.len() <= 64
		&& id
			.bytes()
			.all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == b'-')
}

#[cfg(test)]
mod tests {
	use super::*;
	use crate::identity::SessionCodec;
	use std::time::Duration;

	#[test]
	fn deployment_app_roles_map_to_explicit_builtin_grants() {
		let access = AccessControl::defaults(&RoleMap::default());
		let codec = SessionCodec::new("test", Duration::from_secs(60), Duration::from_secs(600));
		let (mut session, _) = codec
			.issue(
				"user",
				"user@example.test",
				"User",
				vec![],
				chrono::Utc::now(),
				false,
			)
			.unwrap();
		assert!(!access.allows(&session, Permission::ReadDashboard));
		session.identity_groups = vec!["Magpie.Auditor".into()];
		assert!(access.allows(&session, Permission::ReadAudit));
		assert!(!access.allows(&session, Permission::ChangeEnforcement));
		session.identity_groups = vec!["Magpie.Owner".into()];
		assert!(ACTIONS.iter().all(|(p, _, _)| access.allows(&session, *p)));
	}
}
