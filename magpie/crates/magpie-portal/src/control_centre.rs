//! Unified Control Centre & Approvals Inbox JSON APIs for enterprise and government operators.
use super::*;

fn json_error(status: StatusCode, message: impl Into<String>) -> Response {
	(status, Json(json!({ "error": message.into() }))).into_response()
}

pub(super) fn routes() -> Router<AppState> {
	Router::new()
		.route("/api/magpie/control-centre", get(overview))
		.route("/api/magpie/control-centre/mode", post(set_mode))
		.route(
			"/api/magpie/control-centre/cohorts/{id}",
			post(set_cohort_enforcement),
		)
		.route(
			"/api/magpie/control-centre/kill-switch",
			post(set_kill_switch),
		)
		.route("/api/magpie/control-centre/settings", post(update_settings))
		.route("/api/magpie/control-centre/preset", post(apply_preset))
		.route(
			"/api/magpie/approvals",
			get(list_approvals).post(submit_approval),
		)
		.route(
			"/api/magpie/approvals/revisions/{id}/decide",
			post(decide_revision),
		)
		.route(
			"/api/magpie/approvals/items/{id}/decide",
			post(decide_item_approval),
		)
}

pub(super) fn can_view_control_centre(state: &AppState, session: &SessionRecord) -> bool {
	[
		Permission::ReadSettings,
		Permission::ReadPolicies,
		Permission::ReadDashboard,
		Permission::ChangeEnforcement,
		Permission::ManageSettings,
	]
	.into_iter()
	.any(|p| access::allows(state, session, p))
}

pub(super) fn can_view_approvals(state: &AppState, session: &SessionRecord) -> bool {
	access::allows(state, session, Permission::ReadPolicies)
		|| access::allows(state, session, Permission::Approve)
}

pub(super) fn pending_counts(state: &AppState) -> (usize, usize) {
	let store = state.inner.store.lock().expect("db");
	let pending_revisions = store
		.list_pending_revisions()
		.map(|items| items.len())
		.unwrap_or(0);
	let pending_items = store
		.list_documents("approval")
		.unwrap_or_default()
		.into_iter()
		.filter(|doc| doc.active)
		.filter_map(|doc| serde_json::from_value::<Approval>(doc.body).ok())
		.filter(|approval| approval.status == ApprovalStatus::Pending)
		.count();
	(pending_revisions, pending_items)
}

pub(super) fn active_kill_switch(state: &AppState) -> KillSwitch {
	let store = state.inner.store.lock().expect("db");
	store
		.get_document("kill", "global")
		.ok()
		.flatten()
		.and_then(|doc| serde_json::from_value(doc.body).ok())
		.unwrap_or_default()
}

#[derive(Clone, Copy)]
struct PresetSpec {
	id: &'static str,
	name: &'static str,
	audience: &'static str,
	summary: &'static str,
	honest_note: &'static str,
	mode: guardrail::Mode,
	sovereign_mode: bool,
	telemetry_enabled: bool,
	retention_days: u32,
	dlp_profile: DlpProfile,
}

#[derive(Clone, Copy)]
enum DlpProfile {
	ObserveDefault,
	EnterpriseStandard,
	GovOfficialSensitive,
	GovProtected,
}

const PRESETS: &[PresetSpec] = &[
	PresetSpec {
		id: "pilot-observe",
		name: "Pilot / Observe Only",
		audience: "Initial rollout & tuning",
		summary: "Evaluate Australian DLP, PSPF markings, and sovereign routing without blocking or altering traffic.",
		honest_note: "Traffic passes through unchanged while would-be actions are recorded in Decision Audit. Authentication and portal RBAC remain enforced.",
		mode: guardrail::Mode::LogOnly,
		sovereign_mode: true,
		telemetry_enabled: false,
		retention_days: 365,
		dlp_profile: DlpProfile::ObserveDefault,
	},
	PresetSpec {
		id: "enterprise-standard",
		name: "Enterprise Standard",
		audience: "Commercial & Microsoft 365 enterprises",
		summary: "Enforce blocks on credentials, payment cards, TFNs, Medicare, and high-risk IDs while logging general contact details. Leaves Sovereign Mode off so regional/data-zone commercial deployments can operate.",
		honest_note: "Built-in PSPF marking routing restrictions remain active. Review Azure OpenAI / Bedrock data residency contracts separately.",
		mode: guardrail::Mode::Enforce,
		sovereign_mode: false,
		telemetry_enabled: false,
		retention_days: 365,
		dlp_profile: DlpProfile::EnterpriseStandard,
	},
	PresetSpec {
		id: "gov-official-sensitive",
		name: "Government — OFFICIAL: Sensitive",
		audience: "Australian Government & agencies",
		summary: "Enforce Australian Sovereign Mode, all Australian personal identifiers, credentials, and PSPF handling controls with a 365-day audit retention target and telemetry off.",
		honest_note: "Supports ISM and DTA Responsible AI 2.0 controls on connected routes. Agencies still own endpoint controls, central SIEM retention, and impact assessments.",
		mode: guardrail::Mode::Enforce,
		sovereign_mode: true,
		telemetry_enabled: false,
		retention_days: 365,
		dlp_profile: DlpProfile::GovOfficialSensitive,
	},
	PresetSpec {
		id: "gov-protected",
		name: "Government — PROTECTED",
		audience: "High-assurance Australian Government",
		summary: "Strictest posture: enforces Australian Sovereign Mode, blocks all Australian identifiers, secrets, protective markings, and handling caveats, and restricts PROTECTED traffic to explicitly approved Australian destinations.",
		honest_note: "PROTECTED destinations must also be allow-listed via deployment configuration (MAGPIE_APPROVED_PROTECTED). SECRET and TOP SECRET markings are always refused.",
		mode: guardrail::Mode::Enforce,
		sovereign_mode: true,
		telemetry_enabled: false,
		retention_days: 365,
		dlp_profile: DlpProfile::GovProtected,
	},
];

fn preset_rule_target(profile: DlpProfile, kind: dlp::HitKind) -> Option<(bool, dlp::Action)> {
	use dlp::{Action, HitKind};
	if kind == HitKind::Presidio {
		// Leave optional external Presidio analyser untouched so presets never create surprise external calls.
		return None;
	}
	let target = match profile {
		DlpProfile::ObserveDefault | DlpProfile::GovOfficialSensitive => match kind {
			HitKind::Marking | HitKind::Caveat => (true, Action::Log),
			_ => (true, Action::Redact),
		},
		DlpProfile::EnterpriseStandard => match kind {
			HitKind::Secret
			| HitKind::PaymentCard
			| HitKind::Tfn
			| HitKind::Medicare
			| HitKind::Ihi
			| HitKind::Dva
			| HitKind::Crn
			| HitKind::Passport
			| HitKind::PassportMrz
			| HitKind::DriverLicence
			| HitKind::Bsb
			| HitKind::AccountNumber => (true, Action::Redact),
			HitKind::Email
			| HitKind::Phone
			| HitKind::Address
			| HitKind::Abn
			| HitKind::Acn
			| HitKind::Marking
			| HitKind::Caveat => (true, Action::Log),
			HitKind::Presidio => unreachable!(),
		},
		DlpProfile::GovProtected => (true, Action::Block),
	};
	Some(target)
}

#[derive(serde::Serialize)]
#[serde(rename_all = "camelCase")]
struct PresetChange {
	area: String,
	label: String,
	current: String,
	target: String,
	requires_approval: bool,
}

fn describe_preset_changes(
	preset: &PresetSpec,
	settings: &Settings,
	rules: &[DlpRule],
) -> Vec<PresetChange> {
	let mut changes = Vec::new();
	if settings.guardrail_mode != preset.mode {
		changes.push(PresetChange {
			area: "mode".into(),
			label: "Organisation enforcement mode".into(),
			current: mode_label(settings.guardrail_mode).into(),
			target: mode_label(preset.mode).into(),
			requires_approval: false,
		});
	}
	if settings.sovereign_mode != preset.sovereign_mode {
		changes.push(PresetChange {
			area: "settings".into(),
			label: "Australian Sovereign Mode".into(),
			current: on_off(settings.sovereign_mode).into(),
			target: on_off(preset.sovereign_mode).into(),
			requires_approval: settings.require_approval,
		});
	}
	if settings.telemetry_enabled != preset.telemetry_enabled {
		changes.push(PresetChange {
			area: "settings".into(),
			label: "Telemetry and phone-home".into(),
			current: on_off(settings.telemetry_enabled).into(),
			target: on_off(preset.telemetry_enabled).into(),
			requires_approval: settings.require_approval,
		});
	}
	if settings.retention_days != preset.retention_days {
		changes.push(PresetChange {
			area: "settings".into(),
			label: "Audit retention target".into(),
			current: format!("{} days", settings.retention_days),
			target: format!("{} days", preset.retention_days),
			requires_approval: settings.require_approval,
		});
	}
	for rule in rules {
		let Some((enabled, action)) = preset_rule_target(preset.dlp_profile, rule.kind) else {
			continue;
		};
		if rule.enabled != enabled || rule.action != action {
			changes.push(PresetChange {
				area: "dlp".into(),
				label: format!("DLP · {}", dlp::kind_name(rule.kind)),
				current: rule_state_label(rule.enabled, rule.action),
				target: rule_state_label(enabled, action),
				requires_approval: settings.require_approval,
			});
		}
	}
	changes
}

fn mode_label(mode: guardrail::Mode) -> &'static str {
	match mode {
		guardrail::Mode::LogOnly => "Log-only trial",
		guardrail::Mode::Enforce => "Enforcing",
	}
}

fn on_off(value: bool) -> &'static str {
	if value { "On" } else { "Off" }
}

fn rule_state_label(enabled: bool, action: dlp::Action) -> String {
	if !enabled {
		"Disabled".into()
	} else {
		action.as_str().into()
	}
}

fn readiness_summary(recent: &[AuditEvent]) -> Value {
	let would_act = recent
		.iter()
		.filter(|event| {
			matches!(
				event
					.detail
					.get("wouldAction")
					.and_then(|value| value.as_str())
					.unwrap_or(""),
				"block" | "redact" | "mask" | "tokenise" | "requireApproval"
			)
		})
		.count();
	let message = if recent.is_empty() {
		"No gateway decisions recorded yet. Send a synthetic prompt through a connected app, review Decision Audit, then enforce one pilot group first.".to_string()
	} else if would_act == 0 {
		format!(
			"The latest {} decisions have no findings that would block or alter traffic. Test with a synthetic TFN or credential before widening enforcement.",
			recent.len()
		)
	} else {
		format!(
			"The latest {} decisions include {} that would block or alter traffic when enforced. Review Decision Audit and tune noisy detectors before expanding enforcement.",
			recent.len(),
			would_act
		)
	};
	json!({
		"recentDecisions": recent.len(),
		"wouldActCount": would_act,
		"message": message,
	})
}

async fn overview(State(state): State<AppState>, headers: HeaderMap) -> Response {
	let (session, _) = match session_from(&state, &headers) {
		Ok(s) => s,
		Err(r) => return r,
	};
	if !can_view_control_centre(&state, &session) {
		return json_error(
			StatusCode::FORBIDDEN,
			"Your role cannot view the Control Centre.",
		);
	}
	let settings = load_settings(&state);
	let kill = active_kill_switch(&state);
	let rules = load_rules(&state);
	let (pending_revisions, pending_items) = pending_counts(&state);

	let can_change_enforcement = access::allows(&state, &session, Permission::ChangeEnforcement);
	let can_manage_settings = access::allows(&state, &session, Permission::ManageSettings);
	let can_edit_dlp = access::allows(&state, &session, Permission::EditDlp);
	let can_approve = access::allows(&state, &session, Permission::Approve);
	let can_submit_approval = access::allows(&state, &session, Permission::SubmitApproval);
	let can_manage_access = access::allows(&state, &session, Permission::ManageAccess);
	let can_read_audit = access::allows(&state, &session, Permission::ReadAudit);
	let can_read_policies = access::allows(&state, &session, Permission::ReadPolicies);
	let can_read_settings = access::allows(&state, &session, Permission::ReadSettings);

	let (cohorts, readiness) = {
		let store = state.inner.store.lock().expect("db");
		let cohorts = if can_read_settings || can_manage_access || can_change_enforcement {
			let access_ctrl = store.access_control().ok().flatten();
			let users = store.portal_users().unwrap_or_default();
			access_ctrl
				.map(|ac| {
					ac.groups
						.into_iter()
						.map(|g| {
							let member_names: Vec<String> = g
								.members
								.iter()
								.map(|subject| {
									users
										.iter()
										.find(|u| &u.subject == subject)
										.map(|u| format!("{} ({})", u.name, u.email))
										.unwrap_or_else(|| subject.clone())
								})
								.collect();
							json!({
								"id": g.id,
								"name": g.name,
								"roles": g.roles,
								"memberCount": g.members.len(),
								"memberNames": member_names,
								"identityGroups": g.identity_groups,
								"trafficNames": g.traffic_names,
								"enforcement": g.enforcement,
							})
						})
						.collect::<Vec<_>>()
				})
				.unwrap_or_default()
		} else {
			Vec::new()
		};
		let readiness = if can_read_audit || can_read_settings || can_change_enforcement {
			let recent = store
				.search_audit(&AuditQuery {
					limit: 200,
					..AuditQuery::default()
				})
				.unwrap_or_default();
			readiness_summary(&recent)
		} else {
			json!({ "recentDecisions": 0, "wouldActCount": 0, "message": "" })
		};
		(cohorts, readiness)
	};

	let enabled_rules = rules.iter().filter(|r| r.enabled).count();
	let blocking_rules = rules
		.iter()
		.filter(|r| {
			r.enabled
				&& matches!(
					r.action,
					dlp::Action::Block
						| dlp::Action::Redact
						| dlp::Action::Mask
						| dlp::Action::Tokenise
						| dlp::Action::RequireApproval
				)
		})
		.count();
	let logging_rules = rules
		.iter()
		.filter(|r| r.enabled && matches!(r.action, dlp::Action::Log | dlp::Action::Allow))
		.count();

	let presets: Vec<Value> = PRESETS
		.iter()
		.map(|preset| {
			let changes = describe_preset_changes(preset, &settings, &rules);
			let missing_permissions = missing_permissions_for_changes(
				&changes,
				can_change_enforcement,
				can_manage_settings,
				can_edit_dlp,
			);
			json!({
				"id": preset.id,
				"name": preset.name,
				"audience": preset.audience,
				"summary": preset.summary,
				"honestNote": preset.honest_note,
				"targetMode": preset.mode,
				"targetSovereignMode": preset.sovereign_mode,
				"matchesCurrent": changes.is_empty(),
				"changes": changes,
				"canApply": missing_permissions.is_empty() && !changes.is_empty(),
				"missingPermissions": missing_permissions,
			})
		})
		.collect();

	Json(json!({
		"mode": settings.guardrail_mode,
		"sovereignMode": settings.sovereign_mode,
		"telemetryEnabled": settings.telemetry_enabled,
		"presidioAnalyzerUrl": settings.presidio_analyzer_url,
		"retentionDays": settings.retention_days,
		"requireApproval": settings.require_approval,
		"approvedProtected": settings.approved_protected,
		"killSwitch": {
			"global": kill.global,
			"agents": kill.agents,
		},
		"oidcEnabled": state.inner.config.oidc_enabled(),
		"breakGlassEnabled": state.inner.config.break_glass.enabled,
		"pendingRevisionsCount": if can_read_policies || can_approve { pending_revisions } else { 0 },
		"pendingApprovalsCount": if can_read_policies || can_approve { pending_items } else { 0 },
		"dlpSummary": {
			"enabled": enabled_rules,
			"total": rules.len(),
			"blocking": blocking_rules,
			"logging": logging_rules,
		},
		"readiness": readiness,
		"cohorts": cohorts,
		"presets": presets,
		"permissions": {
			"canChangeEnforcement": can_change_enforcement,
			"canManageSettings": can_manage_settings,
			"canEditDlp": can_edit_dlp,
			"canApprove": can_approve,
			"canSubmitApproval": can_submit_approval,
			"canManageAccess": can_manage_access,
			"canReadAudit": can_read_audit,
			"canReadPolicies": can_read_policies,
			"canReadSettings": can_read_settings,
		}
	}))
	.into_response()
}

fn missing_permissions_for_changes(
	changes: &[PresetChange],
	can_change_enforcement: bool,
	can_manage_settings: bool,
	can_edit_dlp: bool,
) -> Vec<&'static str> {
	let mut missing = Vec::new();
	if changes.iter().any(|c| c.area == "mode") && !can_change_enforcement {
		missing.push("Change enforcement mode");
	}
	if changes.iter().any(|c| c.area == "settings") && !can_manage_settings {
		missing.push("Change settings and kill switches");
	}
	if changes.iter().any(|c| c.area == "dlp") && !can_edit_dlp {
		missing.push("Edit DLP rules");
	}
	missing
}

#[derive(Deserialize)]
struct ModeBody {
	mode: String,
}

async fn set_mode(
	State(state): State<AppState>,
	headers: HeaderMap,
	Json(body): Json<ModeBody>,
) -> Response {
	let session = match require(&state, &headers, Permission::ChangeEnforcement) {
		Ok(s) => s,
		Err(r) => return r,
	};
	let mode = match body.mode.parse::<guardrail::Mode>() {
		Ok(m) => m,
		Err(_) => return json_error(StatusCode::BAD_REQUEST, "Choose log-only or enforce."),
	};
	match state
		.inner
		.store
		.lock()
		.expect("db")
		.set_guardrail_mode(mode, &session.email)
	{
		Ok(()) => Json(json!({ "mode": mode })).into_response(),
		Err(_) => json_error(
			StatusCode::INTERNAL_SERVER_ERROR,
			"Could not save the enforcement mode. No change was applied.",
		),
	}
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct CohortBody {
	enforcement: String,
	#[serde(default)]
	traffic_names: Option<Vec<String>>,
}

async fn set_cohort_enforcement(
	State(state): State<AppState>,
	headers: HeaderMap,
	Path(id): Path<String>,
	Json(body): Json<CohortBody>,
) -> Response {
	let session = match require(&state, &headers, Permission::ChangeEnforcement) {
		Ok(s) => s,
		Err(r) => return r,
	};
	let enforcement: magpie::rbac::GroupEnforcement = match body.enforcement.parse() {
		Ok(e) => e,
		Err(error) => return json_error(StatusCode::BAD_REQUEST, error.to_string()),
	};
	let cleaned_names = if let Some(raw_names) = body.traffic_names {
		match magpie::rbac::traffic_names_from_lines(&raw_names.join("\n")) {
			Ok(names) => Some(names),
			Err(error) => return json_error(StatusCode::BAD_REQUEST, error.to_string()),
		}
	} else {
		None
	};
	let store = state.inner.store.lock().expect("db");
	let result = (|| -> magpie::Result<Value> {
		let mut access = store
			.access_control()?
			.ok_or_else(|| magpie::Error::Msg("Access controls are unavailable.".into()))?;
		let updated = {
			let group = access
				.groups
				.iter_mut()
				.find(|group| group.id == id)
				.ok_or_else(|| magpie::Error::Msg("Choose a group that already exists.".into()))?;
			group.enforcement = enforcement;
			if let Some(names) = cleaned_names {
				group.traffic_names = names;
			}
			json!({
				"id": group.id,
				"name": group.name,
				"enforcement": group.enforcement,
				"trafficNames": group.traffic_names,
			})
		};
		store.set_access_control(&access, &session.email)?;
		Ok(updated)
	})();
	match result {
		Ok(updated) => Json(updated).into_response(),
		Err(error) => json_error(StatusCode::BAD_REQUEST, error.to_string()),
	}
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct KillSwitchBody {
	global: bool,
	#[serde(default)]
	agents: Vec<String>,
}

async fn set_kill_switch(
	State(state): State<AppState>,
	headers: HeaderMap,
	Json(body): Json<KillSwitchBody>,
) -> Response {
	let session = match require(&state, &headers, Permission::ManageSettings) {
		Ok(s) => s,
		Err(r) => return r,
	};
	let mut agents: Vec<String> = body
		.agents
		.into_iter()
		.map(|a| a.trim().to_string())
		.filter(|a| !a.is_empty())
		.collect();
	if agents.len() > 64 || agents.iter().any(|a| a.len() > 128) {
		return json_error(
			StatusCode::BAD_REQUEST,
			"Provide at most 64 agent IDs, each 128 characters or fewer.",
		);
	}
	agents.sort();
	agents.dedup();
	let kill = KillSwitch {
		global: body.global,
		agents: agents.clone(),
	};
	if save_revision(
		&state,
		"kill",
		"global",
		&session.email,
		serde_json::to_value(&kill).unwrap_or(json!({})),
		false,
	)
	.is_err()
	{
		return json_error(
			StatusCode::INTERNAL_SERVER_ERROR,
			"Could not update kill switch.",
		);
	}
	let reason = if kill.global {
		"Global kill switch enabled"
	} else if !kill.agents.is_empty() {
		"Per-agent kill switch updated"
	} else {
		"Kill switches cleared"
	};
	let _ = record_audit(
		&state,
		"policy",
		&session.email,
		"",
		"",
		"",
		"allow",
		reason,
	);
	Json(json!({ "global": kill.global, "agents": kill.agents })).into_response()
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct GovernanceSettingsBody {
	sovereign_mode: bool,
	telemetry_enabled: bool,
	retention_days: u32,
	#[serde(default)]
	presidio_analyzer_url: Option<String>,
}

async fn update_settings(
	State(state): State<AppState>,
	headers: HeaderMap,
	Json(body): Json<GovernanceSettingsBody>,
) -> Response {
	let session = match require(&state, &headers, Permission::ManageSettings) {
		Ok(s) => s,
		Err(r) => return r,
	};
	if body.retention_days == 0 || body.retention_days > 3650 {
		return json_error(
			StatusCode::BAD_REQUEST,
			"Audit retention target must be between 1 and 3650 days.",
		);
	}
	let current = load_settings(&state);
	let presidio = body
		.presidio_analyzer_url
		.map(|v| v.trim().to_string())
		.unwrap_or(current.presidio_analyzer_url.clone());
	if !presidio.is_empty() && !presidio.starts_with("http://") && !presidio.starts_with("https://") {
		return json_error(
			StatusCode::BAD_REQUEST,
			"Presidio analyser URL must start with http:// or https://, or be left blank.",
		);
	}
	let next_body = json!({
		"sovereignMode": body.sovereign_mode,
		"telemetryEnabled": body.telemetry_enabled,
		"presidioAnalyzerUrl": presidio,
		"capturePayloads": false,
		"retentionDays": body.retention_days,
		"requireApproval": current.require_approval,
		"approvedProtected": current.approved_protected,
	});
	match save_revision(
		&state,
		"settings",
		"global",
		&session.email,
		next_body,
		current.require_approval,
	) {
		Ok(status) => {
			let pending = status == magpie::change::RevisionStatus::Pending;
			let _ = record_audit(
				&state,
				"policy",
				&session.email,
				"",
				"",
				"",
				"allow",
				if pending {
					"governance settings staged for approval"
				} else {
					"governance settings updated"
				},
			);
			Json(json!({ "pending": pending })).into_response()
		},
		Err(_) => json_error(
			StatusCode::INTERNAL_SERVER_ERROR,
			"Could not save governance settings.",
		),
	}
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct PresetBody {
	preset_id: String,
}

async fn apply_preset(
	State(state): State<AppState>,
	headers: HeaderMap,
	Json(body): Json<PresetBody>,
) -> Response {
	let (session, _) = match session_from(&state, &headers) {
		Ok(s) => s,
		Err(r) => return r,
	};
	let Some(preset) = PRESETS.iter().find(|p| p.id == body.preset_id) else {
		return json_error(StatusCode::BAD_REQUEST, "Unknown posture preset.");
	};
	let settings = load_settings(&state);
	let rules = load_rules(&state);
	let changes = describe_preset_changes(preset, &settings, &rules);
	if changes.is_empty() {
		return json_error(
			StatusCode::BAD_REQUEST,
			"This preset already matches the active configuration.",
		);
	}
	let missing = missing_permissions_for_changes(
		&changes,
		access::allows(&state, &session, Permission::ChangeEnforcement),
		access::allows(&state, &session, Permission::ManageSettings),
		access::allows(&state, &session, Permission::EditDlp),
	);
	if !missing.is_empty() {
		return json_error(
			StatusCode::FORBIDDEN,
			format!(
				"Your role needs additional permission(s) to apply this preset: {}.",
				missing.join(", ")
			),
		);
	}

	let mut applied_immediate: Vec<String> = Vec::new();
	let mut pending_approval: Vec<String> = Vec::new();
	let mut skipped_pending: Vec<String> = Vec::new();

	// 1. Guardrail mode (runtime setting, takes effect immediately and is audited atomically).
	if settings.guardrail_mode != preset.mode {
		if state
			.inner
			.store
			.lock()
			.expect("db")
			.set_guardrail_mode(preset.mode, &session.email)
			.is_err()
		{
			return json_error(
				StatusCode::INTERNAL_SERVER_ERROR,
				"Could not update enforcement mode.",
			);
		}
		applied_immediate.push(format!(
			"Organisation enforcement mode set to {}",
			mode_label(preset.mode)
		));
	}

	let store = state.inner.store.lock().expect("db");
	let existing_pending = match store.list_pending_revisions() {
		Ok(list) => list,
		Err(_) => {
			return json_error(
				StatusCode::INTERNAL_SERVER_ERROR,
				"Could not inspect pending revisions.",
			);
		},
	};

	// 2. Settings document (respects second-person approval).
	let settings_changed = settings.sovereign_mode != preset.sovereign_mode
		|| settings.telemetry_enabled != preset.telemetry_enabled
		|| settings.retention_days != preset.retention_days;
	if settings_changed {
		if existing_pending
			.iter()
			.any(|r| r.kind == "settings" && r.doc_id == "global")
		{
			skipped_pending.push("Governance settings (already has a pending change)".into());
		} else {
			let next_body = json!({
				"sovereignMode": preset.sovereign_mode,
				"telemetryEnabled": preset.telemetry_enabled,
				"presidioAnalyzerUrl": settings.presidio_analyzer_url,
				"capturePayloads": false,
				"retentionDays": preset.retention_days,
				"requireApproval": settings.require_approval,
				"approvedProtected": settings.approved_protected,
			});
			match save_revision_in_store(
				&store,
				"settings",
				"global",
				&session.email,
				next_body,
				settings.require_approval,
			) {
				Ok(magpie::change::RevisionStatus::Pending) => {
					pending_approval.push("Governance settings".into());
				},
				Ok(_) => {
					applied_immediate.push("Governance settings updated".into());
				},
				Err(_) => {
					return json_error(
						StatusCode::INTERNAL_SERVER_ERROR,
						"Could not save governance settings.",
					);
				},
			}
		}
	}

	// 3. DLP rules (preserve exact-value exceptions; respect second-person approval).
	for rule in &rules {
		let Some((enabled, action)) = preset_rule_target(preset.dlp_profile, rule.kind) else {
			continue;
		};
		if rule.enabled == enabled && rule.action == action {
			continue;
		}
		if existing_pending
			.iter()
			.any(|r| r.kind == "dlp_rule" && r.doc_id == rule.id)
		{
			skipped_pending.push(format!(
				"DLP · {} (already has a pending change)",
				dlp::kind_name(rule.kind)
			));
			continue;
		}
		let mut updated = rule.clone();
		updated.enabled = enabled;
		updated.action = action;
		match save_revision_in_store(
			&store,
			"dlp_rule",
			&updated.id,
			&session.email,
			json!(updated),
			settings.require_approval,
		) {
			Ok(magpie::change::RevisionStatus::Pending) => {
				pending_approval.push(format!("DLP · {}", dlp::kind_name(rule.kind)));
			},
			Ok(_) => {
				applied_immediate.push(format!(
					"DLP · {} set to {}",
					dlp::kind_name(rule.kind),
					rule_state_label(enabled, action)
				));
			},
			Err(_) => {
				return json_error(
					StatusCode::INTERNAL_SERVER_ERROR,
					"Could not save DLP rule change.",
				);
			},
		}
	}

	let _ = store.append_audit(&AuditEvent::new(
		"policy",
		&session.email,
		"",
		"",
		"",
		"allow",
		&format!("Posture preset '{}' applied", preset.name),
		json!({
			"presetId": preset.id,
			"appliedImmediate": applied_immediate,
			"pendingApproval": pending_approval,
			"skippedPending": skipped_pending,
		}),
	));

	Json(json!({
		"presetId": preset.id,
		"presetName": preset.name,
		"appliedImmediate": applied_immediate,
		"pendingApproval": pending_approval,
		"skippedPending": skipped_pending,
	}))
	.into_response()
}

async fn list_approvals(State(state): State<AppState>, headers: HeaderMap) -> Response {
	let (session, _) = match session_from(&state, &headers) {
		Ok(s) => s,
		Err(r) => return r,
	};
	if !can_view_approvals(&state, &session) {
		return json_error(
			StatusCode::FORBIDDEN,
			"Your role cannot view approvals.",
		);
	}
	let can_approve = access::allows(&state, &session, Permission::Approve);
	let can_submit = access::allows(&state, &session, Permission::SubmitApproval);
	let can_export_policy = access::allows(&state, &session, Permission::ExportPolicy);
	let today = Utc::now().date_naive();

	let store = state.inner.store.lock().expect("db");
	let pending_revisions = store.list_pending_revisions().unwrap_or_default();
	let approval_docs = store.list_documents("approval").unwrap_or_default();
	drop(store);

	let revisions: Vec<Value> = pending_revisions
		.into_iter()
		.map(|rev| {
			let is_own = session.email.trim().eq_ignore_ascii_case(rev.actor.trim());
			let cannot_reason = if !can_approve {
				Some("Requires the Approve permission.")
			} else if is_own {
				Some("Separation of duties: a different authorised person must approve your change.")
			} else {
				None
			};
			let diff = magpie::change::summarise_diff(&rev.before, &rev.after);
			json!({
				"id": rev.id,
				"kind": rev.kind,
				"docId": rev.doc_id,
				"actor": rev.actor,
				"createdAt": rev.created_at.to_rfc3339(),
				"diff": diff,
				"before": rev.before,
				"after": rev.after,
				"canApprove": cannot_reason.is_none(),
				"canReject": can_approve,
				"cannotApproveReason": cannot_reason,
			})
		})
		.collect();

	let items: Vec<Value> = approval_docs
		.into_iter()
		.filter(|doc| doc.active)
		.filter_map(|doc| {
			let approval = serde_json::from_value::<Approval>(doc.body.clone()).ok()?;
			let effective = magpie::approvals::effective_status(&approval, today);
			let is_pending = approval.status == ApprovalStatus::Pending;
			let is_expired = chrono::NaiveDate::parse_from_str(&approval.expires_on, "%Y-%m-%d")
				.is_ok_and(|expiry| expiry < today);
			let is_own = doc.updated_by.eq_ignore_ascii_case(&session.email)
				|| approval.business_owner.eq_ignore_ascii_case(&session.email)
				|| approval.business_owner.eq_ignore_ascii_case(&session.name);
			let cannot_reason = if !is_pending {
				None
			} else if !can_approve {
				Some("Requires the Approve permission.")
			} else if is_own {
				Some("Separation of duties: the submitter or business owner cannot approve this request.")
			} else if is_expired {
				Some("This request's expiry date has passed.")
			} else {
				None
			};
			Some(json!({
				"id": doc.id,
				"kind": approval.kind,
				"name": approval.name,
				"businessOwner": approval.business_owner,
				"purpose": approval.purpose,
				"classification": approval.classification,
				"expiresOn": approval.expires_on,
				"status": effective,
				"submittedBy": doc.updated_by,
				"updatedAt": doc.updated_at,
				"canApprove": is_pending && cannot_reason.is_none(),
				"canReject": is_pending && can_approve,
				"cannotApproveReason": cannot_reason,
			}))
		})
		.collect();

	Json(json!({
		"revisions": revisions,
		"items": items,
		"canApprove": can_approve,
		"canSubmitApproval": can_submit,
		"canExportPolicy": can_export_policy,
		"viewerEmail": session.email,
	}))
	.into_response()
}

#[derive(Deserialize)]
struct DecideBody {
	decision: String,
}

async fn decide_revision(
	State(state): State<AppState>,
	headers: HeaderMap,
	Path(id): Path<String>,
	Json(body): Json<DecideBody>,
) -> Response {
	let session = match require(&state, &headers, Permission::Approve) {
		Ok(s) => s,
		Err(r) => return r,
	};
	let store = state.inner.store.lock().expect("db");
	let Some(mut revision) = store.get_revision(&id).ok().flatten() else {
		return json_error(StatusCode::NOT_FOUND, "No such pending revision.");
	};
	match body.decision.as_str() {
		"approve" => {
			if let Err(error) = magpie::change::approve(&mut revision, &session.email) {
				return json_error(StatusCode::FORBIDDEN, error.to_string());
			}
			let current = match store.get_document(&revision.kind, &revision.doc_id) {
				Ok(doc) => doc,
				Err(_) => {
					return json_error(
						StatusCode::INTERNAL_SERVER_ERROR,
						"Could not read the active document.",
					);
				},
			};
			if current.as_ref().map(|d| &d.body).unwrap_or(&json!({})) != &revision.before
				|| current.as_ref().is_some_and(|d| {
					chrono::DateTime::parse_from_rfc3339(&d.updated_at)
						.map_or(true, |updated| updated > revision.created_at)
				}) {
				revision.status = magpie::change::RevisionStatus::Rejected;
				if store.update_revision(&revision).is_err() {
					return json_error(
						StatusCode::INTERNAL_SERVER_ERROR,
						"Could not mark stale revision rejected.",
					);
				}
				let _ = store.append_audit(&AuditEvent::new(
					"policy",
					&session.email,
					"",
					"",
					"",
					"reject",
					"Stale revision rejected; submit the change again against the current document.",
					json!({ "revision": revision.id }),
				));
				return json_error(
					StatusCode::CONFLICT,
					"The active document changed. This stale revision was rejected; submit the change again.",
				);
			}
			let next_revision = current.as_ref().map(|doc| doc.revision + 1).unwrap_or(1);
			let document = Document {
				kind: revision.kind.clone(),
				id: revision.doc_id.clone(),
				body: revision.after.clone(),
				revision: next_revision,
				updated_at: Utc::now().to_rfc3339(),
				updated_by: session.email.clone(),
				active: true,
			};
			if store
				.save_changes(&[(revision.clone(), Some(document))])
				.is_err()
			{
				return json_error(
					StatusCode::INTERNAL_SERVER_ERROR,
					"Could not activate the revision.",
				);
			}
			drop(store);
			let _ = record_audit(
				&state,
				"policy",
				&session.email,
				"",
				&revision.doc_id,
				"",
				"allow",
				"revision approved",
			);
			Json(json!({ "id": revision.id, "status": "active" })).into_response()
		},
		"reject" => {
			if let Err(error) = magpie::change::reject(&mut revision, &session.email) {
				return json_error(StatusCode::CONFLICT, error.to_string());
			}
			if store.update_revision(&revision).is_err() {
				return json_error(
					StatusCode::INTERNAL_SERVER_ERROR,
					"Could not reject the revision.",
				);
			}
			drop(store);
			let _ = record_audit(
				&state,
				"policy",
				&session.email,
				"",
				&revision.doc_id,
				"",
				"reject",
				"revision rejected",
			);
			Json(json!({ "id": revision.id, "status": "rejected" })).into_response()
		},
		_ => json_error(StatusCode::BAD_REQUEST, "Choose approve or reject."),
	}
}

async fn decide_item_approval(
	State(state): State<AppState>,
	headers: HeaderMap,
	Path(id): Path<String>,
	Json(body): Json<DecideBody>,
) -> Response {
	let session = match require(&state, &headers, Permission::Approve) {
		Ok(s) => s,
		Err(r) => return r,
	};
	let store = state.inner.store.lock().expect("db");
	let Some(doc) = store.get_document("approval", &id).ok().flatten() else {
		return json_error(StatusCode::NOT_FOUND, "No such approval request.");
	};
	let Ok(mut approval) = serde_json::from_value::<Approval>(doc.body.clone()) else {
		return json_error(StatusCode::BAD_REQUEST, "The approval record is invalid.");
	};
	if !doc.active || approval.status != ApprovalStatus::Pending {
		return json_error(
			StatusCode::CONFLICT,
			"Only an active pending request can be decided.",
		);
	}
	match body.decision.as_str() {
		"approve" => {
			if magpie::approvals::validate(&approval).is_err()
				|| chrono::NaiveDate::parse_from_str(&approval.expires_on, "%Y-%m-%d")
					.is_ok_and(|expiry| expiry < Utc::now().date_naive())
			{
				return json_error(
					StatusCode::BAD_REQUEST,
					"Correct the request and expiry before approving it.",
				);
			}
			if doc.updated_by.eq_ignore_ascii_case(&session.email)
				|| approval.business_owner.eq_ignore_ascii_case(&session.email)
				|| approval.business_owner.eq_ignore_ascii_case(&session.name)
			{
				return json_error(StatusCode::FORBIDDEN, "You cannot approve your own request.");
			}
			approval.status = ApprovalStatus::Approved;
			let mut updated = doc;
			updated.body = serde_json::to_value(&approval).unwrap_or(json!({}));
			updated.revision += 1;
			updated.updated_by = session.email.clone();
			updated.updated_at = Utc::now().to_rfc3339();
			if store.put_document(&updated).is_err() {
				return json_error(
					StatusCode::INTERNAL_SERVER_ERROR,
					"Could not save approval.",
				);
			}
			if approval.kind == ItemKind::Tool {
				let _ = store.put_document(&Document {
					kind: "tool".into(),
					id: approval.name.clone(),
					body: json!({
						"server": "approved",
						"name": approval.name,
						"status": "approved",
						"classification": approval.classification
					}),
					revision: 1,
					updated_at: Utc::now().to_rfc3339(),
					updated_by: session.email.clone(),
					active: true,
				});
			}
			drop(store);
			let approvals = load_approvals(&state);
			let cel =
				magpie::approvals::cel_for_approved_tools(&approvals, Utc::now().date_naive());
			let _ = save_revision(
				&state,
				"policy",
				"approved-tools",
				&session.email,
				json!({ "cel": cel }),
				false,
			);
			let _ = record_audit(
				&state,
				"approval",
				&session.email,
				"",
				&approval.name,
				"",
				"allow",
				"approved",
			);
			Json(json!({ "id": id, "status": "approved" })).into_response()
		},
		"reject" => {
			approval.status = ApprovalStatus::Rejected;
			let mut updated = doc;
			updated.body = serde_json::to_value(&approval).unwrap_or(json!({}));
			updated.revision += 1;
			updated.updated_by = session.email.clone();
			updated.updated_at = Utc::now().to_rfc3339();
			if store.put_document(&updated).is_err() {
				return json_error(
					StatusCode::INTERNAL_SERVER_ERROR,
					"Could not reject approval.",
				);
			}
			drop(store);
			let _ = record_audit(
				&state,
				"approval",
				&session.email,
				"",
				&approval.name,
				"",
				"reject",
				"approval rejected",
			);
			Json(json!({ "id": id, "status": "rejected" })).into_response()
		},
		_ => json_error(StatusCode::BAD_REQUEST, "Choose approve or reject."),
	}
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct SubmitApprovalBody {
	name: String,
	kind: String,
	business_owner: String,
	purpose: String,
	classification: String,
	expires_on: String,
}

async fn submit_approval(
	State(state): State<AppState>,
	headers: HeaderMap,
	Json(body): Json<SubmitApprovalBody>,
) -> Response {
	let session = match require(&state, &headers, Permission::SubmitApproval) {
		Ok(s) => s,
		Err(r) => return r,
	};
	let kind = match body.kind.as_str() {
		"model" => ItemKind::Model,
		"mcpServer" => ItemKind::McpServer,
		"tool" => ItemKind::Tool,
		_ => {
			return json_error(
				StatusCode::BAD_REQUEST,
				"Choose tool, model, or mcpServer.",
			);
		},
	};
	let mut use_case = magpie::register::UseCase::example();
	use_case.name = body.name.trim().to_string();
	use_case.description = body.purpose.trim().to_string();
	use_case.owner_name = body.business_owner.trim().to_string();
	use_case.reference = format!("REQ-{}", magpie::new_id());
	let approval = Approval {
		id: magpie::new_id(),
		kind,
		name: body.name.trim().to_string(),
		business_owner: body.business_owner.trim().to_string(),
		purpose: body.purpose.trim().to_string(),
		classification: body.classification.trim().to_string(),
		expires_on: body.expires_on.trim().to_string(),
		status: ApprovalStatus::Pending,
		use_case,
	};
	if let Err(error) = magpie::approvals::validate(&approval) {
		return json_error(StatusCode::BAD_REQUEST, error.to_string());
	}
	let id = approval.id.clone();
	if save_revision(
		&state,
		"approval",
		&id,
		&session.email,
		serde_json::to_value(&approval).unwrap_or(json!({})),
		false,
	)
	.is_err()
	{
		return json_error(
			StatusCode::INTERNAL_SERVER_ERROR,
			"Could not save approval request.",
		);
	}
	let _ = record_audit(
		&state,
		"approval",
		&session.email,
		"",
		&approval.name,
		"",
		"allow",
		"queued",
	);
	Json(json!({ "id": id, "status": "pending" })).into_response()
}
