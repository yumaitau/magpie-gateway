# Australian privacy and assurance

Magpie uses the native Agentgateway console for connections, providers, models,
MCP, traffic, keys, logs and playgrounds. **Controls** in the main navigation
shows effective mode, privacy controls, ISM support and all eight Essential Eight
strategies, with links to their settings or an external owner. It distinguishes
built-in controls, observe-only rules and controls outside Magpie.
**Magpie Governance → Australian privacy**
controls local detector actions. **Australian assurance** shows current trial /
enforcement status and interprets the product's contribution to Australian
privacy and protective security requirements. These are assessment aids, not
certification or legal determinations.

## Bower review

Reviewed [Bower](https://github.com/jusso-dev/Bower), particularly
[`AustralianIdentifierDetectors.cs`](https://github.com/jusso-dev/Bower/blob/main/src/Bower.Redaction/Detectors/Australian/AustralianIdentifierDetectors.cs),
[`ChecksumAlgorithms.cs`](https://github.com/jusso-dev/Bower/blob/main/src/Bower.Redaction/Validation/ChecksumAlgorithms.cs)
and its [privacy engine design](https://github.com/jusso-dev/Bower/blob/main/docs/privacy/privacy-secret-engine.md),
on 11 October 2026. The adopted ideas are deterministic local recognition,
per-detector actions, stronger checks where documented, no original value in
finding metadata, and a separate assurance explanation. Magpie does not embed
the Bower service or claim parity with every Bower detector/transport.

TFN candidate filtering uses the same mod-11 weights as Bower for unlabelled
8/9-digit candidates, labelled **heuristic**, not authoritative ATO validation.
Labelled TFNs remain detectable when this filter fails. Repeated placeholder
numbers are excluded from unlabelled matching. An 8-digit input is treated as
leading-zero for this candidate filter. This supersedes the original plan's
choice to use labelled TFNs only. It does not claim that the ATO publishes or
endorses this algorithm, or that a number belongs to a real person.

Medicare uses the existing ADHA check-digit algorithm and now detects unlabelled
candidates. IHI uses the 800360 prefix and Luhn validation described in
[ADHA's IHI test cases](https://implementer.digitalhealth.gov.au/resources/hi-service-ihi-lookup-test-cases).
Neither proves issuance or entitlement. DVA uses labelled candidate matching;
CRN, licences and printed passports remain context/format based. ABN/ACN and
passport MRZ use the existing published checksum filters. Email and Luhn payment
card candidates supplement the Australian phone, address, banking and secret
patterns. Overlapping matches retain every finding; the strongest action wins,
and overlapping transformations redact the union instead of leaving a suffix.

Rules support allow, log, redact, mask (last two characters), keyed pseudonyms,
block and require approval. HMAC pseudonyms use a 128-bit digest prefix and a
server-held key; they remain potentially linkable personal information. No
unkeyed TFN hash option is offered. Saving a detector rule requires `editDlp` and
uses the existing second-person approval setting. Unedited and newly added
detectors retain their defaults. **Disable detector** suppresses its findings and
actions; **Enable detector** preserves its configured action. **More options →
Restore default** enables it and restores the default action. All three use the
same approval flow as action edits; pending changes do not alter the active rule.
Existing saved rules without an `enabled` field remain enabled. Disabling Presidio
also stops outbound analysis requests. Disabling marking/caveat findings does not
disable the independent classification routing checks. Other enabled detectors can
still act on overlapping text. The preview always sanitises identifiers,
even if the configured action is allow/log, and is neither persisted nor sent
to a model.

## Operating scope

Log-only evaluates **Magpie** controls and records would-action metadata while
passing traffic unchanged. It is a trial, not privacy enforcement. Authentication
and independently configured native Agentgateway policies still operate.
Enforcement applies Magpie actions only on routes connected to its guardrails.
The existing LLM routes have request and response hooks. New routes/models must
have equivalent hooks; the native MCP connection UI does not automatically wire
the separate `/mcp/check` API into MCP traffic. SECRET and TOP SECRET are
refused by Magpie in enforcement mode even if a destination has a PROTECTED
approval; log-only records that would-block decision without blocking. Native policy editing can change
those connections, so grant gateway edit actions carefully and verify route
coverage after changes.

The recognisers inspect text. This release does not add OCR, arbitrary attachment
parsing, a healthcare entitlement lookup, a state licence verification service,
or Bower's full structured-event field-name secret engine. Test both realistic
synthetic positives and negatives for each organisation. A checksum is a useful
candidate filter, not proof of identity. Prompts can also contain sensitive
information beyond identifiable numbers.

## Australian alignment

- **APPs 1, 3 and 6:** configuration records, minimisation and approvals support
  accountable handling. The organisation still needs a lawful purpose, privacy
  assessment, notices and consent where required.
- **APP 8:** region restrictions reduce unintended transfers. Assess contracts,
  subprocessors, support access and actual processing; an Australian hosting
  label alone does not settle overseas-disclosure obligations.
- **APPs 9 and 11, TFN Rule:** identifier protection and role controls help limit
  exposure. Establish lawful identifier handling, justified retention, secure
  disposal and breach response. Do not treat pseudonymisation as guaranteed
  de-identification.
- **ISM September 2026:** decision events contribute evidence for 2159, 0585 and
  1959; access restrictions and append-only application records contribute to
  1985 and 1815 respectively. Configure and prove central collection, monitoring,
  backup protection and searchable retention (1988). A saved retention value is
  not evidence of actual retention or disposal.
- **PSPF 2026 sections 9.2–9.6:** marking/caveat recognition, destination approval
  and access groups support handling decisions. The entity must classify and
  authorise the system, enforce need-to-know and caveats, assess suppliers and
  meet handling requirements. Magpie is not a cross-domain solution. The
  Australian residency rule is Magpie product policy, not a claim that PSPF
  universally requires all such information to remain in Australia.

Authoritative sources reviewed 11 October 2026:
[OAIC APPs](https://www.oaic.gov.au/privacy/australian-privacy-principles/read-the-australian-privacy-principles),
[TFN Rule guidance](https://www.oaic.gov.au/privacy/privacy-guidance-for-organisations-and-government-agencies/handling-personal-information/the-privacy-tax-file-number-rule-2015-and-the-protection-of-tax-file-number-information),
[ISM security assurance](https://www.cyber.gov.au/ism/2026-09/guidelines-for-security-assurance),
[PSPF 2026](https://www.protectivesecurity.gov.au/publications-library/pspf-annual-release-2026),
[current protective markings](https://www.stylemanual.gov.au/writing-and-designing-content/security-classifications-and-protective-markings).
PSPF applicability depends on the entity and its obligations; it is not a
universal requirement for every Australian organisation.

## Tune false positives

**Tune DLP** is in the main navigation. Choose a detector from recent finding
counts or the selector, adjust its action, and compare an active rule with a
draft using synthetic text. Audit log rows with DLP findings offer a **Tune**
button for each detector. The comparison reports detector names, matching
reasons, UTF-8 byte offsets and decisions. Its reference preview always sanitises
recognised values, including values excluded by the draft. The test is local,
not audited, not persisted and does not call Presidio or an AI provider. It does
not simulate model approvals, routing, injection screening or the complete
request/response transport.

Monitor-only (`log`) affects one detector. Exact-value exceptions suppress only
that detector’s matching findings, across all traffic connected to Magpie. They
are case-sensitive and include whitespace and punctuation. Another detector can
still act on overlapping text. Protective markings and caveats cannot have exact
exceptions, and classification routing remains independent.

Exceptions persist an HMAC-SHA256 fingerprint keyed by the session secret, plus
an ID and operator label; the original value is not saved. Do not include the
value in the label. Rotating the session secret invalidates existing fingerprints
and matching reverts to normal detection until exceptions are recreated. Labels
are visible to policy readers and reviewers; fingerprints are omitted from the
tuning API. Each detector allows up to 32 exceptions, with values up to 512 bytes
and labels up to 80 bytes. Remove an exception through the same preview/save
flow; restore-default clears every exception on that detector.

Saving requires `editDlp`, checks the active revision, rejects competing pending
changes, and uses the normal second-person approval setting. The tuner never
changes global enforcement mode. Existing rules lacking `exceptions` retain
normal matching. No production detector action or exception is changed by the
feature deployment.
