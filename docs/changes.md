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
| `deploy/` | Portal image, Compose, Helm, and AWS and Azure examples |
| `detections/` | KQL and Athena SQL for Magpie audit events |
| `docs/threat-model.md` | Threat model for the gateway and portal |
| `docs/data-handling.md` | What is stored, where, and for how long |
| `docs/windows-endpoint.md` | Intune, Defender and App Control notes |
| `docs/compliance.md` | How the monthly evidence report is produced |
| `.github/workflows/magpie.yml` | Test, clippy, axe, image, SBOM and scan |

The portal crate is `magpie/crates/magpie-portal`. It serves the pages,
the mock identity provider, SCIM and the guardrail webhook.
