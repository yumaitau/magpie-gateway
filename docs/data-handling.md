# Data handling

What the portal stores, where it lives, and how long it stays. This
describes the code in `magpie/`. It is not a privacy impact assessment.

## Store

The running database is SQLite. A single node uses a file, or
`:memory:` in tests. A `postgres://` URL is refused. `POSTGRES_SCHEMA`
is shipped so an operator can create the same tables. This build does
not run those queries.

| Data | Where | Default retention | Notes |
| --- | --- | --- | --- |
| Documents (agents, tools, providers, settings, approvals, policies) | `documents` | Until replaced or rolled back | GitOps export is this table |
| Revisions | `revisions` | Kept | Actor, before, after, status |
| Audit events | `audit_events` | Setting, default 365 days | Updates and deletes raise an error. The prune function counts old rows and does not delete them |
| Sessions | `sessions` | Until logout, idle timeout, absolute timeout, or back-channel logout | The cookie is HMAC signed. The server keeps a row so logout can revoke it |
| SCIM users and groups | Process memory | Until restart | Not written to SQLite in this cut |
| PKCE state and mock codes | Process memory | Until the callback | Lost on restart |
| DLP matched text | Not stored in the hit shown to the UI | n/a | The hit preview is the replacement. Payload capture is off |
| Presidio request | Sent only when the analyzer URL is set | Their retention, not ours | Default URL is empty |
| Telemetry | Not sent | n/a | Off unless a sink URL is set |

## What a decision record contains

Each decision stores the time, kind, actor, agent, tool, model,
decision, reason, and a detail object with the phase, destination,
region and hit metadata. It does not store the raw identifier unless
`capturePayloads` is turned on. That switch is off in the seed data and
the settings form does not turn it on.

## Tokenisation

`tokenise` writes `TOK_<KIND>_<hex>` from HMAC-SHA256. The key is the
session secret, or `magpie-example-key` if the key is empty. It is not
a vault and it cannot be reversed by the portal. Do not treat the token
as a protected copy of the identifier. Prefer redact or mask.

## Residency of the portal itself

The portal does not choose a region for its own disk. Put the SQLite
volume, the container, and any sink in Australia if the client requires
that. The provider residency shown in the UI is the region tag on the
provider record, not a live lookup of the model host.

## Example data

`MAGPIE_SEED_EXAMPLES=1` inserts synthetic rows: Alex Nguyen, Sam Patel,
Casey Morgan, `example_search`, `example-au-model`, and an example ABN
that passes the published modulus. They are not clients. Demo mode
prints that on each page.
