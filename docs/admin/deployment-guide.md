# Magpie Gateway deployment guide

This guide covers end-to-end deployment, verification, upgrade, and decommissioning of **Magpie Gateway** on **Microsoft Azure (Australia East / Australia Central)** using Azure Container Apps, Azure Key Vault, User-Assigned Managed Identity, Microsoft Entra ID, and Microsoft Sentinel, as well as **AWS EKS (`ap-southeast-2`)** engagements.

## Document control

| Item | Value |
|---|---|
| Version | 2.0 |
| Date | 21-11-2026 |
| Owner | Magpie Gateway platform administrator |
| Review | Every 12 months, or after a release that changes infrastructure or identity wiring |
| Audience | Cloud platform engineers, Azure/AWS architects, Entra ID administrators |

## Contents

1. Architecture overview
2. Prerequisites and Azure permissions
3. Deploying to Microsoft Azure with PowerShell and Terraform
4. Connecting Azure OpenAI and Microsoft AI Foundry (sovereign residency)
5. Wiring Microsoft Entra ID SSO and SCIM 2.0
6. Wiring Microsoft Sentinel Logs Ingestion
7. Deploying to AWS EKS (`ap-southeast-2`)
8. Reference: Terraform variables and environment variables
9. Verification, upgrades, and removal

---

## 1. Architecture overview

Magpie Gateway runs as a **dual-container single-replica workload** (on Azure Container Apps or Kubernetes/EKS):

| Container | Port(s) | Responsibility |
|---|---|---|
| `portal` (`magpie-portal`) | `8088` | Authenticated React + Rust admin console, Control Centre, Approvals Inbox, Entra OIDC + SCIM 2.0, Australian DLP/PSPF guardrail webhook, SIEM background exporter, Teams/Email alert worker, SQLite control-plane database (`/data/magpie.sqlite`) |
| `proxy` (`agentgateway`) | `4000` (data plane), `15000` (loopback admin), `15021` (readiness) | High-performance Rust LLM, MCP, and A2A gateway translating OpenAI/Anthropic/Bedrock/Azure Foundry APIs; calls `http://127.0.0.1:8088/guardrails/*` over localhost with `x-magpie-webhook-token` |

### Why single-replica with persistent storage

The portal stores policies, approvals, RBAC, SCIM directory state, SIEM cursors, and immutable `audit_events` in SQLite at `/data/magpie.sqlite`, while the proxy persists UI-managed routes at `/data/gateway/config.yaml`. Running `min_replicas = 1, max_replicas = 1` (Azure Container Apps `Single` revision mode) or Kubernetes `Recreate` guarantees a single SQLite writer with zero split-brain corruption.

### Network boundary rules

- Public/corporate ingress forwards `/v1/*`, `/mcp`, and `/a2a` to proxy port `4000` and `/` to portal port `8088`.
- **Never expose `/guardrails/*` or `/mcp/check` on public ingress.** The proxy calls those routes over loopback (`127.0.0.1:8088`) with a shared `MAGPIE_WEBHOOK_TOKEN`.
- On Azure, set `enable_internal_only = true` (default) to place the Container Apps environment inside a private VNet subnet (`snet-aca-infra`) behind Azure Front Door Private Link or Application Gateway WAF.

---

## 2. Prerequisites and Azure permissions

| Requirement | Minimum version / role |
|---|---|
| PowerShell | PowerShell 7.4+ (Windows, macOS, or Linux) |
| Azure CLI (`az`) | 2.65+ signed in to the target tenant (`az login --tenant <tenant-guid>`) |
| Terraform | 1.6.0+ |
| Azure Subscription RBAC | `Contributor` + `User Access Administrator` on the target subscription or resource group (to create Key Vault, Container Apps, Sentinel DCR, and Managed Identity role assignments) |
| Microsoft Entra ID Role | `Application Administrator` or `Cloud Application Administrator` (for the `Entra` step) |

---

## 3. Deploying to Microsoft Azure with PowerShell and Terraform

The production Azure Terraform module lives in `deploy/terraform/azure/` (`versions.tf`, `variables.tf`, `main.tf`, `app.tf`, `outputs.tf`) and is orchestrated by `deploy/Deploy-MagpieAzure.ps1`.

### 3.1 Define your splatted deployment parameters

```powershell
$magpie = @{
    TenantId              = '<tenant-guid>'
    SubscriptionId        = '<subscription-guid>'
    NamePrefix            = 'magpieagency'
    Location              = 'australiaeast'
    PublicUrl             = 'https://magpie.agency.gov.au'
    GuardrailMode         = 'log-only'
    AzureOpenAiResourceId = '/subscriptions/<sub-guid>/resourceGroups/rg-ai/providers/Microsoft.CognitiveServices/accounts/aoai-agency-au'
    EnableSentinel        = $true
    InternalOnly          = $true
}
```

### 3.2 Preview with `-WhatIf`

Always run a non-destructive preview first:

```powershell
./deploy/Deploy-MagpieAzure.ps1 @magpie -Step Preflight,Infra -WhatIf
```

### 3.3 Execute full deployment

```powershell
./deploy/Deploy-MagpieAzure.ps1 @magpie -Step Preflight,Entra,Infra,Verify
```

What Terraform provisions in `rg-<NamePrefix>-<Location>`:

1. **Virtual Network (`vnet-<prefix>`)**: `/23` delegated subnet `snet-aca-infra` for Container Apps + `snet-private-endpoints`.
2. **User-Assigned Managed Identity (`id-<prefix>`)**: granted `Key Vault Secrets User`, `Monitoring Metrics Publisher` (on the Sentinel DCR), and optionally `Cognitive Services OpenAI User` (on your Azure OpenAI / AI Foundry account).
3. **Azure Key Vault (`kv-<prefix>-<suffix>`)**: RBAC-authorised, purge-protected, 90-day soft delete; stores auto-generated `magpie-session-secret`, `magpie-webhook-token`, and optional Entra/SCIM/recovery secrets.
4. **Encrypted Azure Files (`st<prefix><suffix>` / `magpie-data`)**: Zone-redundant storage (`ZRS`), TLS 1.2 minimum, HTTPS-only, mounted at `/data`.
5. **Log Analytics + Microsoft Sentinel (`law-<prefix>`, `dce-<prefix>`, `dcr-<prefix>`)**: 365-day retention (`ISM-1988`), Sentinel workspace onboarding, and `Custom-MagpieAudit_CL` Logs Ingestion DCR.
6. **Azure Container Apps (`cae-<prefix>`, `ca-<prefix>`)**: Dual-container (`portal` + `proxy`) with Key Vault secret references and health probes (`/healthz` and `/healthz/ready`).

---

## 4. Connecting Azure OpenAI and Microsoft AI Foundry (sovereign residency)

Magpie's native **LLM → Models** editor supports both **Azure OpenAI** (`openAI`) and **Microsoft AI Foundry** (`foundry`) with passwordless **Managed Identity** authentication (`AZURE_CLIENT_ID` is automatically injected by the Azure Terraform module).

### Australian data residency considerations

| Azure deployment SKU | Processing location | Compatible with Sovereign Mode ON? | Recommended posture preset |
|---|---|---|---|
| **Australia East — Standard (Regional)** | Australia East only | **Yes** | *Government — OFFICIAL: Sensitive* or *PROTECTED* (with `MAGPIE_APPROVED_PROTECTED`) |
| **Australia East — Data Zone (Asia Pacific)** | Asia Pacific data zone (may process outside Australia) | **No** *(blocked by Sovereign Mode)* | *Enterprise Standard* (Sovereign Mode Off; DLP & PSPF markings remain active) |
| **Global Standard** | Microsoft global pool | **No** *(blocked by Sovereign Mode)* | *Enterprise Standard* only for non-sensitive workloads |

::: important
Verify your Microsoft Customer Agreement, Azure OpenAI deployment type, and data processing location in the Azure portal before classifying a route as Australian sovereign or PROTECTED.
:::

---

## 5. Wiring Microsoft Entra ID SSO and SCIM 2.0

1. Confirm the Entra Enterprise Application created by `-Step Entra` has **Assignment required = Yes** and your agency's **Conditional Access** policy attached (phishing-resistant MFA + compliant devices).
2. Add the six `Magpie.*` App Roles (`Magpie.Owner`, `Magpie.Admin`, `Magpie.PolicyEditor`, `Magpie.Approver`, `Magpie.Auditor`, `Magpie.Viewer`) and assign security groups.
3. Open **Workspace → Identity & Entra** in Magpie, select **Test OIDC discovery**, then select **Generate SCIM bearer token** and configure Entra Enterprise Application Provisioning pointing at `https://<your-host>/scim/v2`.

---

## 6. Wiring Microsoft Sentinel Logs Ingestion

1. Retrieve Terraform outputs (`sentinel_dce_endpoint`, `sentinel_dcr_immutable_id`, `managed_identity_client_id`).
2. In Magpie, open **Protection → SIEM & Sentinel**, add a **Microsoft Sentinel** sink with **Azure Managed Identity**, save, and select **Send test**.
3. Select **Download Sentinel DCR & KQL pack** and schedule the six KQL analytics rules in Microsoft Sentinel.

---

## 7. Deploying to AWS EKS (`ap-southeast-2`)

For AWS-hosted engagements, use `./magpie.sh` and `deploy/terraform/aws/` + `deploy/helm/magpie-gateway/`:

```bash
./magpie.sh new <client>       # Interactive wizard; probes AU Bedrock profiles
./magpie.sh up <client>        # Terraform VPC/EKS/ALB + Helm install
./magpie.sh sso <client>       # Optional Entra ID app registration via az CLI
./magpie.sh check <client>     # Live smoke test including DLP check
./magpie.sh uitest <client>    # Playwright WCAG 2.2 AA check against live stack
```

Full AWS shared-ALB and CloudFront notes remain documented in `docs/engagement.md`.

---

## 8. Reference: Terraform variables and environment variables

### Azure Terraform variables (`deploy/terraform/azure/variables.tf`)

| Variable | Default | Purpose |
|---|---|---|
| `name_prefix` | `magpie` | Lowercase alphanumeric prefix (3–14 chars) |
| `location` | `australiaeast` | Validated AU region (`australiaeast`, `australiasoutheast`, `australiacentral`, `australiacentral2`) |
| `portal_image` | `ghcr.io/yumaitau/magpie-gateway-portal:main` | Portal container image (pin to digest in production) |
| `proxy_image` | `ghcr.io/agentgateway/agentgateway:v1.6.0` | Upstream Agentgateway image (pin to digest in production) |
| `guardrail_mode` | `log-only` | Initial default seeded only when database has no saved mode |
| `enable_sentinel` | `true` | Provisions Log Analytics, Sentinel onboarding, DCE, and DCR |
| `enable_internal_only` | `true` | Restricts Container Apps ingress to internal VNet |
| `retention_days` | `365` | Log Analytics workspace retention (ISM-1988 baseline) |
| `approved_protected` | `""` | Comma-separated approved AU destinations for PROTECTED traffic |
| `azure_openai_resource_id` | `""` | Grants Managed Identity `Cognitive Services OpenAI User` |

### Portal runtime environment variables

| Variable | Required | Purpose |
|---|---|---|
| `MAGPIE_DATABASE_URL` | Yes | SQLite file path (`/data/magpie.sqlite`); `postgres://` is intentionally refused |
| `MAGPIE_SESSION_SECRET` | Yes | 32+ byte HMAC key for session cookies and DLP exception fingerprints |
| `MAGPIE_WEBHOOK_TOKEN` | Recommended | Shared secret required in `x-magpie-webhook-token` on `/guardrails/*` and `/mcp/check` |
| `MAGPIE_ENTRA_TENANT_ID` | SSO | Entra tenant GUID (shortcut for v2.0 OIDC issuer) |
| `MAGPIE_OIDC_CLIENT_ID` | SSO | Entra App Registration Client ID |
| `MAGPIE_OIDC_CLIENT_SECRET` | SSO | Entra client secret (from Key Vault) |
| `MAGPIE_PUBLIC_URL` | Behind proxy | Canonical HTTPS origin (e.g. `https://magpie.agency.gov.au`) |
| `MAGPIE_ADMIN_PASSWORD` | Optional | Local recovery administrator passphrase (16+ chars) |
| `MAGPIE_SCIM_TOKEN` | Optional | Initial SCIM bearer token (can also be rotated in UI) |
| `MAGPIE_GUARDRAIL_MODE` | Optional | Initial seed (`log-only` or `enforce`); UI owns mode after first boot |
| `MAGPIE_APPROVED_PROTECTED` | Optional | Comma-separated list of destinations allowed for PROTECTED markings |

---

## 9. Verification, upgrades, and removal

### Post-deployment verification checklist

1. `GET /healthz` returns `200 OK`; `GET /guardrails/request` from outside the VNet returns `404` or `401`.
2. Entra SSO sign-in completes with MFA and assigns the expected portal role.
3. **Identity & Entra → Test OIDC discovery** returns green **OIDC discovery reachable**.
4. **SIEM & Sentinel → Send test** returns **Delivered** and appears in `MagpieAudit_CL`.
5. **Control Centre** opens cleanly and **Compliance & Evidence Pack** downloads a signed JSON bundle with a 64-character SHA-256 digest.

### Upgrading container images

1. Take an Azure Files snapshot of the `magpie-data` share (see *Administrator SOPs, SOP 7*).
2. Update `portal_image` and/or `proxy_image` to the new pinned digests.
3. Run `./deploy/Deploy-MagpieAzure.ps1 @magpie -Step Infra -WhatIf`, review the plan, then apply without `-WhatIf`. SQLite migrations (`Store::migrate()`) execute automatically on startup without altering existing policy documents, RBAC, or enforcement mode.

### Decommissioning

Export your final **Signed Assurance Bundle**, **Audit CSV**, and **AI Use-Case Register CSV** first, then run:

```powershell
./deploy/Deploy-MagpieAzure.ps1 @magpie -Step Destroy
```
