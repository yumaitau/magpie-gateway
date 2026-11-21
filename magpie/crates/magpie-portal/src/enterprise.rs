//! Enterprise Microsoft Identity (Entra OIDC + persistent SCIM) and live Government Evidence Pack APIs.
use super::*;
use chrono::{NaiveDate, TimeZone};
use sha2::{Digest, Sha256};

fn json_error(status: StatusCode, message: impl Into<String>) -> Response {
	(status, Json(json!({ "error": message.into() }))).into_response()
}

pub(super) fn routes() -> Router<AppState> {
	Router::new()
		.route("/api/magpie/identity", get(identity_overview))
		.route("/api/magpie/identity/scim-token", post(rotate_scim_token))
		.route("/api/magpie/identity/test-oidc", post(test_oidc_discovery))
		.route(
			"/api/magpie/identity/map-scim-group",
			post(map_scim_group),
		)
		.route("/api/magpie/evidence", get(evidence_overview))
		.route("/api/magpie/evidence.csv", get(evidence_csv))
		.route("/api/magpie/evidence.pdf", get(evidence_pdf))
		.route("/api/magpie/evidence-pack.json", get(evidence_pack_json))
}

pub(super) fn can_view_identity(state: &AppState, session: &SessionRecord) -> bool {
	access::allows(state, session, Permission::ManageAccess)
		|| access::allows(state, session, Permission::ReadSettings)
}

pub(super) fn can_view_evidence(state: &AppState, session: &SessionRecord) -> bool {
	access::allows(state, session, Permission::ReadAudit)
		|| access::allows(state, session, Permission::ExportAudit)
		|| access::allows(state, session, Permission::ReadPolicies)
}

fn public_base(state: &AppState, headers: &HeaderMap) -> String {
	if !state.inner.config.public_url.trim().is_empty() {
		return state
			.inner
			.config
			.public_url
			.trim()
			.trim_end_matches('/')
			.to_string();
	}
	let host = header_str(headers, "host").unwrap_or("localhost:8088");
	let scheme = if state.inner.config.cookie_secure {
		"https"
	} else {
		"http"
	};
	format!("{scheme}://{host}")
}

async fn identity_overview(State(state): State<AppState>, headers: HeaderMap) -> Response {
	let (session, _) = match session_from(&state, &headers) {
		Ok(s) => s,
		Err(r) => return r,
	};
	if !can_view_identity(&state, &session) {
		return json_error(
			StatusCode::FORBIDDEN,
			"Your role cannot view identity and directory settings.",
		);
	}
	let can_manage = access::allows(&state, &session, Permission::ManageAccess);
	let base = public_base(&state, &headers);
	let cfg = &state.inner.config;
	let resolved_issuer = if cfg.oidc_enabled() {
		issuer_for(cfg.oidc_kind, &cfg.issuer)
	} else {
		String::new()
	};
	let oidc_kind = match cfg.oidc_kind {
		OidcProviderKind::Entra => "entra",
		OidcProviderKind::Okta => "okta",
		OidcProviderKind::Google => "google",
		OidcProviderKind::Generic => "generic",
	};

	let (scim_token_source, scim_users, scim_groups, existing_groups, available_roles) = {
		let store = state.inner.store.lock().expect("db");
		let db_token = store.scim_token_override().ok().flatten();
		let source = if db_token.is_some() {
			"database"
		} else if !cfg.scim_token.is_empty() {
			"environment"
		} else {
			"none"
		};
		let dir = state.inner.directory.lock().expect("scim");
		let users: Vec<Value> = dir
			.users
			.iter()
			.map(|u| {
				json!({
					"id": u["id"],
					"userName": u["userName"],
					"displayName": u["displayName"],
					"externalId": u["externalId"],
					"active": u.get("active").and_then(Value::as_bool).unwrap_or(true),
				})
			})
			.collect();
		let groups: Vec<Value> = dir
			.groups
			.iter()
			.map(|g| {
				let members = g
					.get("members")
					.and_then(Value::as_array)
					.map(|a| a.len())
					.unwrap_or(0);
				json!({
					"id": g["id"],
					"displayName": g["displayName"],
					"externalId": g["externalId"],
					"memberCount": members,
				})
			})
			.collect();
		let access_ctrl = store.access_control().ok().flatten();
		let existing_groups: Vec<Value> = access_ctrl
			.as_ref()
			.map(|ac| {
				ac.groups
					.iter()
					.map(|g| {
						json!({
							"id": g.id,
							"name": g.name,
							"roles": g.roles,
							"identityGroups": g.identity_groups,
							"enforcement": g.enforcement,
						})
					})
					.collect()
			})
			.unwrap_or_default();
		let roles: Vec<Value> = access_ctrl
			.as_ref()
			.map(|ac| {
				ac.roles
					.iter()
					.map(|r| json!({ "id": r.id, "name": r.name }))
					.collect()
			})
			.unwrap_or_default();
		(source, users, groups, existing_groups, roles)
	};

	let app_roles = vec![
		json!({"value": "Magpie.Owner", "name": "Owner", "description": "Full gateway, governance and access administration"}),
		json!({"value": "Magpie.Admin", "name": "Admin", "description": "Day-to-day gateway, DLP, SIEM, and cohort administration"}),
		json!({"value": "Magpie.PolicyEditor", "name": "Policy Editor", "description": "Edit policies and DLP rules; submit approvals"}),
		json!({"value": "Magpie.Approver", "name": "Approver", "description": "Approve another person's staged policy or tool changes"}),
		json!({"value": "Magpie.Auditor", "name": "Auditor", "description": "Read-only audit log, controls, and compliance evidence exports"}),
		json!({"value": "Magpie.Viewer", "name": "Viewer", "description": "Read-only dashboard and approved catalogue"}),
	];

	Json(json!({
		"oidcEnabled": cfg.oidc_enabled(),
		"oidcKind": oidc_kind,
		"issuer": resolved_issuer,
		"clientId": cfg.client_id,
		"publicUrl": base,
		"redirectUri": format!("{base}/api/auth/callback"),
		"scimUrl": format!("{base}/scim/v2"),
		"scimConfigured": scim_token_source != "none",
		"scimTokenSource": scim_token_source,
		"scimUsers": scim_users,
		"scimGroups": scim_groups,
		"groups": existing_groups,
		"roles": available_roles,
		"appRoles": app_roles,
		"canManageAccess": can_manage,
	}))
	.into_response()
}

async fn rotate_scim_token(State(state): State<AppState>, headers: HeaderMap) -> Response {
	let session = match require(&state, &headers, Permission::ManageAccess) {
		Ok(s) => s,
		Err(r) => return r,
	};
	let token = format!(
		"mp_scim_{}",
		identity::start_pkce().verifier
	);
	if state
		.inner
		.store
		.lock()
		.expect("db")
		.set_scim_token_override(&token, &session.email)
		.is_err()
	{
		return json_error(
			StatusCode::INTERNAL_SERVER_ERROR,
			"Could not rotate SCIM bearer token.",
		);
	}
	Json(json!({
		"token": token,
		"note": "Copy this SCIM secret token into Microsoft Entra ID Enterprise Application → Provisioning. It will not be shown again."
	}))
	.into_response()
}

async fn test_oidc_discovery(State(state): State<AppState>, headers: HeaderMap) -> Response {
	let (session, _) = match session_from(&state, &headers) {
		Ok(s) => s,
		Err(r) => return r,
	};
	if !can_view_identity(&state, &session) {
		return json_error(
			StatusCode::FORBIDDEN,
			"Your role cannot test identity configuration.",
		);
	}
	if !state.inner.config.oidc_enabled() {
		return json_error(
			StatusCode::BAD_REQUEST,
			"OIDC / Microsoft Entra ID is not configured in deployment environment variables.",
		);
	}
	let issuer = issuer_for(state.inner.config.oidc_kind, &state.inner.config.issuer);
	let url = format!(
		"{}/.well-known/openid-configuration",
		issuer.trim_end_matches('/')
	);
	let resp = match state
		.inner
		.gateway_client
		.get(&url)
		.timeout(Duration::from_secs(10))
		.send()
		.await
	{
		Ok(r) => r,
		Err(_) => {
			return Json(json!({
				"ok": false,
				"url": url,
				"error": "Could not reach the identity provider discovery endpoint. Check outbound HTTPS / firewall rules."
			}))
			.into_response();
		},
	};
	if !resp.status().is_success() {
		return Json(json!({
			"ok": false,
			"url": url,
			"error": format!("Discovery endpoint returned HTTP {}.", resp.status().as_u16())
		}))
		.into_response();
	}
	match resp.json::<Discovery>().await {
		Ok(disc) => Json(json!({
			"ok": true,
			"url": url,
			"issuer": disc.issuer,
			"authorizationEndpoint": disc.authorization_endpoint,
			"tokenEndpoint": disc.token_endpoint,
			"jwksUri": disc.jwks_uri,
		}))
		.into_response(),
		Err(_) => Json(json!({
			"ok": false,
			"url": url,
			"error": "Discovery response was not valid OpenID Connect metadata."
		}))
		.into_response(),
	}
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct MapScimGroupBody {
	scim_group_id: String,
	role_id: String,
	enforcement: String,
}

async fn map_scim_group(
	State(state): State<AppState>,
	headers: HeaderMap,
	Json(body): Json<MapScimGroupBody>,
) -> Response {
	let session = match require(&state, &headers, Permission::ManageAccess) {
		Ok(s) => s,
		Err(r) => return r,
	};
	let enforcement: magpie::rbac::GroupEnforcement = match body.enforcement.parse() {
		Ok(e) => e,
		Err(e) => return json_error(StatusCode::BAD_REQUEST, e.to_string()),
	};
	let scim_group = {
		let dir = state.inner.directory.lock().expect("scim");
		dir.get_group(&body.scim_group_id).cloned()
	};
	let Some(scim_group) = scim_group else {
		return json_error(StatusCode::NOT_FOUND, "Synced SCIM group not found.");
	};
	let display = scim_group
		.get("displayName")
		.and_then(Value::as_str)
		.unwrap_or("Entra group")
		.trim()
		.to_string();
	let external_id = scim_group
		.get("externalId")
		.and_then(Value::as_str)
		.unwrap_or("")
		.trim()
		.to_string();
	let mut claim_ids = Vec::new();
	if !external_id.is_empty() {
		claim_ids.push(external_id);
	}
	if !display.is_empty() && !claim_ids.contains(&display) {
		claim_ids.push(display.clone());
	}
	let slug: String = display
		.to_ascii_lowercase()
		.chars()
		.map(|c| if c.is_ascii_alphanumeric() { c } else { '-' })
		.collect::<String>()
		.trim_matches('-')
		.chars()
		.take(48)
		.collect();
	let group_id = if slug.is_empty() {
		format!("entra-{}", &body.scim_group_id[..body.scim_group_id.len().min(8)])
	} else {
		format!("entra-{slug}")
	};

	let store = state.inner.store.lock().expect("db");
	let result = (|| -> magpie::Result<Value> {
		let mut access = store
			.access_control()?
			.ok_or_else(|| magpie::Error::Msg("Access controls unavailable.".into()))?;
		if !access.roles.iter().any(|r| r.id == body.role_id) {
			return Err(magpie::Error::Msg("Choose an existing role.".into()));
		}
		let can_enforce = access.allows(&session, Permission::ChangeEnforcement);
		if let Some(existing) = access.groups.iter_mut().find(|g| g.id == group_id) {
			if !existing.roles.contains(&body.role_id) {
				existing.roles.push(body.role_id.clone());
			}
			for cid in &claim_ids {
				if !existing.identity_groups.contains(cid) {
					existing.identity_groups.push(cid.clone());
				}
			}
			if can_enforce {
				existing.enforcement = enforcement;
			}
		} else {
			let eff_enforcement = if can_enforce {
				enforcement
			} else {
				magpie::rbac::GroupEnforcement::Follow
			};
			access.groups.push(magpie::rbac::UserGroup {
				id: group_id.clone(),
				name: display.clone(),
				roles: vec![body.role_id.clone()],
				members: vec![],
				identity_groups: claim_ids.clone(),
				traffic_names: vec![],
				enforcement: eff_enforcement,
			});
		}
		store.set_access_control(&access, &session.email)?;
		Ok(json!({ "id": group_id, "name": display, "identityGroups": claim_ids }))
	})();
	match result {
		Ok(v) => Json(v).into_response(),
		Err(e) => json_error(StatusCode::BAD_REQUEST, e.to_string()),
	}
}

#[derive(Deserialize)]
struct EvidenceQuery {
	#[serde(default)]
	from: String,
	#[serde(default)]
	to: String,
}

type ParsedRange = (
	Option<chrono::DateTime<Utc>>,
	Option<chrono::DateTime<Utc>>,
	String,
);

fn parse_range(q: &EvidenceQuery) -> Result<ParsedRange, String> {
	let from = if q.from.trim().is_empty() {
		None
	} else {
		let d = NaiveDate::parse_from_str(q.from.trim(), "%Y-%m-%d")
			.map_err(|_| "From date must use YYYY-MM-DD format.".to_string())?;
		Some(Utc.from_utc_datetime(&d.and_hms_opt(0, 0, 0).unwrap()))
	};
	let to = if q.to.trim().is_empty() {
		None
	} else {
		let d = NaiveDate::parse_from_str(q.to.trim(), "%Y-%m-%d")
			.map_err(|_| "To date must use YYYY-MM-DD format.".to_string())?;
		Some(Utc.from_utc_datetime(&d.and_hms_opt(23, 59, 59).unwrap()))
	};
	if let (Some(a), Some(b)) = (from, to)
		&& a > b
	{
		return Err("From date must be on or before To date.".into());
	}
	let label = match (q.from.trim(), q.to.trim()) {
		("", "") => "All recorded history".into(),
		(a, "") => format!("From {a}"),
		("", b) => format!("Up to {b}"),
		(a, b) => format!("{a} to {b}"),
	};
	Ok((from, to, label))
}

async fn evidence_overview(
	State(state): State<AppState>,
	headers: HeaderMap,
	Query(query): Query<EvidenceQuery>,
) -> Response {
	let (session, _) = match session_from(&state, &headers) {
		Ok(s) => s,
		Err(r) => return r,
	};
	if !can_view_evidence(&state, &session) {
		return json_error(
			StatusCode::FORBIDDEN,
			"Your role cannot view compliance evidence.",
		);
	}
	let (from, to, period_label) = match parse_range(&query) {
		Ok(r) => r,
		Err(e) => return json_error(StatusCode::BAD_REQUEST, e),
	};
	let settings = load_settings(&state);
	let rules = load_rules(&state);
	let stats = match state.inner.store.lock().expect("db").monthly_stats(from, to) {
		Ok(s) => s,
		Err(_) => {
			return json_error(
				StatusCode::INTERNAL_SERVER_ERROR,
				"Could not aggregate audit events.",
			);
		},
	};
	let rows = magpie::evidence::monthly_rows(&stats);
	let can_export = access::allows(&state, &session, Permission::ExportAudit);
	let can_export_policy = access::allows(&state, &session, Permission::ExportPolicy);
	Json(json!({
		"periodLabel": period_label,
		"from": query.from,
		"to": query.to,
		"generatedAt": Utc::now().to_rfc3339(),
		"stats": {
			"requests": stats.requests,
			"blocks": stats.blocks,
			"dlpHits": stats.dlp_hits,
			"policyChanges": stats.policy_changes,
			"logins": stats.logins,
			"approvals": stats.approvals,
			"sovereignDenies": stats.sovereign_denies,
		},
		"posture": {
			"mode": settings.guardrail_mode,
			"sovereignMode": settings.sovereign_mode,
			"requireApproval": settings.require_approval,
			"retentionDays": settings.retention_days,
			"oidcEnabled": state.inner.config.oidc_enabled(),
			"enabledDetectors": rules.iter().filter(|r| r.enabled).count(),
			"totalDetectors": rules.len(),
		},
		"rows": rows,
		"canExport": can_export,
		"canExportPolicy": can_export_policy,
	}))
	.into_response()
}

async fn evidence_csv(
	State(state): State<AppState>,
	headers: HeaderMap,
	Query(query): Query<EvidenceQuery>,
) -> Response {
	let session = match require(&state, &headers, Permission::ExportAudit) {
		Ok(s) => s,
		Err(r) => return r,
	};
	let (from, to, period_label) = match parse_range(&query) {
		Ok(r) => r,
		Err(e) => return json_error(StatusCode::BAD_REQUEST, e),
	};
	let stats = match state.inner.store.lock().expect("db").monthly_stats(from, to) {
		Ok(s) => s,
		Err(_) => return json_error(StatusCode::INTERNAL_SERVER_ERROR, "Could not aggregate audit events."),
	};
	let rows = magpie::evidence::monthly_rows(&stats);
	let _ = record_audit(
		&state,
		"audit",
		&session.email,
		"",
		"",
		"",
		"allow",
		&format!("Exported compliance evidence CSV ({period_label})"),
	);
	let filename = format!("magpie-evidence-{}.csv", Utc::now().format("%Y-%m-%d"));
	(
		[
			(header::CONTENT_TYPE, "text/csv; charset=utf-8".to_string()),
			(
				header::CONTENT_DISPOSITION,
				format!("attachment; filename=\"{filename}\""),
			),
		],
		magpie::evidence::to_csv(&rows),
	)
		.into_response()
}

async fn evidence_pdf(
	State(state): State<AppState>,
	headers: HeaderMap,
	Query(query): Query<EvidenceQuery>,
) -> Response {
	let session = match require(&state, &headers, Permission::ExportAudit) {
		Ok(s) => s,
		Err(r) => return r,
	};
	let (from, to, period_label) = match parse_range(&query) {
		Ok(r) => r,
		Err(e) => return json_error(StatusCode::BAD_REQUEST, e),
	};
	let stats = match state.inner.store.lock().expect("db").monthly_stats(from, to) {
		Ok(s) => s,
		Err(_) => return json_error(StatusCode::INTERNAL_SERVER_ERROR, "Could not aggregate audit events."),
	};
	let rows = magpie::evidence::monthly_rows(&stats);
	let _ = record_audit(
		&state,
		"audit",
		&session.email,
		"",
		"",
		"",
		"allow",
		&format!("Exported compliance evidence PDF ({period_label})"),
	);
	let title = format!("Magpie Gateway Compliance Evidence Report ({period_label})");
	let filename = format!("magpie-evidence-{}.pdf", Utc::now().format("%Y-%m-%d"));
	(
		[
			(header::CONTENT_TYPE, "application/pdf".to_string()),
			(
				header::CONTENT_DISPOSITION,
				format!("attachment; filename=\"{filename}\""),
			),
		],
		magpie::evidence::to_pdf(&title, &rows),
	)
		.into_response()
}

async fn evidence_pack_json(
	State(state): State<AppState>,
	headers: HeaderMap,
	Query(query): Query<EvidenceQuery>,
) -> Response {
	let session = match require(&state, &headers, Permission::ExportAudit) {
		Ok(s) => s,
		Err(r) => return r,
	};
	let (from, to, period_label) = match parse_range(&query) {
		Ok(r) => r,
		Err(e) => return json_error(StatusCode::BAD_REQUEST, e),
	};
	let settings = load_settings(&state);
	let rules = load_rules(&state);
	let (stats, approvals) = {
		let store = state.inner.store.lock().expect("db");
		let stats = match store.monthly_stats(from, to) {
			Ok(s) => s,
			Err(_) => {
				return json_error(
					StatusCode::INTERNAL_SERVER_ERROR,
					"Could not aggregate audit events.",
				);
			},
		};
		let approvals: Vec<UseCase> = store
			.list_documents("approval")
			.unwrap_or_default()
			.into_iter()
			.filter(|d| d.active)
			.filter_map(|d| serde_json::from_value::<Approval>(d.body).ok())
			.map(|a| a.use_case)
			.collect();
		(stats, approvals)
	};
	let rows = magpie::evidence::monthly_rows(&stats);
	let payload = json!({
		"schema": "magpie.assurance-pack.v1",
		"generatedAt": Utc::now().to_rfc3339(),
		"generatedBy": session.email,
		"period": { "label": period_label, "from": query.from, "to": query.to },
		"disclaimer": "Every framework mapping is labelled direct or interpretive. A gateway alone does not satisfy ISM, Essential Eight, or agency accountability obligations.",
		"posture": {
			"guardrailMode": settings.guardrail_mode,
			"sovereignMode": settings.sovereign_mode,
			"telemetryEnabled": settings.telemetry_enabled,
			"retentionDays": settings.retention_days,
			"requireApproval": settings.require_approval,
			"oidcEnabled": state.inner.config.oidc_enabled(),
			"approvedProtected": settings.approved_protected,
			"dlpRules": rules.iter().map(|r| json!({
				"id": r.id,
				"kind": r.kind,
				"enabled": r.enabled,
				"action": r.action,
				"exceptionCount": r.exceptions.len(),
			})).collect::<Vec<_>>(),
		},
		"stats": {
			"requests": stats.requests,
			"blocks": stats.blocks,
			"dlpHits": stats.dlp_hits,
			"policyChanges": stats.policy_changes,
			"logins": stats.logins,
			"approvals": stats.approvals,
			"sovereignDenies": stats.sovereign_denies,
		},
		"evidenceRows": rows,
		"useCaseRegister": approvals,
	});
	let canonical = serde_json::to_string_pretty(&payload).unwrap_or_default();
	let mut hasher = Sha256::new();
	hasher.update(canonical.as_bytes());
	let sha256 = hasher
		.finalize()
		.iter()
		.map(|b| format!("{b:02x}"))
		.collect::<String>();
	let envelope = json!({
		"sha256": sha256,
		"pack": payload,
	});
	let _ = record_audit(
		&state,
		"audit",
		&session.email,
		"",
		"",
		"",
		"allow",
		&format!("Exported signed assurance bundle ({period_label})"),
	);
	let filename = format!("magpie-assurance-pack-{}.json", Utc::now().format("%Y-%m-%d"));
	(
		[
			(header::CONTENT_TYPE, "application/json; charset=utf-8".to_string()),
			(
				header::CONTENT_DISPOSITION,
				format!("attachment; filename=\"{filename}\""),
			),
		],
		serde_json::to_string_pretty(&envelope).unwrap_or_default(),
	)
		.into_response()
}
