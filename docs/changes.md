# Magpie Gateway changes

Upstream agentgateway is unmodified apart from the fork notice in
`README.md` (import commit). Product changes are additions:

| Path | What it is |
| --- | --- |
| `NOTICE` | Attribution and the list of Yuma IT additions |
| `docs/plan.md` | Architecture and the change plan |
| `docs/upstream-sync.md` | How to merge `upstream/main` |
| `docs/UPSTREAM` | Pinned upstream commit |
| `docs/changes.md` | This list |
| `magpie/` | Policy library and the admin portal |

The portal crate is `magpie/crates/magpie-portal`. It serves the pages,
the mock identity provider, SCIM and the guardrail webhook. Deployment
examples, detection content and the Magpie CI workflow are added in
later commits.
