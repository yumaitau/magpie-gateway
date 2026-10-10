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
A `postgres://` URL is refused; use a SQLite file or `:memory:`.

Build and test from the repository root:

```bash
cargo test --manifest-path magpie/Cargo.toml
cargo clippy --manifest-path magpie/Cargo.toml --all-targets -- -D warnings
```

See `docs/plan.md` for how this attaches to the proxy, and `NOTICE` for
licence and trademark limits. Prompt Guard weights are not included.
