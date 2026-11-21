# Magpie Gateway security and assurance guide

This document describes Magpie Gateway's security architecture, trust boundaries, threat model, data handling guarantees, and an honest, source-cited mapping against Australian Government security and AI governance frameworks: **ASD Information Security Manual (ISM September 2026)**, **ASD Essential Eight (November 2023 model)**, **Protective Security Policy Framework (PSPF)**, **Australian Privacy Principles (APPs)**, and **DTA Policy for the responsible use of AI in government 2.0** (including the **June 2026 Agentic AI Addendum**).

## Document control

| Item | Value |
|---|---|
| Version | 2.0 |
| Date | 21-11-2026 |
| Owner | Magpie Gateway security lead |
| Review | Every 12 months, or upon ISM / DTA policy revision |
| Audience | CISOs, ITSAs, IRAP assessors, privacy officers, enterprise architects |

## Contents

1. Executive assurance statement
2. System architecture and trust boundaries
3. Identity, authentication, and RBAC model
4. Data handling, privacy, and secret isolation
5. Threat model and concrete mitigations
6. Honest Australian Government framework mapping
7. Shared responsibility matrix (Magpie vs. Agency)

---

## 1. Executive assurance statement

Magpie Gateway provides a technical enforcement, inspection, and audit layer for AI model and MCP tool traffic. In keeping with Australian Government assurance principles:

- **No compliance theatre.** Every mapping in Magpie's UI and exported evidence packs is explicitly labelled **Direct** or **Interpretive** and cites the official published government source.
- **No silent degradation.** Guardrail webhooks fail closed when the portal is unreachable; Trial (`log-only`) mode is displayed prominently across every page and recorded on every audit event (`detail.mode`, `detail.wouldAction`).
- **No external phone-home.** Telemetry is off by default; UI artwork, stylesheets, and scripts are bundled locally with zero third-party CDN requests.

---

## 2. System architecture and trust boundaries

| Trust boundary | Enforcement mechanism |
|---|---|
| **Workstation → Gateway Ingress** | TLS 1.2+ mandatory; corporate WAF / Front Door / internal VNet; Defender for Endpoint indicators block shadow direct LLM endpoints |
| **User Browser → Portal UI (`/ui/*`, `/api/magpie/*`)** | Entra ID OIDC with PKCE (`S256`), `nonce`, `state`, HttpOnly/SameSite/Secure session cookies, idle (30 min) and absolute (12 hr) session expiry, strict same-origin CSRF enforcement on all state-changing requests |
| **Proxy (`agentgateway`) → Portal Guardrails (`/guardrails/*`, `/mcp/check`)** | Loopback-only (`127.0.0.1:8088`) inside the pod/Container App; constant-time `x-magpie-webhook-token` verification; routes return `404` on external ingress |
| **Portal → Proxy Admin API (`127.0.0.1:15000`)** | Explicit route and HTTP-method allowlist (`native.rs`); per-field config diff checked against granular `EditGateway*` permissions; cookies and auth headers stripped before forwarding |
| **Entra Provisioning → SCIM 2.0 (`/scim/v2/*`)** | Constant-time Bearer token authentication (stored in SQLite runtime settings or Key Vault); persisted to SQLite; deactivation immediately revokes portal sessions |
| **Portal → Microsoft Sentinel / Azure OpenAI** | Passwordless Azure User-Assigned Managed Identity (IMDS) or Entra client credentials; least-privilege RBAC (`Monitoring Metrics Publisher` on DCR only; `Cognitive Services OpenAI User` on target Foundry account only) |

---

## 3. Identity, authentication, and RBAC model

- **Zero implicit access:** Newly authenticated Entra users with no matching role or group claim receive **zero permissions** — there is no default Viewer fallback.
- **Per-action RBAC:** 27 distinct `Permission` grants separate operational duties cleanly. For example, an operator can hold `ChangeEnforcement` without `ManageSettings`, or `Approve` without `EditDlp`, or `ReadAudit`/`ExportAudit` without `ReadGatewayConfig` (preventing auditors from viewing upstream provider credentials).
- **Separation of duties:** Enforced on the server for all staged policy/DLP/settings revisions (`change::approve`) and all model/tool/MCP approvals (`control_centre.rs` / `lib.rs`).
- **Live revocation:** Role edits, manual user disabling, and SCIM `active: false` updates take effect immediately and purge matching rows from the `sessions` table.
- **SAML 2.0 deliberately disabled:** Magpie refuses SAML assertions rather than implementing partial XML signature handling; Microsoft Entra ID uses standard OIDC + PKCE.

---

## 4. Data handling, privacy, and secret isolation

| Data class | How Magpie handles it |
|---|---|
| **Raw prompts & model responses** | Inspected in memory only; **never written** to `audit_events`, SIEM sinks (unless `capturePayloads` is explicitly enabled), or Teams/Slack/Email alerts |
| **DLP preview samples (`/api/magpie/dlp/preview`)** | Evaluated in memory and discarded; response returns sanitised preview only |
| **Exact-value DLP exceptions** | Stored exclusively as keyed **HMAC-SHA256 fingerprints** (bound to detector kind + `MAGPIE_SESSION_SECRET`); raw exception values are never persisted or echoed back |
| **Secrets (OIDC, SCIM, SIEM, SMTP, Teams webhooks, Subscription Pools)** | Stored outside policy documents; excluded from `/api/export`, `/api/magpie/evidence-pack.json`, and audit logs; write-only in UI (`hasSecret: true` only) |
| **Audit log immutability** | Enforced by SQLite `BEFORE UPDATE` and `BEFORE DELETE` triggers (`audit_events_no_update`, `audit_events_no_delete`); central tamper-resistant retention provided by streaming to Microsoft Sentinel |

---

## 5. Threat model and concrete mitigations

| Threat | Mitigation in Magpie Gateway | Residual agency responsibility |
|---|---|---|
| **Unintentional citizen PII / TFN / Medicare leakage to LLMs** | 20 built-in checksum-validated AU DLP detectors; blocks enforced LLM requests cleanly without corrupting agent tool calls | Tune false positives in Trial mode; educate staff not to paste RESTRICTED/SECRET material |
| **Data sovereignty breach (APP 8 / PSPF)** | Sovereign Mode restricts destinations to AU regions; built-in PSPF engine enforces AU residency on `OFFICIAL: Sensitive`, explicit allow-list on `PROTECTED`, and hard refusal on `SECRET`/`TOP SECRET` | Verify cloud provider region contracts and avoid Global/Data Zone SKUs for sovereign workloads |
| **Unapproved MCP tool invocation or prompt injection** | Default-deny tool allow list driven by time-bounded approvals; prompt-injection scanner; per-tool rate limits; immediate per-agent and global kill switches | Configure MCP routes to call `/mcp/check`; review high-impact tools before approval |
| **Single insider tampering with policy or disabling guardrails** | Second-person approval gate on all policy, DLP, and settings edits; stale-revision rejection; immutable audit trail streamed to Sentinel | Enforce separate Entra accounts/roles for Policy Editors vs. Approvers; monitor Sentinel alerts |
| **Departed staff retaining AI portal access** | Persistent Entra SCIM 2.0 immediately disables user and revokes all active browser sessions | Revoke user's individual Virtual API Key; enforce Entra Conditional Access |
| **Shadow AI bypass around the gateway** | Intune managed settings lock coding agents to Magpie; Defender for Endpoint indicators block direct public LLM endpoints | Deploy generated Intune scripts and reviewed Defender indicators across managed fleet; implement WDAC |

---

## 6. Honest Australian Government framework mapping

| Framework & reference | Magpie technical capability | Mapping type |
|---|---|---|
| **ISM-2159** *(ISM Sep 2026 — log agentic AI tool invocations, requests, outputs)* | Emits structured audit event for every LLM request, MCP tool check, and policy decision; streams via Sentinel Logs Ingestion API / OTLP | Interpretive |
| **ISM-1988** *(Searchable log retention >= 12 months)* | Configurable 365-day retention target in UI + Azure Log Analytics `retention_in_days = 365` in Terraform | Interpretive |
| **ISM-1815 / ISM-1985** *(Protect logs from modification, deletion, and unauthorised access)* | SQLite immutability triggers + `ReadAudit` RBAC separation + central export to Sentinel | Interpretive |
| **Essential Eight — MFA** | Portal login uses Entra ID OIDC + PKCE; phishing-resistant MFA enforced via Entra Conditional Access | Interpretive |
| **Essential Eight — Restrict admin privileges** | 27 granular RBAC actions; break-glass off by default and audited; separation of duties | Partial (portal only) |
| **Essential Eight — Application control, patching, macros, hardening, backups** | Outside the gateway; Windows endpoint guide and Intune scripts support workstation rollout | Outside Magpie |
| **DTA Responsible AI Policy 2.0 (Standard for accountability 2.0)** | Use-case register CSV/JSON export using published DTA field list | Direct (field list only) |
| **DTA Agentic AI Addendum — AGT.3 / AGT.4 / AGT.7 / AGT.8** | Default-deny tool allow list, approvals, kill switches (`AGT.3`), PSPF & AU sovereignty routing (`AGT.4`), MCP tool audit (`AGT.7`), Sentinel KQL & DLP review (`AGT.8`) | Interpretive |

---

## 7. Shared responsibility matrix

| Control area | Magpie Gateway provides | Agency / Enterprise owns |
|---|---|---|
| **Identity & MFA** | Entra OIDC + PKCE, App Role mapping, persistent SCIM session revocation | Entra Conditional Access, FIDO2/Passkey MFA, access reviews |
| **Workstation enforcement** | Generated Intune PowerShell scripts & Defender indicator CSVs | Intune code signing & ring rollout, Defender blocking decisions, Windows App Control (WDAC) |
| **Central SIEM & SOC** | Native Sentinel Logs Ingestion worker, DCR schema, 6 KQL rules, Teams Adaptive Cards | Sentinel workspace governance, 24x7 alert triage, incident response playbooks |
| **Residency & classification** | Sovereign Mode, built-in PSPF marking checks, `MAGPIE_APPROVED_PROTECTED` | Cloud provider legal contracts, IRAP assessment of underlying Azure/AWS hosting and model endpoints |
| **Availability & backup** | Single-writer SQLite on encrypted ZRS Azure Files / EBS gp3, Terraform IaC | Scheduled snapshots, quarterly restore drills (SOP 7), KMS/Key Vault custody |
