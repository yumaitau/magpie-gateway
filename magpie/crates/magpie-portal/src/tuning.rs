//! Preview and narrowly scoped edits for DLP false positives.
use super::*;

fn error(status: StatusCode, message: &str) -> Response {
	(status, Json(json!({"error": message}))).into_response()
}

pub(super) async fn get(State(state): State<AppState>, headers: HeaderMap) -> Response {
	let session = match require(&state, &headers, Permission::ReadPolicies) {
		Ok(s) => s,
		Err(r) => return r,
	};
	let can_edit = access::allows(&state, &session, Permission::EditDlp);
	let can_audit = access::allows(&state, &session, Permission::ReadAudit);
	let settings = load_settings(&state);
	let store = state.inner.store.lock().expect("db");
	let pending = match store.list_pending_revisions() {
		Ok(p) => p,
		Err(_) => {
			return error(
				StatusCode::INTERNAL_SERVER_ERROR,
				"Could not load pending changes.",
			);
		},
	};
	let documents = match store.list_documents("dlp_rule") {
		Ok(d) => d,
		Err(_) => {
			return error(
				StatusCode::INTERNAL_SERVER_ERROR,
				"Could not load rule revisions.",
			);
		},
	};
	let rules = rules_from_documents(&documents);
	let rules: Vec<_> = rules.iter().map(|r| json!({
        "kind":r.kind, "name":dlp::kind_name(r.kind), "action":r.action, "enabled":r.enabled,
        "revision":documents.iter().find(|d|d.id==r.id).map(|d|d.revision).unwrap_or(0),
        "exceptions":r.exceptions.iter().map(|e|json!({"id":e.id,"label":e.label})).collect::<Vec<_>>(),
        "pending":pending.iter().any(|p| p.kind == "dlp_rule" && p.doc_id == r.id)
    })).collect();
	let mut recent = std::collections::BTreeMap::<String, usize>::new();
	if can_audit {
		let events = match store.search_audit(&AuditQuery {
			limit: 100,
			kind: Some("decision".into()),
			..Default::default()
		}) {
			Ok(e) => e,
			Err(_) => {
				return error(
					StatusCode::INTERNAL_SERVER_ERROR,
					"Could not load recent findings.",
				);
			},
		};
		for event in events {
			if let Ok(hits) = serde_json::from_value::<Vec<dlp::PublicHit>>(event.detail["hits"].clone())
			{
				for hit in hits {
					*recent
						.entry(
							serde_json::to_value(hit.kind)
								.unwrap()
								.as_str()
								.unwrap()
								.into(),
						)
						.or_default() += 1;
				}
			}
		}
	}
	Json(json!({"rules":rules,"mode":settings.guardrail_mode,"requiresApproval":settings.require_approval,"canEdit":can_edit,"canAudit":can_audit,"recent":recent})).into_response()
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub(super) struct Draft {
	kind: dlp::HitKind,
	expected_revision: i64,
	action: dlp::Action,
	enabled: bool,
	#[serde(default)]
	exact_value: String,
	#[serde(default)]
	exception_label: String,
	#[serde(default)]
	remove_exceptions: Vec<String>,
	#[serde(default)]
	text: String,
}

#[allow(clippy::result_large_err)]
fn apply_draft(state: &AppState, mut rule: DlpRule, draft: &Draft) -> Result<DlpRule, Response> {
	if draft.text.len() > 16_384 {
		return Err(error(
			StatusCode::BAD_REQUEST,
			"Use a sample of at most 16 KB.",
		));
	}
	if draft.exact_value.len() > 512
		|| draft.exception_label.len() > 80
		|| draft.remove_exceptions.len() > 32
	{
		return Err(error(
			StatusCode::BAD_REQUEST,
			"An exception value can be at most 512 bytes, its label 80 bytes, and a detector can have at most 32 exceptions.",
		));
	}
	if draft
		.remove_exceptions
		.iter()
		.any(|id| !rule.exceptions.iter().any(|e| &e.id == id))
	{
		return Err(error(
			StatusCode::CONFLICT,
			"An exception changed. Reload the detector and try again.",
		));
	}
	rule.action = draft.action;
	rule.enabled = draft.enabled;
	rule
		.exceptions
		.retain(|e| !draft.remove_exceptions.contains(&e.id));
	if !draft.exact_value.is_empty() {
		if draft.exception_label.trim().is_empty() || draft.exception_label.contains(&draft.exact_value)
		{
			return Err(error(
				StatusCode::BAD_REQUEST,
				"Give the exception a short label without the original value.",
			));
		}
		let Some(fingerprint) = dlp::exception_fingerprint(
			rule.kind,
			&draft.exact_value,
			state.inner.config.session_secret.as_bytes(),
		) else {
			return Err(error(
				StatusCode::BAD_REQUEST,
				"Exact exceptions are not supported for protective markings or caveats.",
			));
		};
		if !rule.exceptions.iter().any(|e| e.fingerprint == fingerprint) {
			if rule.exceptions.len() >= 32 {
				return Err(error(
					StatusCode::BAD_REQUEST,
					"Remove an existing exception before adding another; the limit is 32 per detector.",
				));
			}
			rule.exceptions.push(dlp::DlpException {
				id: magpie::new_id(),
				label: draft.exception_label.trim().into(),
				fingerprint,
			});
		}
	}
	Ok(rule)
}

pub(super) async fn preview(
	State(state): State<AppState>,
	headers: HeaderMap,
	Json(draft): Json<Draft>,
) -> Response {
	if let Err(r) = require(&state, &headers, Permission::EditDlp) {
		return r;
	}
	let mut rules = load_rules(&state);
	let Some(index) = rules.iter().position(|r| r.kind == draft.kind) else {
		return error(StatusCode::BAD_REQUEST, "Unknown detector.");
	};
	let rule = match apply_draft(&state, rules[index].clone(), &draft) {
		Ok(r) => r,
		Err(e) => return e,
	};
	if draft.text.trim().is_empty() {
		return error(
			StatusCode::BAD_REQUEST,
			"Enter a synthetic sample to compare.",
		);
	}
	let key = state.inner.config.session_secret.as_bytes();
	let active = dlp::inspect(&draft.text, &Default::default(), &rules, &[], key);
	rules[index] = rule;
	let proposed = dlp::inspect(&draft.text, &Default::default(), &rules, &[], key);
	// A draft allow/exception must never echo a sensitive original value in the response.
	let safe = dlp::inspect(
		&draft.text,
		&Default::default(),
		&dlp::default_rules(),
		&[],
		key,
	);
	Json(json!({"active":{"action":active.action,"hits":active.hits},"draft":{"action":proposed.action,"hits":proposed.hits},"sanitized":safe.output,"mode":load_settings(&state).guardrail_mode})).into_response()
}

pub(super) async fn save(
	State(state): State<AppState>,
	headers: HeaderMap,
	Json(draft): Json<Draft>,
) -> Response {
	let session = match require(&state, &headers, Permission::EditDlp) {
		Ok(s) => s,
		Err(r) => return r,
	};
	let Some(before) = load_rules(&state)
		.into_iter()
		.find(|r| r.kind == draft.kind)
	else {
		return error(StatusCode::BAD_REQUEST, "Unknown detector.");
	};
	let after = match apply_draft(&state, before.clone(), &draft) {
		Ok(r) => r,
		Err(e) => return e,
	};
	if before == after {
		return error(StatusCode::BAD_REQUEST, "There are no changes to save.");
	}
	let approval = second_person(&state);
	let store = state.inner.store.lock().expect("db");
	let pending = match store.list_pending_revisions() {
		Ok(p) => p,
		Err(_) => {
			return error(
				StatusCode::INTERNAL_SERVER_ERROR,
				"Could not check pending changes.",
			);
		},
	};
	if pending
		.iter()
		.any(|p| p.kind == "dlp_rule" && p.doc_id == before.id)
	{
		return error(
			StatusCode::CONFLICT,
			"This detector already has a pending change. Have it reviewed before saving another change.",
		);
	}
	let existing = match store.get_document("dlp_rule", &before.id) {
		Ok(d) => d,
		Err(_) => {
			return error(
				StatusCode::INTERNAL_SERVER_ERROR,
				"Could not load the active rule.",
			);
		},
	};
	if existing.as_ref().map(|d| d.revision).unwrap_or(0) != draft.expected_revision {
		return error(
			StatusCode::CONFLICT,
			"The active rule changed. Reload it before saving.",
		);
	}
	if existing.as_ref().is_some_and(|d| {
		serde_json::from_value::<DlpRule>(d.body.clone())
			.ok()
			.as_ref()
			!= Some(&before)
	}) {
		return error(
			StatusCode::CONFLICT,
			"The active rule changed. Reload it and try again.",
		);
	}
	match save_revision_in_store(
		&store,
		"dlp_rule",
		&after.id,
		&session.email,
		json!(after),
		approval,
	) {
		Ok(status) => {
			let pending = status == magpie::change::RevisionStatus::Pending;
			let _ = store.append_audit(&AuditEvent::new(
				"policy",
				&session.email,
				"",
				"",
				"",
				"log",
				&format!(
					"DLP {} tuning {}",
					dlp::kind_name(after.kind),
					if pending {
						"pending approval"
					} else {
						"active"
					}
				),
				json!({}),
			));
			Json(json!({"pending":pending})).into_response()
		},
		Err(_) => error(
			StatusCode::INTERNAL_SERVER_ERROR,
			"Could not save the change.",
		),
	}
}
