# Magpie Gateway policy workspace

This workspace is Yuma IT's addition. It does not modify the agentgateway
proxy, controller or admin UI. The library in `crates/magpie` holds the
Australian DLP checks, approvals, sessions and audit records. The portal
crate, when present, serves the admin pages and the guardrail webhook.

The portal listens on `127.0.0.1:8088` unless `MAGPIE_LISTEN` is set.
`MAGPIE_DEMO=1` renders the pages with a synthetic viewer and an
example-data banner. `MAGPIE_MOCK_IDP=1` serves the local identity
provider used by the tests. `MAGPIE_SEED_EXAMPLES=1` loads the example
catalogue. Telemetry stays off unless a setting names an endpoint.

Sign-in: `MAGPIE_ADMIN_PASSWORD` (16 characters or more) enables a local
`admin`. `MAGPIE_ENTRA_TENANT_ID` or `MAGPIE_OIDC_ISSUER`, with
`MAGPIE_OIDC_CLIENT_ID` and `MAGPIE_OIDC_CLIENT_SECRET`, enables single
sign-on; set `MAGPIE_PUBLIC_URL` behind a proxy. `MAGPIE_ROLE_OWNERS`,
`_ADMINS`, `_POLICY_EDITORS`, `_APPROVERS`, `_AUDITORS` and `_VIEWERS`
map IdP group IDs or app-role values to portal roles. The portal will
not start without one way to sign in, unless `MAGPIE_DEMO=1`.
`MAGPIE_WEBHOOK_TOKEN` makes the guardrail and MCP check routes require
the same value in `x-magpie-webhook-token`. `MAGPIE_BOOTSTRAP_MODELS`
(`name=region,...`) approves the deployment's models on first start.
`MAGPIE_APPROVED_PROTECTED` (`bedrock:model-id,...`) adds destinations
allowed for PROTECTED traffic, merged into settings on every start.
A `postgres://` URL is refused; use a SQLite file or `:memory:`.

## Log-only trials

Set `MAGPIE_GUARDRAIL_MODE=log-only` to evaluate policy without blocking or
rewriting LLM requests, LLM responses or `/mcp/check` traffic. The default is
`enforce`; unknown values stop startup. This is an initial default only.
Use **Settings → Guardrail mode → Apply mode** to switch between log-only
and enforcement immediately, without redeploying. The saved choice survives
restarts. The Change enforcement mode permission controls access to this
action. Settings shows the current mode and all portal pages show a banner
during a trial. Policy imports cannot overwrite it.

Audit decisions are `log`, with the would-be policy action in
`detail.wouldAction`, the mode in `detail.mode`, and the findings and reason
preserved. Matched values and full payloads are not added to the audit log.
The normal first matching rejection still ends policy evaluation. A would-be
`redact`, `mask` or `tokenise` action on an LLM request becomes a block in
enforcement mode, as documented below. In log-only mode the original traffic
passes through, including identifiers and secrets.

Log-only covers approvals, sovereignty, DLP, injection checks, tool limits,
argument validation and kill switches. Authentication and portal permissions
still apply. Proxy-level CEL/MCP allow lists and upstream provider restrictions
are independent: the Compose MCP example still enforces its static allow list
and does not call `/mcp/check`. Review those separate controls before an MCP
trial. Webhook transport failures also retain the proxy's configured failure
mode; the engagement proxy remains fail-closed if the portal is unavailable.

To roll out enforcement, select **Enforcement** in Settings and click **Apply
mode**. Changes are audited and take effect for new requests immediately.
In-flight requests finish using the mode they started with. Policy rules and
approvals are kept. `MAGPIE_GUARDRAIL_MODE` (Helm: `portal.environment`, engagement:
`GUARDRAIL_MODE` in `magpie.conf`) only seeds a database with no saved mode.

## Access control

Use **Access control** to create and edit roles with individual action grants,
assign roles to user groups, select users, and map identity-provider groups.
Users appear after their first sign-in. Group membership uses stable user IDs.
The built-in roles are Owner, Admin, Policy Editor, Approver, Auditor and Viewer.
New users have no permissions until assigned by user or identity-provider group.
Role/group changes apply to existing sessions. Disabling a user revokes their
sessions. Access changes and mode changes are persisted and audited atomically.
See [access-control.md](../docs/access-control.md) for permissions and defaults.

Build and test from the repository root:

```bash
cargo test --manifest-path magpie/Cargo.toml
cargo clippy --manifest-path magpie/Cargo.toml --all-targets -- -D warnings
```

See `docs/plan.md` for how this attaches to the proxy, and `NOTICE` for
licence and trademark limits. Prompt Guard weights are not included.
