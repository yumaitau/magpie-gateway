//! An actionable inventory of gateway controls, not a compliance score.
use super::*;

pub(super) async fn overview(State(state): State<AppState>, headers: HeaderMap) -> Response {
	let settings = load_settings(&state);
	let rules = load_rules(&state);
	let observing = settings.guardrail_mode == guardrail::Mode::LogOnly;
	let mode = if observing {
		"Observe only"
	} else {
		"Enforcing"
	};
	let effect = if observing {
		"Observe only — does not block or alter traffic"
	} else {
		"Enforced on connected routes"
	};
	let enabled = rules.iter().filter(|r| r.enabled).count();
	let alert_status = match state.inner.store.lock().expect("db").alert_destinations() {
		Ok(destinations) => format!(
			"{} of {} destinations enabled",
			destinations.iter().filter(|d| d.enabled).count(),
			destinations.len()
		),
		Err(_) => "Status unavailable".into(),
	};
	let link = |path: &str, label: &str| -> String {
		if require(&state, &headers, access::page_permission(path)).is_ok() {
			format!(r#"<a href="{}">{}</a>"#, ui::esc(path), ui::esc(label))
		} else {
			format!("{} — ask an administrator", ui::esc(label))
		}
	};
	let edit_privacy = link("/dlp", "Edit, disable or restore detectors");
	let edit_settings = link("/settings", "Change enforcement or settings");
	let access = link("/access", "Edit roles and group permissions");
	let audit = link("/audit", "View decision records");
	let alerts = link("/alerts", "Manage alert destinations");
	let approvals = link("/approvals", "Manage model and tool approvals");
	let row = |name: &str, status: &str, detail: &str, action: &str| {
		format!(
			r#"<tr><th scope="row">{}</th><td>{}</td><td>{}<p>{}</p></td></tr>"#,
			ui::esc(name),
			ui::esc(status),
			ui::esc(detail),
			action
		)
	};
	let privacy_rows = [
        row("Personal information and secrets · APPs 3, 9, 11", &format!("{enabled}/{} detectors enabled · {mode}", rules.len()), "Set actions for TFNs, Medicare, contact details, credentials and other text detectors. Off produces no findings for that detector; allow still records findings.", &edit_privacy),
        row("Australian destination restriction · APP 8", if settings.sovereign_mode { effect } else { "Off" }, "Sovereign mode restricts destinations using region tags. Turn it off in Settings to remove the blanket Australian-region restriction. Classification restrictions remain separate. Provider contracts and actual processing locations still need assessment.", &edit_settings),
        row("Protective markings · PSPF", effect, "Built-in: OFFICIAL: Sensitive and above require an Australian destination; PROTECTED also requires an approved destination; SECRET and TOP SECRET are refused. Cannot disable individually in the portal. Log-only observes these decisions. Changing a marking detector does not remove routing restrictions.", &edit_settings),
        row("Additional Presidio analysis", if !dlp::detector_enabled(&rules, dlp::HitKind::Presidio) { "Off" } else if settings.presidio_analyzer_url.is_empty() { "Not configured" } else { "Configured — availability not verified" }, "When enabled and configured, text is sent to your Presidio analyser. Disable the Presidio detector to stop these requests, or clear the analyser URL in Settings. Local detectors continue independently.", &edit_privacy),
    ].join("");
	let ism_rows = [
        row("Access to administration and logs", "Always active", "Portal permissions remain enforced in log-only mode. An administrator can add or remove permissions in roles and groups. Authentication has no off switch here.", &access),
        row("Model and tool approval checks", effect, "Named models and tools checked through Magpie require a current approval. Review their approvals and expiry dates. Native MCP routes must explicitly call the Magpie check API to receive this coverage.", &approvals),
        row("Prompt injection screening", effect, "Built-in pattern checks run on responses and tool calls passing through Magpie. There is no per-pattern disable switch in the portal; gateway mode controls whether findings block traffic.", &edit_settings),
        row("Gateway decision audit", "Built-in", "Magpie records decisions without raw prompt text. Records are append-only through the application; there is no disable or delete control here. This does not establish coverage of every route or protect storage from infrastructure administrators.", &audit),
        row("Alert delivery", &alert_status, "Slack, Teams, webhook and email destinations are optional. Add, disable or delete a destination in Alerts. Delivery of selected alerts is not a complete central audit export.", &alerts),
        row("Audit retention", &format!("{} days configured · storage verification required", settings.retention_days), "This setting records a retention target; automatic deletion is not implemented. Verify retained history, backups and disposal in your storage operations.", &edit_settings),
        row("Approval of policy changes", if settings.require_approval { "Second person required" } else { "Changes activate immediately" }, "The active revision stays in use while a change is pending. A different authorised person must approve pending revisions on Policies or Australian privacy. This requirement has no switch in the portal.", &link("/policies", "Review pending policy changes")),
    ].join("");
	let e8_rows = [
        row("Restrict administrative privileges", "Partial support — portal only", "Magpie roles restrict portal actions. Privileged accounts, operating environments and organisation-wide access reviews are managed outside Magpie.", &access),
        row("Multi-factor authentication", if state.inner.config.oidc_enabled() { "Identity provider configured · MFA unverified" } else { "No identity provider configured" }, "MFA is not enforced by the local password login. Configure OIDC in deployment settings and require MFA at your identity provider; verify local fallback access separately.", "Deployment / identity administrator"),
        row("Application control", "Outside Magpie", "Model and tool approvals do not control workstation executables.", "Manage in your endpoint application-control policy."),
        row("Patch applications", "Outside Magpie", "Application inventory, vulnerability scanning and patch rollout are not managed here.", "Manage in your application and deployment tooling."),
        row("Restrict Microsoft Office macros", "Outside Magpie", "Magpie does not configure Office macros.", "Manage in your Office / device policies."),
        row("User application hardening", "Outside Magpie", "Browser and application hardening are not configured here.", "Manage in your endpoint security policies."),
        row("Patch operating systems", "Outside Magpie", "Host and device patching are not managed here.", "Manage in your infrastructure and device tooling."),
        row("Regular backups", "Outside Magpie", "A database or retention target is not evidence of tested backups.", "Configure backups and test recovery in your storage platform."),
    ].join("");
	let inventory = |rows: &str| {
		format!(
			r#"<div class="table-scroll" tabindex="0" role="region" aria-label="Controls table"><table><thead><tr><th scope="col">Control</th><th scope="col">Current state</th><th scope="col">What it does / how to change it</th></tr></thead><tbody>{rows}</tbody></table></div>"#
		)
	};
	let body = format!(
		r#"<p>See what is in play and where to change it. Framework names describe alignment; they are not switches or a compliance score.</p>
<h2>Gateway mode: {mode}</h2><p>{effect}. Coverage applies only to traffic connected to Magpie guardrails. Login and portal permissions are always enforced.</p><p>{edit_settings}</p>
<h2>Privacy and information handling</h2><p><a href="/ui/dlp-tuning">Tune false positives</a> · <a href="/ui/help/dlp">DLP tuning guide</a></p>{privacy}
<h2>ISM — Information Security Manual</h2><p>These gateway capabilities support parts of an ISM assessment. They do not implement the whole manual.</p>{ism}
<h2>Essential Eight</h2><p>Portal roles and identity integration provide limited support. The remaining strategies require controls in your devices, applications and infrastructure. No maturity level is assessed here.</p>
<details><summary>See all eight strategies and their owners</summary>{e8}</details>
<details><summary>Framework references and detailed coverage</summary><p>Mappings are guidance for your assessment. Changing a product setting does not remove an applicable requirement.</p><ul><li><a href="/australia">Australian assurance and detector coverage</a></li><li><a href="https://www.cyber.gov.au/ism/2026-09/guidelines-for-security-assurance">ASD ISM — security assurance</a></li><li><a href="https://www.cyber.gov.au/publication/essential-eight-maturity-model">ASD Essential Eight maturity model</a></li><li><a href="https://www.oaic.gov.au/privacy/australian-privacy-principles/australian-privacy-principles-quick-reference">OAIC Australian Privacy Principles</a></li></ul></details>"#,
		privacy = inventory(&privacy_rows),
		ism = inventory(&ism_rows),
		e8 = inventory(&e8_rows)
	);
	page(&state, &headers, "/controls", "Controls", body)
}
