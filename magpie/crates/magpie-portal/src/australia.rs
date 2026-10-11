//! Current configuration and the organisation's remaining assurance work.
use super::*;

pub(super) async fn overview(State(state): State<AppState>, headers: HeaderMap) -> Response {
	let settings = load_settings(&state);
	let rules = load_rules(&state);
	let redact_count = rules
		.iter()
		.filter(|r| {
			matches!(
				r.action,
				dlp::Action::Redact
					| dlp::Action::Mask
					| dlp::Action::Tokenise
					| dlp::Action::Block
					| dlp::Action::RequireApproval
			)
		})
		.count();
	let posture = if settings.guardrail_mode == guardrail::Mode::LogOnly {
		"Trial — findings are logged; prompts and responses pass unchanged. No DLP redaction or residency blocking is being enforced."
	} else {
		"Enforcement — configured actions apply to traffic connected to Magpie guardrails."
	};
	let coverage = table(&["Identifier", "Detection and limits"], &[
        vec!["TFN".into(), "8/9-digit labelled candidates; Bower-compatible mod-11 heuristic for unlabelled candidates. Not ATO verification. Labelled candidates remain protected when a checksum fails.".into()],
        vec!["Medicare".into(), "10 digits plus optional individual reference number, first-digit range and published check digit. Detects unlabelled numbers; does not prove a card was issued.".into()],
        vec!["Individual Healthcare Identifier (IHI)".into(), "800360 prefix, 16 digits and Luhn check digit, following ADHA guidance. No HI Service lookup.".into()],
        vec!["Centrelink CRN, DVA, passport, driver licence".into(), "Context and format candidates; passport MRZ check digits where present. No entitlement or document validity claims.".into()],
        vec!["ABN / ACN".into(), "Published ABR/ASIC checksum filters. Public business identifiers can be allowed by policy.".into()],
        vec!["Banking, contact data and credentials".into(), "BSB, labelled account numbers, payment card candidates, Australian phone/address heuristics, email and supported secret patterns. Coverage is text-based, not image or attachment OCR.".into()],
        vec!["PSPF protective markings".into(), "Recognises markings and caveats in text/metadata. Australian destination checks and an approved PROTECTED destination list are product policy, not evidence of accreditation or personnel clearance.".into()],
    ]);
	let mapping = table(&["Framework", "Magpie contribution", "Organisation responsibility"], &[
        vec!["Privacy Act / APPs 1, 3, 6".into(), "Per-detector actions, approvals and metadata-only decision audit support minimisation and accountable use.".into(), "Privacy impact assessment, lawful purpose, collection notices, consent where required, and privacy policy. Detector coverage is not a legal determination.".into()],
        vec!["APP 8 — overseas disclosure".into(), "Australian region restrictions and destination approval can reduce unintended transfers when enforced.".into(), "Assess provider contracts, support access, subprocessors, training use and actual processing locations. An Australian region tag alone does not establish compliance.".into()],
        vec!["APPs 9 and 11 / TFN Rule".into(), "Identifier redaction or keyed pseudonyms, role restrictions and withheld raw values in decision findings.".into(), "Determine permitted handling of government identifiers; protect all storage, set justified retention, verify disposal and backups, and manage breach response. Pseudonyms can remain personal information.".into()],
        vec!["ISM 2159, 0585, 1959".into(), "Structured gateway decision records identify time, actor, model/tool, action and findings without copying prompt text.".into(), "Centralise required events, verify delivery and coverage, monitor incidents and evaluate the full system. Native MCP does not automatically call the separate Magpie MCP check API.".into()],
        vec!["ISM 1985, 1815, 1988".into(), "Audit permissions, application-level append-only records and a configurable retention setting.".into(), "Protect logs and backups from administrators, prove searchable retention and operating collection. A retention setting is not proof of 12 months of retained evidence.".into()],
        vec!["PSPF 2026 sections 9.2–9.6".into(), "Marking and caveat detection, routing restrictions, approval records and access groups.".into(), "Classify information, authorise the system for its classification, enforce need-to-know and caveats, assess cloud suppliers and apply handling rules. Magpie is not a cross-domain solution.".into()],
    ]);
	let body = format!(
		r#"<p>Australian privacy and protective security controls, with their current operating state and the evidence your organisation still needs.</p>
<p class="banner" role="status">{}</p>
<p><strong>{} of {} detector rules</strong> have a protective action configured. Sovereign mode: <strong>{}</strong>. Identity provider: <strong>{}</strong>.</p>
<p><a href="/dlp">Configure Australian privacy</a> · <a href="/settings">Enforcement and residency settings</a> · <a href="/audit">Review decision evidence</a></p>
<h2>Australian detection coverage</h2>{}
<h2>Control alignment</h2><p>These are interpretive mappings to support an assessment. They are not certification, legal advice or a claim that the deployment meets every requirement. PSPF obligations apply according to the entity and its contracts; they are not universal obligations for every Australian business.</p>{}
<h2>Trial and rollout</h2><ol><li>Use synthetic test cases to confirm detector coverage and false positives.</li><li>Run a log-only trial with an approved cohort and monitor would-action findings. Sensitive data still reaches the configured destination in this mode.</li><li>Set actions per detector, review approvals and configure access groups.</li><li>Switch to enforcement in Settings, then verify requests and responses at each connected route.</li><li>Retain test evidence and review privacy, hosting, access and incident procedures with the organisation's privacy and security owners.</li></ol>
<h2>Reference sources</h2><ul>
<li><a href="https://www.oaic.gov.au/privacy/australian-privacy-principles/read-the-australian-privacy-principles">OAIC — Australian Privacy Principles</a></li>
<li><a href="https://www.oaic.gov.au/privacy/privacy-guidance-for-organisations-and-government-agencies/handling-personal-information/the-privacy-tax-file-number-rule-2015-and-the-protection-of-tax-file-number-information">OAIC — Privacy (Tax File Number) Rule 2015</a></li>
<li><a href="https://www.cyber.gov.au/ism/2026-09/guidelines-for-security-assurance">ASD — ISM September 2026, security assurance</a></li>
<li><a href="https://www.protectivesecurity.gov.au/publications-library/pspf-annual-release-2026">PSPF Annual Release 2026</a> and <a href="https://www.stylemanual.gov.au/writing-and-designing-content/security-classifications-and-protective-markings">current protective markings guidance</a></li>
<li><a href="https://github.com/jusso-dev/Bower/blob/main/docs/privacy/privacy-secret-engine.md">Bower — privacy engine design reference</a></li></ul><p class="muted small">Sources reviewed 11 October 2026. No detector calls an external identity verification service.</p>"#,
		ui::esc(posture),
		redact_count,
		rules.len(),
		if settings.sovereign_mode { "on" } else { "off" },
		if state.inner.config.oidc_enabled() {
			"configured; MFA must be verified at the IdP"
		} else {
			"local administrator only; SSO/MFA not configured"
		},
		coverage,
		mapping
	);
	page(&state, &headers, "/australia", "Australian assurance", body)
}
