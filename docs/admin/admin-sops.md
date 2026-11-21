# Magpie Gateway administrator SOPs

These standard operating procedures (SOPs) cover routine day-2 administration and incident response tasks for Magpie Gateway in enterprise Microsoft and Australian Government environments. Each SOP states its purpose, when to run it, who runs it, prerequisites, numbered steps, and explicit verification checks.

## Document control

| Item | Value |
|---|---|
| Version | 2.0 |
| Date | 21-11-2026 |
| Owner | Magpie Gateway platform administrator |
| Review | Every 12 months, or after a release affecting identity, SIEM, or persistence |
| Audience | Platform administrators, SOC analysts, Entra ID & Intune administrators |

## Contents

1. SOP 1 — Staged trial-to-enforcement rollout by Microsoft Entra group
2. SOP 2 — Incident response: activating and clearing emergency kill switches
3. SOP 3 — Rotating SCIM bearer tokens, webhook tokens, and Entra client secrets
4. SOP 4 — Emergency user offboarding (Entra SCIM and manual override)
5. SOP 5 — Applying or updating a governance posture preset with two-person approval
6. SOP 6 — Troubleshooting Microsoft Sentinel ingestion and cursor health
7. SOP 7 — Backing up, testing restore, and customer exit export
8. SOP 8 — Quarterly model, tool, and MCP approval expiry review
9. SOP 9 — Break-glass recovery administrator access

---

## 1. SOP 1 — Staged trial-to-enforcement rollout by Microsoft Entra group

**Purpose.** Move an organisation safely from **Log-only trial** to **Enforcing** one pilot cohort at a time, with instant single-group rollback if false positives appear.

**When to run.** At the end of a 1–2 week log-only observation period, and for each subsequent rollout wave.

**Who.** Magpie Admin or Owner holding `ChangeEnforcement` (and `EditDlp` if tuning detectors).

**Before you start.**

- Confirm at least one week of representative traffic has been recorded in **Decision audit**.
- Ensure your pilot Entra security group (for example `SG-AI-Pilot-Engineers`) is synced via SCIM and bound under **Identity & Entra**.

**Steps.**

1. Open **Operate → Control Centre** and read the **Readiness** summary under **Enforcement & staged cohort rollout**.
2. Open **Operate → Decision audit**, filter for `decision = log`, and inspect every `wouldAction` that would block traffic.
3. For legitimate false positives (such as a public switchboard phone number or internal email format), open **Protection → Tune DLP**, compare against a synthetic sample, save for approval, and have a second **Approver** activate the tuned rule.
4. Return to **Control Centre → Group rollout cohorts**. Locate the pilot group, confirm its virtual **Gateway key names** are listed, switch **Cohort mode** to **Enforcing**, and select **Save**.
5. Monitor pilot feedback and Microsoft Sentinel (`MagpieAudit_CL`) for 48–72 hours.
6. Repeat for additional cohorts. Only when every pilot wave is clean, switch **Organisation fallback mode** to **Enforcing**.

**Verification.**

- Topbar status pill updates to `Trial · 1 enforcing` (then `Enforcing` once fallback switches).
- Synthetic ABN test prompt from a pilot user is blocked (`HTTP 400`); same prompt from a non-pilot user logs without blocking.
- Decision Audit contains an atomic `access` or `settings` audit entry attributed to your account.

---

## 2. SOP 2 — Incident response: activating and clearing emergency kill switches

**Purpose.** Immediately halt all enforced AI traffic globally, or halt a specific misbehaving autonomous agent, during a confirmed data spill, prompt-injection incident, or compromised API key.

**When to run.** Immediately upon P1/P2 security incident declaration by the SOC or ITSA.

**Who.** Magpie Admin or Owner holding `ManageSettings`.

**Before you start.**

- Record the incident ticket number.
- Remember: kill switches bypass second-person approval intentionally and take effect on the very next request for enforced traffic. Cohorts explicitly set to **Trial** remain log-only unless their cohort mode is also switched to **Enforcing**.

**Steps.**

1. Open **Operate → Control Centre** and scroll to **Emergency kill switches**.
2. **For a single runaway agent**: add its `agentId` to **Per-agent kill switch** and select **Save per-agent list**.
3. **For a full gateway halt**: tick **Confirm I want to block all enforced AI traffic immediately** and select **Activate global kill switch**.
4. Confirm the red **Global kill switch is active** banner appears at the top of the console.
5. Contain the underlying incident (revoke compromised virtual API key in **LLM → Virtual API Keys**, or disable user in Entra ID).
6. Once the Incident Commander authorises resumption, return to **Control Centre** and select **Clear global kill switch** (or remove the agent ID and save).

**Verification.**

- Confirm enforced requests receive an immediate kill-switch block while active and succeed after clearing.
- Confirm Decision Audit and Microsoft Sentinel (`MagpieAudit_CL`) contain both `"Global kill switch enabled"` and `"Kill switches cleared"` records with timestamps and operator identity.

---

## 3. SOP 3 — Rotating SCIM bearer tokens, webhook tokens, and Entra client secrets

**Purpose.** Rotate integration credentials on your agency schedule (every 90–180 days) or after administrator offboarding.

**When to run.** Scheduled rotation window or suspected credential exposure.

**Who.** Magpie Access Administrator (`ManageAccess`) + Entra Application Administrator + Azure Contributor.

**Steps.**

### 3.1 Rotate SCIM 2.0 bearer token (zero downtime)

1. Open **Workspace → Identity & Entra**.
2. Select **Rotate SCIM bearer token** and copy the new `mp_scim_...` value.
3. Immediately open **Entra admin center → Enterprise applications → Magpie Gateway → Provisioning → Admin Credentials**, paste the new **Secret Token**, select **Test Connection**, and **Save**.

### 3.2 Rotate Entra OIDC client secret & webhook token (Azure)

1. Add a new client secret in Entra ID (keeping the old secret active for 15 minutes during rollout).
2. Update Key Vault secret `magpie-oidc-client-secret` (and `magpie-webhook-token` if rotating internal webhook auth), then restart the Container App revision:

   ```powershell
   az keyvault secret set --vault-name <kv-name> --name magpie-oidc-client-secret --value '<new-secret>'
   az containerapp revision restart --name ca-<prefix> --resource-group rg-<prefix>-<location> --revision <latest-revision>
   ```

3. Remove the expired Entra client secret once sign-in is verified.

**Verification.**

- Old SCIM token returns `HTTP 401 Unauthorized` on `/scim/v2/ServiceProviderConfig`; Entra **Test Connection** succeeds with the new token.
- Fresh incognito browser sign-in completes Entra OIDC + MFA cleanly.
- Decision Audit records `SCIM provisioning bearer token rotated.`.

---

## 4. SOP 4 — Emergency user offboarding (Entra SCIM and manual override)

**Purpose.** Terminate a departed or suspended user's portal sessions and AI gateway API access immediately.

**When to run.** Immediately upon HR termination or security suspension notice.

**Who.** Entra ID Administrator or Magpie Access Administrator (`ManageAccess` + `EditGatewayKeys`).

**Steps.**

1. **Automated path (Entra SCIM):** Disable the user account in Microsoft Entra ID (or remove them from the Magpie Enterprise Application) and run **Provision on demand**. Magpie automatically disables their `portal_users` record and deletes all rows for their subject in `sessions`.
2. **Immediate manual override (if waiting for SCIM is unacceptable):** Open **Workspace → Roles and groups** (`/ui/governance/access`), select the user under **Users**, and select **Disable user**. Their active browser sessions are revoked atomically.
3. **Revoke AI API keys:** Open **LLM → Virtual API Keys** (and **Workspace → Subscription pools** if used) and revoke any virtual API keys issued to that user.

**Verification.**

- User's active session receives `HTTP 401` on next request.
- Any API call using their revoked virtual API key is rejected.
- Decision Audit records `Portal user disabled.` attributed to `scim-provisioning` or your admin account.

---

## 5. SOP 5 — Applying or updating a governance posture preset with two-person approval

**Purpose.** Transition an environment cleanly between baselines (for example from *Pilot / Observe Only* to *Government — OFFICIAL: Sensitive*).

**When to run.** After governance board approval to change enforcement posture.

**Who.** Person A: Magpie Admin (`ChangeEnforcement`, `ManageSettings`, `EditDlp`). Person B: Different authorised **Approver** (`Approve`).

**Steps.**

1. Person A opens **Operate → Control Centre**, selects **Review changes** on the target preset, verifies every row in the diff table, and selects **Apply / stage**.
2. Person B signs in with their own Entra account, opens **Operate → Approvals**, reviews every staged revision diff, and selects **Approve & activate** (or **Reject** if any rule does not match the change record).

**Verification.**

- Target preset card in **Control Centre** displays the green **Active** pill (`Matches current`).
- Attempting self-approval by Person A is refused by the server with `HTTP 403`.

---

## 6. SOP 6 — Troubleshooting Microsoft Sentinel ingestion and cursor health

**Purpose.** Diagnose and resolve stalled or failing audit log delivery to Microsoft Sentinel (`MagpieAudit_CL`).

**When to run.** When **Protection → SIEM & Sentinel** shows status **Retrying** or a non-empty **Last error**, or when Sentinel stops receiving events for > 10 minutes.

**Who.** SOC Engineer / Azure Platform Administrator.

**Steps.**

1. Open **Protection → SIEM & Sentinel** and read the exact **Last error** column on the affected sink:
   - `HTTP 401` / `HTTP 403`: Verify Magpie's User-Assigned Managed Identity (or Entra App) holds the **Monitoring Metrics Publisher** role on the target DCR resource.
   - `HTTP 404`: Confirm `DCR immutable ID` (`dcr-...`) and `Stream name` (`Custom-MagpieAudit_CL`) match the DCR JSON.
   - `IMDS unreachable`: Managed Identity auth works only on Azure hosts; switch to **Entra App Registration (client credentials)** if running on AWS or on-premises.
   - `HTTP 429` / `503`: Azure Monitor throttling — Magpie automatically preserves its cursor and retries every 5 seconds with no event loss.
2. Once corrected, select **Send test** on the sink row.

**Verification.**

- **Send test** returns green **Delivered synthetic test event**.
- `Last error` clears on the next 5-second worker cycle and `Delivered` count climbs.

---

## 7. SOP 7 — Backing up, testing restore, and customer exit export

**Purpose.** Maintain tested recovery capability for SQLite control-plane state and `/data/gateway/config.yaml`, and support clean agency exit.

**When to run.** Automated daily snapshots + quarterly restore test + prior to any major upgrade or decommissioning.

**Who.** Cloud Platform Administrator.

**Steps.**

1. **Snapshot persistent Azure Files share (Azure):**

   ```powershell
   az storage share-rm snapshot `
       --resource-group rg-<prefix>-<location> `
       --storage-account <storage-account-name> `
       --name magpie-data
   ```

2. **Logical governance & evidence export (for archival or customer exit):**
   - Open **Operate → Evidence pack** and download the **Signed bundle (JSON + SHA-256)** and **Executive PDF**.
   - Open **Operate → Decision audit** and export the complete `audit.csv`.
   - Open **Operate → Approvals** and export `use-cases.csv`.
3. **Quarterly restore drill:** Mount the latest snapshot into a non-production Container App in a sandbox resource group, start Magpie with `MAGPIE_GUARDRAIL_MODE=log-only`, verify `magpie.sqlite` integrity (`PRAGMA integrity_check;`), and confirm audit count and policies match production.

**Verification.**

- Restore drill passes `PRAGMA integrity_check` (`ok`), portal serves `/healthz` (`200`), and **Controls** displays the exact active policy posture.

---

## 8. SOP 8 — Quarterly model, tool, and MCP approval expiry review

**Purpose.** Prevent unexpected outages from expired model/tool approvals and ensure every approved AI capability retains an accountable business owner.

**When to run.** First week of every quarter.

**Who.** Approver / AI Governance Lead.

**Steps.**

1. Open **Operate → Approvals** (`/ui/approvals`).
2. Filter by **Approved** and sort/scan **Expiry** for items expiring within 60 days; also check the **Expired** filter.
3. Contact each item's **Business owner** to confirm ongoing business need, PSPF classification, and privacy impact assessment currency.
4. Submit renewed approval requests for continuing items and have a second approver release them.
5. Export the updated **AI Use-Case Register (CSV)** for the agency's DTA accountability register.

**Verification.**

- Zero active production models or MCP tools appear under **Expired**.

---

## 9. SOP 9 — Break-glass recovery administrator access

**Purpose.** Regain administrative access if Microsoft Entra ID is unreachable or OIDC settings are misconfigured.

**When to run.** Emergency recovery only; never for routine administration.

**Who.** Authorised emergency custodian with access to Azure Key Vault break-glass secret.

**Steps.**

1. Retrieve the sealed local recovery passphrase (`magpie-admin-password`) from your privileged break-glass vault per agency PAM procedure.
2. Sign in at `/ui/login` with username `admin` (or `MAGPIE_ADMIN_USER`) and the recovery passphrase.
3. Repair the OIDC/Entra configuration or user group mapping, verify normal Entra SSO sign-in works again, and immediately sign out of the break-glass session.
4. Rotate `magpie-admin-password` (or disable local break-glass once Entra access is restored) and review the audited break-glass login entry in Decision Audit and Microsoft Sentinel.

**Verification.**

- Decision Audit and Sentinel record the break-glass login, every action taken during recovery, and the post-incident credential rotation.
