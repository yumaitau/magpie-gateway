use super::*;
use magpie::rbac::{ACTIONS, AccessControl, AccessRole, UserGroup};

pub(crate) fn allows(state: &AppState, session: &SessionRecord, permission: Permission) -> bool {
	if session.break_glass {
		return true;
	}
	state
		.inner
		.store
		.lock()
		.expect("db")
		.access_control()
		.ok()
		.flatten()
		.is_some_and(|access| access.allows(session, permission))
}

pub(crate) fn permissions(state: &AppState, session: &SessionRecord) -> Vec<Permission> {
	let access = state
		.inner
		.store
		.lock()
		.expect("db")
		.access_control()
		.ok()
		.flatten();
	ACTIONS
		.iter()
		.filter(|(p, _, _)| {
			session.break_glass || access.as_ref().is_some_and(|a| a.allows(session, *p))
		})
		.map(|(p, _, _)| *p)
		.collect()
}

pub(crate) fn role_names(state: &AppState, session: &SessionRecord) -> Vec<String> {
	state
		.inner
		.store
		.lock()
		.expect("db")
		.access_control()
		.ok()
		.flatten()
		.map(|access| {
			access
				.roles_for(session)
				.iter()
				.map(|r| r.name.clone())
				.collect()
		})
		.unwrap_or_default()
}

pub(crate) fn page_permission(path: &str) -> Permission {
	match path {
		"/dashboard" => Permission::ReadDashboard,
		"/policies" | "/dlp" | "/approvals" => Permission::ReadPolicies,
		"/audit" => Permission::ReadAudit,
		"/settings" => Permission::ReadSettings,
		"/access" => Permission::ManageAccess,
		_ => Permission::ReadCatalogue,
	}
}

fn checkbox(name: &str, label: &str, checked: bool, disabled: bool) -> String {
	format!(
		r#"<label class="choice"><input type="checkbox" name="{}" value="1" {} {}> {}</label>"#,
		ui::esc(name),
		if checked { "checked" } else { "" },
		if disabled { "disabled" } else { "" },
		ui::esc(label)
	)
}

pub(super) async fn access_page(State(state): State<AppState>, headers: HeaderMap) -> Response {
	if let Err(response) = require(&state, &headers, Permission::ManageAccess) {
		return response;
	}
	let store = state.inner.store.lock().expect("db");
	let access = match store.access_control() {
		Ok(Some(access)) => access,
		_ => {
			return (
				StatusCode::INTERNAL_SERVER_ERROR,
				"Could not load access controls.",
			)
				.into_response();
		},
	};
	let users = store.portal_users().unwrap_or_default();
	drop(store);
	let mut body = String::from(
		r#"<p>Roles grant individual actions. User groups assign those roles to people and identity-provider groups. A user receives the combined permissions of all matching groups. Unassigned users have no portal access.</p>
<p>Changes are audited and apply to the next action, including existing sessions. Identity-provider group changes are refreshed at the next sign-in. Disabling a user denies their next portal request.</p>
<p>The Owner role and local recovery administrator retain all actions. You cannot remove your own access-management permission. Anyone granted Manage users, groups and all role grants can grant any role.</p>
<h2>Roles</h2>"#,
	);
	for role in access.roles.iter().chain(std::iter::once(&AccessRole {
		id: String::new(),
		name: String::new(),
		permissions: vec![],
	})) {
		let new = role.id.is_empty();
		let owner = role.id == "owner";
		let key = if new { "new" } else { &role.id };
		body.push_str(&format!(r#"<details><summary>{}</summary><form method="post" action="/access/roles">
<label for="role-id-{key}">Role ID</label><input id="role-id-{key}" name="id" value="{}" required pattern="[a-z0-9-]+" maxlength="64" {}>
<label for="role-name-{key}">Role name</label><input id="role-name-{key}" name="name" value="{}" required maxlength="100" {}>
<fieldset><legend>Allowed actions</legend><div class="choices">"#,
			if new { "Create a role".into() } else { ui::esc(&role.name) }, ui::esc(&role.id), if new { "" } else { "readonly" }, ui::esc(&role.name), if owner { "readonly" } else { "" }));
		for (permission, id, label) in ACTIONS {
			body.push_str(&checkbox(
				&format!("permission.{id}"),
				label,
				role.permissions.contains(permission),
				owner,
			));
		}
		body.push_str("</div></fieldset>");
		if !owner {
			body.push_str("<button type=\"submit\" name=\"operation\" value=\"save\">Save role</button>");
			if !new {
				body.push_str(
					" <button type=\"submit\" name=\"operation\" value=\"delete\">Delete role</button>",
				);
			}
		} else {
			body.push_str("<p>The Owner role is reserved for full access and cannot be changed.</p>");
		}
		body.push_str("</form></details>");
	}
	body.push_str("<h2>User groups</h2><p>Select people who have signed in, or map the exact group IDs or app-role values supplied by your identity provider. Creating a group does not create login credentials.</p>");
	for group in access.groups.iter().chain(std::iter::once(&UserGroup {
		id: String::new(),
		name: String::new(),
		roles: vec![],
		members: vec![],
		identity_groups: vec![],
	})) {
		let new = group.id.is_empty();
		let key = if new { "new" } else { &group.id };
		body.push_str(&format!(r#"<details><summary>{}</summary><form method="post" action="/access/groups">
<label for="group-id-{key}">Group ID</label><input id="group-id-{key}" name="id" value="{}" required pattern="[a-z0-9-]+" maxlength="64" {}>
<label for="group-name-{key}">Group name</label><input id="group-name-{key}" name="name" value="{}" required maxlength="100">
<fieldset><legend>Assigned roles</legend><div class="choices">"#,
			if new { "Create a user group".into() } else { ui::esc(&group.name) }, ui::esc(&group.id), if new { "" } else { "readonly" }, ui::esc(&group.name)));
		for role in &access.roles {
			body.push_str(&checkbox(
				&format!("role.{}", role.id),
				&role.name,
				group.roles.contains(&role.id),
				false,
			));
		}
		body.push_str(
			"</div></fieldset><fieldset><legend>People in this group</legend><div class=\"choices\">",
		);
		for user in &users {
			body.push_str(&checkbox(
				&format!("member.{}", user.subject),
				&format!("{} ({}) — {}", user.name, user.email, user.subject),
				group.members.contains(&user.subject),
				false,
			));
		}
		// Preserve members provisioned before a user has first signed in.
		for subject in group
			.members
			.iter()
			.filter(|id| !users.iter().any(|u| &u.subject == *id))
		{
			body.push_str(&checkbox(
				&format!("member.{subject}"),
				subject,
				true,
				false,
			));
		}
		body.push_str(&format!(r#"</div></fieldset><label for="idp-{key}">Identity-provider group IDs or app roles (one per line)</label><textarea id="idp-{key}" name="identity_groups" rows="3">{}</textarea>
<button type="submit" name="operation" value="save">Save group</button>"#, ui::esc(&group.identity_groups.join("\n"))));
		if !new {
			body.push_str(
				" <button type=\"submit\" name=\"operation\" value=\"delete\">Delete group</button>",
			);
		}
		body.push_str("</form></details>");
	}
	body.push_str("<h2>Users</h2><p>People appear here after signing in. Membership uses the stable identity-provider user ID, rather than an editable email address. Assign them to a group above to grant access.</p>");
	let rows = users
		.iter()
		.map(|user| {
			vec![
				user.name.clone(),
				user.email.clone(),
				user.subject.clone(),
				user.last_login.clone(),
				if user.disabled {
					"Disabled".into()
				} else {
					"Enabled".into()
				},
			]
		})
		.collect::<Vec<_>>();
	body.push_str(&table(
		&["Name", "Email", "User ID", "Last sign-in", "Status"],
		&rows,
	));
	body.push_str(r#"<form method="post" action="/access/users"><label for="access-user">User</label><select id="access-user" name="subject">"#);
	for user in &users {
		body.push_str(&format!(
			"<option value=\"{}\">{} ({})</option>",
			ui::esc(&user.subject),
			ui::esc(&user.name),
			ui::esc(&user.email)
		));
	}
	body.push_str(r#"</select><button type="submit" name="disabled" value="true">Disable user</button> <button type="submit" name="disabled" value="false">Enable user</button></form>"#);
	page(&state, &headers, "/access", "Access control", body)
}

fn value<'a>(form: &'a HashMap<String, String>, key: &str) -> &'a str {
	form.get(key).map(|v| v.trim()).unwrap_or("")
}
fn checked(form: &HashMap<String, String>, prefix: &str) -> Vec<String> {
	let mut values = form
		.iter()
		.filter(|(_, v)| v.as_str() == "1")
		.filter_map(|(k, _)| k.strip_prefix(prefix).map(str::to_string))
		.collect::<Vec<_>>();
	values.sort();
	values.dedup();
	values
}

fn save(
	state: &AppState,
	session: &SessionRecord,
	update: impl FnOnce(&mut AccessControl) -> magpie::Result<()>,
) -> Response {
	let store = state.inner.store.lock().expect("db");
	let result = (|| -> magpie::Result<()> {
		let mut access = store
			.access_control()?
			.ok_or_else(|| magpie::Error::Msg("Access controls are unavailable.".into()))?;
		// Recheck under the write lock so a concurrent revocation takes effect.
		if !access.allows(session, Permission::ManageAccess) {
			return Err(magpie::Error::Forbidden);
		}
		update(&mut access)?;
		access.validate()?;
		if !access.allows(session, Permission::ManageAccess) {
			return Err(magpie::Error::Msg("This would remove your own access-management permission. Ask another access administrator to make that change.".into()));
		}
		store.set_access_control(&access, &session.email)
	})();
	match result {
		Ok(()) => Redirect::to("/access").into_response(),
		Err(error) => (StatusCode::BAD_REQUEST, error.to_string()).into_response(),
	}
}

pub(super) async fn role_post(
	State(state): State<AppState>,
	headers: HeaderMap,
	Form(form): Form<HashMap<String, String>>,
) -> Response {
	let session = match require(&state, &headers, Permission::ManageAccess) {
		Ok(s) => s,
		Err(r) => return r,
	};
	save(&state, &session, |access| {
		let id = value(&form, "id");
		if id == "owner" {
			return Err(magpie::Error::Msg(
				"The Owner role cannot be changed.".into(),
			));
		}
		let selected = checked(&form, "permission.");
		if selected
			.iter()
			.any(|id| !ACTIONS.iter().any(|(_, known, _)| id == known))
		{
			return Err(magpie::Error::Msg("Unknown action.".into()));
		}
		access.roles.retain(|role| role.id != id);
		if value(&form, "operation") != "delete" {
			access.roles.push(AccessRole {
				id: id.into(),
				name: value(&form, "name").into(),
				permissions: ACTIONS
					.iter()
					.filter(|(_, id, _)| selected.iter().any(|s| s == id))
					.map(|(p, _, _)| *p)
					.collect(),
			});
		}
		Ok(())
	})
}

pub(super) async fn group_post(
	State(state): State<AppState>,
	headers: HeaderMap,
	Form(form): Form<HashMap<String, String>>,
) -> Response {
	let session = match require(&state, &headers, Permission::ManageAccess) {
		Ok(s) => s,
		Err(r) => return r,
	};
	save(&state, &session, |access| {
		let id = value(&form, "id");
		access.groups.retain(|group| group.id != id);
		if value(&form, "operation") != "delete" {
			access.groups.push(UserGroup {
				id: id.into(),
				name: value(&form, "name").into(),
				roles: checked(&form, "role."),
				members: checked(&form, "member."),
				identity_groups: value(&form, "identity_groups")
					.lines()
					.map(str::trim)
					.filter(|s| !s.is_empty())
					.map(str::to_string)
					.collect(),
			});
		}
		Ok(())
	})
}

pub(super) async fn user_post(
	State(state): State<AppState>,
	headers: HeaderMap,
	Form(form): Form<HashMap<String, String>>,
) -> Response {
	let session = match require(&state, &headers, Permission::ManageAccess) {
		Ok(s) => s,
		Err(r) => return r,
	};
	let subject = value(&form, "subject");
	if subject == session.subject || subject == state.inner.config.break_glass.username {
		return (
			StatusCode::BAD_REQUEST,
			"You cannot disable yourself or the local recovery administrator.",
		)
			.into_response();
	}
	let disabled = match value(&form, "disabled") {
		"true" => true,
		"false" => false,
		_ => return StatusCode::BAD_REQUEST.into_response(),
	};
	let store = state.inner.store.lock().expect("db");
	match store.set_user_disabled(subject, disabled, &session.email) {
		Ok(()) => Redirect::to("/access").into_response(),
		Err(error) => (StatusCode::BAD_REQUEST, error.to_string()).into_response(),
	}
}
