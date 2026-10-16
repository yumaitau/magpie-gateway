//! Persistent portal roles and groups. Grants are evaluated on every action.

use crate::identity::{Permission, Role, RoleMap, SessionRecord};
use crate::{Error, Result};
use serde::{Deserialize, Serialize};

pub const ACTIONS: &[(Permission, &str, &str)] = &[
	(
		Permission::ReadGatewayConfig,
		"readGatewayConfig",
		"Read gateway configuration (includes provider credentials)",
	),
	(
		Permission::EditGatewayModels,
		"editGatewayModels",
		"Configure gateway models and their policies",
	),
	(
		Permission::EditGatewayProviders,
		"editGatewayProviders",
		"Configure gateway providers",
	),
	(
		Permission::EditGatewayKeys,
		"editGatewayKeys",
		"Manage gateway API keys",
	),
	(
		Permission::EditGatewayPolicies,
		"editGatewayPolicies",
		"Configure gateway policies and guardrails",
	),
	(
		Permission::EditGatewayMcp,
		"editGatewayMcp",
		"Configure MCP servers and policies",
	),
	(
		Permission::EditGatewayTraffic,
		"editGatewayTraffic",
		"Configure gateway listeners and traffic routes",
	),
	(
		Permission::EditGatewaySettings,
		"editGatewaySettings",
		"Configure gateway storage, observability and UI",
	),
	(
		Permission::ReadGatewayLogs,
		"readGatewayLogs",
		"Read gateway request logs and analytics",
	),
	(
		Permission::RunGatewayCel,
		"runGatewayCel",
		"Use the CEL playground",
	),
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

/// How a group's own traffic is handled, relative to the organisation default.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum GroupEnforcement {
	/// Use the organisation guardrail mode.
	#[default]
	Follow,
	/// Apply policy actions for this group even when the organisation is in a trial.
	Enforce,
	/// Record what would happen and leave this group's traffic unchanged.
	#[serde(rename = "log-only")]
	Observe,
}

impl std::str::FromStr for GroupEnforcement {
	type Err = crate::Error;

	fn from_str(value: &str) -> std::result::Result<Self, Self::Err> {
		match value {
			"follow" => Ok(Self::Follow),
			"enforce" => Ok(Self::Enforce),
			"log-only" => Ok(Self::Observe),
			_ => Err(crate::Error::Msg(
				"Choose follow, log-only or enforce for the group.".into(),
			)),
		}
	}
}

/// Result of matching a request to group settings. The organisation default
/// applies when this is [`EffectiveEnforcement::Organisation`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EffectiveEnforcement {
	Organisation,
	Enforce,
	Observe,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UserGroup {
	pub id: String,
	pub name: String,
	pub roles: Vec<String>,
	pub members: Vec<String>,
	pub identity_groups: Vec<String>,
	/// Gateway key names and other traffic actors. A request's actor is the
	/// virtual API key name the proxy sends.
	#[serde(default)]
	pub traffic_names: Vec<String>,
	#[serde(default)]
	pub enforcement: GroupEnforcement,
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
					traffic_names: vec![],
					enforcement: GroupEnforcement::Follow,
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

	/// Resolve traffic for `actor` (a gateway key name, user id, email or display name).
	/// `presented_groups` are identity-provider group ids supplied with the request.
	/// Enforcing wins when the person is in more than one explicit group.
	/// Requests with no person (`gateway`, `unknown` or blank) follow the organisation.
	pub fn effective_enforcement(
		&self,
		users: &[PortalUser],
		actor: &str,
		presented_groups: &[&str],
	) -> (EffectiveEnforcement, Vec<String>) {
		let actor = actor.trim();
		if actor.is_empty()
			|| actor.eq_ignore_ascii_case("gateway")
			|| actor.eq_ignore_ascii_case("unknown")
		{
			return (EffectiveEnforcement::Organisation, Vec::new());
		}
		let mut enforce = Vec::new();
		let mut observe = Vec::new();
		for group in &self.groups {
			if !group.covers(users, actor, presented_groups) {
				continue;
			}
			match group.enforcement {
				GroupEnforcement::Follow => {},
				GroupEnforcement::Enforce => enforce.push(group.id.clone()),
				GroupEnforcement::Observe => observe.push(group.id.clone()),
			}
		}
		enforce.sort();
		observe.sort();
		if !enforce.is_empty() {
			(EffectiveEnforcement::Enforce, enforce)
		} else if !observe.is_empty() {
			(EffectiveEnforcement::Observe, observe)
		} else {
			(EffectiveEnforcement::Organisation, Vec::new())
		}
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
			if group.traffic_names.len() > 40
				|| group
					.traffic_names
					.iter()
					.any(|name| !valid_traffic_name(name))
			{
				return Err(fail(
					"Gateway key names use letters, numbers and . _ - @, with at most 40 names of up to 100 characters.",
				));
			}
		}
		Ok(())
	}
}

impl UserGroup {
	fn covers(&self, users: &[PortalUser], actor: &str, presented_groups: &[&str]) -> bool {
		let actor_is = |value: &str| value.trim().eq_ignore_ascii_case(actor);
		if self.members.iter().any(|member| actor_is(member))
			|| self.traffic_names.iter().any(|name| actor_is(name))
		{
			return true;
		}
		if self.identity_groups.iter().any(|id| {
			actor_is(id)
				|| presented_groups
					.iter()
					.any(|group| group.trim().eq_ignore_ascii_case(id.trim()))
		}) {
			return true;
		}
		self.members.iter().any(|member| {
			users.iter().any(|user| {
				&user.subject == member
					&& (actor_is(&user.email)
						|| actor_is(&user.name)
						|| user
							.email
							.split('@')
							.next()
							.is_some_and(|local| local.len() >= 3 && actor_is(local)))
			})
		})
	}
}

pub fn traffic_names_from_lines(text: &str) -> Result<Vec<String>> {
	let mut names = Vec::new();
	for line in text.lines() {
		let name = line.trim();
		if name.is_empty() {
			continue;
		}
		if !valid_traffic_name(name) || names.len() >= 40 {
			return Err(crate::Error::Msg(
				"Gateway key names use letters, numbers and . _ - @, with at most 40 names of up to 100 characters.".into(),
			));
		}
		if !names
			.iter()
			.any(|existing: &String| existing.eq_ignore_ascii_case(name))
		{
			names.push(name.to_string());
		}
	}
	Ok(names)
}

fn valid_traffic_name(name: &str) -> bool {
	!name.is_empty()
		&& name.len() <= 100
		&& name
			.chars()
			.all(|c| c.is_ascii_alphanumeric() || matches!(c, '.' | '_' | '-' | '@'))
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

	#[test]
	fn group_enforcement_is_per_person_and_enforce_wins() {
		let user = PortalUser {
			subject: "example-user".into(),
			name: "Alex Nguyen".into(),
			email: "alex.nguyen@example.gov.au".into(),
			last_login: String::new(),
			disabled: false,
		};
		let mut access = AccessControl::defaults(&RoleMap::default());
		access.groups.push(UserGroup {
			id: "pilot".into(),
			name: "Pilot".into(),
			roles: vec![],
			members: vec!["example-user".into()],
			identity_groups: vec!["contractors".into()],
			traffic_names: vec!["justin".into()],
			enforcement: GroupEnforcement::Enforce,
		});
		access.groups.push(UserGroup {
			id: "hold".into(),
			name: "Hold".into(),
			roles: vec![],
			members: vec![],
			identity_groups: vec![],
			traffic_names: vec!["justin".into()],
			enforcement: GroupEnforcement::Observe,
		});
		let users = [user];
		assert_eq!(
			access.effective_enforcement(&users, "gateway", &[]).0,
			EffectiveEnforcement::Organisation
		);
		assert_eq!(
			access.effective_enforcement(&users, "justin", &[]).0,
			EffectiveEnforcement::Enforce
		);
		assert_eq!(
			access.effective_enforcement(&users, "alex.nguyen", &[]).0,
			EffectiveEnforcement::Enforce
		);
		assert_eq!(
			access.effective_enforcement(&users, "Alex Nguyen", &[]).0,
			EffectiveEnforcement::Enforce
		);
		let (mode, groups) = access.effective_enforcement(&users, "other", &["contractors"]);
		assert_eq!(mode, EffectiveEnforcement::Enforce);
		assert_eq!(groups, vec!["pilot".to_string()]);
		access.groups.retain(|group| group.id != "pilot");
		assert_eq!(
			access.effective_enforcement(&users, "justin", &[]).0,
			EffectiveEnforcement::Observe
		);
		let stored = serde_json::from_str::<UserGroup>(
			r#"{"id":"saved","name":"Saved","roles":[],"members":[],"identity_groups":[]}"#,
		)
		.unwrap();
		assert_eq!(stored.enforcement, GroupEnforcement::Follow);
		assert!(stored.traffic_names.is_empty());
	}
}
