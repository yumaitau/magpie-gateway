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
A `postgres://` URL is refused; use a SQLite file or `:memory:`.

Build and test from the repository root:

```bash
cargo test --manifest-path magpie/Cargo.toml
cargo clippy --manifest-path magpie/Cargo.toml --all-targets -- -D warnings
```

See `docs/plan.md` for how this attaches to the proxy, and `NOTICE` for
licence and trademark limits. Prompt Guard weights are not included.
