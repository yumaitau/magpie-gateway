# Magpie Gateway plan

Working name: Magpie Gateway. Private repository
`yumaitau/magpie-gateway`. Upstream is agentgateway `main` at
`85d1f087` (Apache-2.0). This plan was checked against that tree, not
against the briefing alone.

## What already exists

agentgateway is a Rust proxy (`crates/agentgateway`, binary in
`crates/agentgateway-app`) with a Go Kubernetes controller
(`controller/`) and a React admin UI (`ui/`, Vite, TanStack Router,
Monaco, Playwright). Rust edition is 2024, toolchain 1.99.0.

Configuration is split:

- Startup settings (`config` in YAML) cover the admin address, the
  database URL, tracing and xDS. See `RawConfig` in
  `crates/agentgateway/src/lib.rs`.
- Routes, LLM providers, MCP servers, policies and the attached UI
  live in `LocalConfig` (`crates/agentgateway/src/types/local.rs`).
  File mode reloads the YAML. Hybrid mode stores UI-edited resources
  in Postgres or SQLite with a revision (`config_store.rs`).

CEL authorisation is real. `RuleSets::validate` in
`crates/agentgateway/src/http/authorization.rs` applies allow, deny
and require expressions. JWT claims are on `jwt.*`
(`crates/agentgateway/src/http/jwt.rs`). MCP tool calls expose
`mcp.tool.name` and `mcp.params` (`crates/agentgateway/src/cel/tests.rs`).
An allow list that does not match denies the call. With no rules, the
default is allow, so "deny unapproved tools" means Magpie must install
an allow list rather than rely on an empty policy.

Guardrails:

- LLM webhooks POST `GuardrailsPromptRequest` to `/request` and
  `GuardrailsResponseRequest` to `/response`
  (`crates/agentgateway/src/llm/policy/webhook.rs`). The action is an
  untagged `pass`, `mask` (replacement body) or `reject` (status and
  body). Audit mode can record without blocking.
- MCP `tools/call` runs guardrails before the upstream tool and again
  on the tool result (`mcp/session.rs`, `mcp/handler.rs`). The remote
  protocol is gRPC `ExtMcp` (`crates/protos/proto/ext_mcp.proto`), not
  the LLM JSON webhook. In-process CEL processors are the other option.

PII detection already exists for email, phone, credit card, US SSN and
Canadian SIN (`crates/agentgateway/src/llm/policy/pii/`). Builtins are
selected in a regex guardrail rule. There is no Australian recogniser
and no PSPF marking check.

The admin UI is not a basic shell. It already has login, providers,
models, MCP servers, policies, a CEL page, guardrails, logs, costs and
settings. Login is gateway OIDC or API-key policy on the attached UI,
not a portal session with RBAC, SCIM or an approvals queue. Playwright
covers the UI. axe is not in CI.

Postgres and SQLite are already used for request logs, budgets and
hybrid config. OpenTelemetry export is already configurable, and it is
opt-in via `config.tracing`. Regions exist as cloud-provider fields
(Bedrock, Vertex). Nothing denies a destination for being outside
Australia.

## Corrections to the briefing

- The UI is a full configuration console. Replacing it would fork a
  large React app and make every upstream sync painful. Magpie adds
  its own portal beside it.
- Postgres, SQLite, CEL-on-JWT, guardrail webhooks and PII builtins are
  already there.
- MCP tool inspection is gRPC ExtMcp or CEL, not the LLM webhook.
- The published ATO TFN checksum is not public. The ATO says the
  algorithm is released only to organisations with a bona fide need.
  Magpie will not ship a folklore modulo-11 TFN check.
- Centrelink does not publish a CRN checksum. Services Australia
  describes the format as nine digits and a letter.
- No official driver-licence checksum was found for any state or
  territory. Victoria states that a licence number is usually nine
  digits. Other jurisdictions are recorded as "format not published"
  rather than copied from unofficial validators.
- PSPF Policy 8 (2018) and the Style Manual (updated 23 September 2026,
  describing the 2026 PSPF) do not use the same numbering. The Style
  Manual treats OFFICIAL and UNOFFICIAL as optional and not security
  classifications. OFFICIAL: Sensitive, PROTECTED, SECRET and TOP
  SECRET are the classifications. Caveats sit with a classification.
- The DTA agentic AI addendum is best-practice guidance used with the
  AI technical standard. It does not define gateway control IDs. Any
  mapping is interpretive.
- Prompt Guard 2 is under the Llama 4 Community License, with a monthly
  active user threshold and an acceptable use policy. The weights will
  not be vendored.

## What Magpie changes, and where

Nothing under `crates/`, `controller/` or `ui/` is edited. A separate
workspace at `magpie/` holds the product code. `deploy/`, `detections/`
and `docs/` hold packaging, detection content and the compliance
documents. `NOTICE` lists the additions.

The gateway keeps doing transport, MCP, A2A and provider translation.
Magpie is the policy and administration side:

1. `magpie/crates/magpie` is a library: Australian recognisers, PSPF
   markings, secrets detection, DLP actions, sovereignty checks,
   approval and use-case register, CEL generation and evaluation, RBAC,
   OIDC session and SCIM models, change revisions, audit events, sink
   payloads, prompt-injection scanners, rate limits, kill switch, and
   the evidence report.
2. `magpie/crates/magpie-portal` is an axum process. It serves the
   portal, the SCIM endpoints, the OIDC login flow (including a mock
   IdP for tests), and the LLM guardrail webhook the proxy already
   knows how to call. MCP tool calls are judged by the same engine over
   a JSON check API. Wiring that decision into the proxy is a config
   snippet for CEL allow lists plus the webhook. A native gRPC ExtMcp
   server is not in this cut. The JSON check is what the tests and the
   mock MCP server call, before and after the tool.
3. The portal is server-rendered HTML in the portal crate, with its own
   wordmark. It does not load the agentgateway UI, fonts or images, and
   it does not call out to a CDN.
4. Deployment examples point the proxy's LLM webhook at the portal and
   show the generated CEL on the MCP route. Presidio runs as a sidecar
   from `ghcr.io/data-privacy-stack` when an operator sets the URL.
   In-process recognisers still run if Presidio is absent.

## Behaviour

### Portal access

OIDC authorisation-code with PKCE (S256). Presets for Microsoft Entra
ID, Okta and Google Workspace, plus a generic issuer. Sessions are
HMAC-signed cookies, `HttpOnly` and `SameSite=Lax`, `Secure` unless a
local setting turns that off. Idle and absolute timeouts are enforced
on each request. Logout clears the session. Back-channel logout
accepts a logout token and drops sessions for that subject.

A break-glass local administrator is off unless config enables it. The
password is stored as a bcrypt hash. Each use is an audit event, and
the portal shows a banner.

SAML 2.0 is optional and not a supported login method in this cut. The
settings page says so. Unsigned assertions are rejected. OIDC is the
path that is tested.

SCIM 2.0 Users and Groups support create, read, replace, patch and
delete, with `userName eq` filters and bearer authentication. Patch
covers the operations Entra and Okta use for active state, names and
membership. It is not a claim of full RFC 7644 coverage.

Portal roles are Owner, Admin, Policy Editor, Approver, Auditor and
Viewer. Groups or app roles from the IdP map onto them. Auditors can
read and export the audit log. Viewers can read the catalogue and
dashboard and cannot export full logs.

### Change control

Policy, DLP, route and catalogue writes create a revision: actor, time,
a field diff, and the previous document. When second-person approval is
on, the revision stays pending until a different user approves it.
Rollback writes another revision that restores the previous document.
The kill switch is the exception: it is written straight away.

The running store is SQLite, including for a single node. `POSTGRES_SCHEMA`
is the same tables, with a trigger function that rejects audit updates
and deletes. A `postgres://` URL is refused. There is no Postgres query
adapter in this cut. GitOps export and import is a JSON bundle of the
active documents. Use-case rows import and export as CSV.

### Approvals and the use-case register

A request for an MCP server, tool or model records a business owner,
purpose, data classification and expiry. Until it is approved, the
enforcement point denies it. Approval writes the CEL allow list and a
DLP route rule. Export and import use the fields in the DTA Standard
for accountability 2.0 (page last updated 2 December 2025,
https://www.digital.gov.au/ai/ai-in-government-policy/accountability).
The field names follow that page and the labels in
`jusso-dev/crownguard` `src/engine/aiRegister.ts`, which splits the
accountable official into name and email. CSV and JSON use those keys.

### DLP

Recognisers run on LLM request and response text and on MCP tool
arguments and results.

Checksums that have a published algorithm:

- ABN, modulus 89, weights 10, 1, 3, 5, 7, 9, 11, 13, 15, 17, 19 after
  subtracting 1 from the first digit
  (https://abr.business.gov.au/Help/AbnFormat).
- ACN, modified modulus 10, weights 8, 7, 6, 5, 4, 3, 2, 1
  (ASIC Datastream Messages Specification 2.80, Appendix B).
- Medicare card check digit, weights 1, 3, 7, 9, 1, 3, 7, 9 on the
  first eight digits, first digit 2 to 6, tenth digit issue number
  (ADHA conformance profile, Appendix E, hosted by the Australian
  Digital Health Agency). This is not a Services Australia consumer
  page. The code says that.
- ICAO Doc 9303 MRZ check digit, weights 7, 3, 1. Used when a machine
  readable line is present. A printed passport number without a check
  digit is format-only.

Format only, because no official checksum was found:

- TFN, eight or nine digits next to a TFN label. No checksum.
- CRN, nine digits and a trailing letter. No checksum.
- BSB, six digits, hyphen optional, next to a BSB label. Account
  numbers next to it are six to ten digits and are not checksummed.
- Australian phone numbers from the ACMA numbering plan (geographic,
  mobile, 1300/1800/13/180, and 09).
- Driver licences. Victoria's "usually nine digits" is the only
  official length statement used. Other states are labelled-field
  matches only, with the citation gap recorded in the module.
- Australian addresses are a heuristic (state token plus postcode).
  They are not a geocoder.

PSPF markings and caveats are detected in text and in a metadata map.
The operational order used for "and above" is documented as Magpie's
order, aligned to the Style Manual and to Policy 8's "or higher"
wording. OFFICIAL and UNOFFICIAL are not treated as security
classifications.

Classification routing, as policy:

- OFFICIAL: Sensitive and above may go only to a provider whose region
  resolves to Australia.
- PROTECTED and above is blocked unless the destination is on the
  approved list.

Rule actions are allow, log, redact, mask, tokenise, block and require
approval. The portal shows hits with the matched text replaced.
Tokenise is a keyed HMAC placeholder. It is not a reversible vault.

Secrets checks look for private keys, cloud access-key shapes, and
common token prefixes in tool arguments and outputs.

Presidio is called when `presidio.analyzerUrl` is set. Images are the
community ones under `ghcr.io/data-privacy-stack`. Microsoft's registry
is not referenced. Presidio entities are merged with the in-process
hits. If Presidio is down, the in-process recognisers still run and
the failure is audited. The default is off, so a stock install makes
no outbound DLP call.

### Sovereignty

Each provider and route has a region tag. Known Australian regions
include `ap-southeast-2`, `ap-southeast-4`, `australiaeast`,
`australiasoutheast`, `australiacentral`, `australiacentral2` and an
explicit `on-prem-au` label. Sovereign mode denies anything that does
not resolve to Australia. The portal shows the residency of each
provider. Telemetry, Presidio and audit sinks are off unless the
config names an endpoint.

### Audit

Events cover requests, decisions, DLP hits, policy changes, logins and
approvals. Each event has the user, agent, tool, model, decision and
policy reason. The SQLite table rejects updates and deletes with
triggers. Full payloads are off by default. Retention is a setting,
enforced by a prune function the operator schedules. It does not
pretend the database is a WORM device.

Sinks build the request body for OTLP logs, Microsoft Sentinel Logs
Ingestion, Splunk HEC, an OCSF object for S3 or Security Lake, and an
RFC 5424 syslog line. Delivery is attempted only when that sink is
configured. Tests check the payload shape against a local recorder.

Detection content in `detections/` is written for this schema. The
public pack at `yumaitau/ai-agent-threat-detection` does not contain a
tool-call spree query. Its closest burst rule is Microsoft `D14`, which
counts WAF hits in a 10 minute bin (40 hits, 8 rules, 15 URIs), and
`D15`, a draft hunt for new MCP destinations. Magpie's tool-spree query
uses that 10 minute window and a distinct-count idea, with its own
starting thresholds of 100 calls or 15 distinct tools, because a WAF
rule is not a tool call. The other five queries are new.

### Prompt injection and tool safety

Scanners are a trait. The built-in scanner is a pattern list and is
described as a baseline, not a model. A webhook scanner posts the text
to an operator URL. A self-hosted model scanner posts OpenAI-compatible
chat or moderation JSON to an operator URL. Llama Prompt Guard 2 is
documented, not downloaded. Per-tool rate limits, an argument allow
list and a small JSON Schema subset (type, enum, required,
additionalProperties) are enforced in the same decision. The global
and per-agent kill switches are rows the portal updates, and the next
decision reads them.

### Deployment and evidence

Compose runs the portal and agentgateway. Helm runs the portal.
Compose can start Presidio as a profile, using the image's own uid
1001. They do not run Postgres.
The portal refuses a `postgres://` URL. Portal containers are
non-root, with a read-only root filesystem. Terraform examples cover
ECS on AWS `ap-southeast-2` and a container app on Azure Australia
East. CI builds the portal image, writes a CycloneDX SBOM and scans
it. A push to `main` publishes the image to GHCR and signs the digest
with keyless cosign. No signing key is stored. The Windows guide
cites current Claude Code, Codex and VS Code docs for the exact
paths. It does not invent an Intune profile name where the vendor
has not published one.

The monthly report is CSV and a small PDF. Each row maps a Magpie
control or an observed count to the ISM (September 2026), the
Essential Eight (November 2023 model, still the current FAQ), and the
DTA policy 2.0 plus the agentic addendum. Rows say whether the link is
direct or interpretive. `docs/threat-model.md` and
`docs/data-handling.md` cover the gateway itself.

## Tests

Library tests cover checksums, markings, DLP actions, sovereignty,
CEL generation against a sample request, approvals, the register
round trip, immutable audit, sessions, SCIM, revisions and rollback.
Portal tests cover OIDC login against the mock IdP, a denied then
approved tool call, and DLP masking. Playwright loads every portal
page and runs axe against WCAG 2.2 AA.

## Deliberate limits

- No gRPC ExtMcp implementation. MCP enforcement is the shared engine
  behind a JSON check, plus CEL the portal generates for the proxy.
- No SAML login. The endpoint returns 501.
- No TFN or CRN checksum, and no invented driver-licence formats.
- Tokenisation is not reversible.
- Presidio is not started unless configured. Australian checksums do
  not depend on it.
- Prompt Guard weights are not shipped.
- The upstream React UI is unchanged. Operators who only need gateway
  config keep using it. The Magpie portal is the control plane for
  identity, approvals, DLP and audit.
