# Syncing Magpie Gateway with upstream

Magpie Gateway is a private working copy of agentgateway. The import
merged `upstream/main` with `--allow-unrelated-histories`, so the
upstream commits stay in this repository and later syncs are ordinary
merges.

## Remotes

```bash
git remote add upstream https://github.com/agentgateway/agentgateway.git
git fetch upstream main --tags
```

`origin` is the private `yumaitau/magpie-gateway` repository. Do not
push this repository's branches to the public agentgateway remote, and
do not publish Magpie images or charts to a public registry.

## Where Magpie code lives

New work stays in paths upstream does not own:

- `magpie/` policy engine, portal and tests
- `deploy/` Helm, Compose and Terraform examples
- `detections/` queries written for the Magpie audit schema
- `docs/` product and compliance documents
- `.github/workflows/magpie.yml`
- `.github/workflows/upstream-sync.yml`
- `NOTICE`

The proxy, controller and admin UI under `crates/`, `controller/` and
`ui/` stay as imported. Magpie attaches to them through configuration
that already exists: LLM guardrail webhooks, MCP `mcpGuardrails`, CEL
authorisation, and the hybrid config store. That is the rebase
strategy. A sync should not need to replay edits inside those trees.

The one intentional divergence in an upstream file is the fork notice
at the top of `README.md`. On a conflict, keep upstream's README and
put the notice back above it.

## Day to day

Feature branches start from the latest Magpie integration branch, not
from a rewritten upstream history. Rebase those branches onto the
integration branch as usual. Do not rebase the original import merge.
It joins two unrelated histories, and rebasing it creates a second
copy of upstream.

## Taking upstream main

```bash
git fetch upstream main
git checkout <magpie-integration-branch>
git merge upstream/main
# resolve README.md if it conflicts, then run the Magpie tests
cargo test --manifest-path magpie/Cargo.toml
```

Record the new upstream commit in `docs/UPSTREAM`.

Prefer a merge commit over a rebase for this step. The merge commit is
the record that a particular upstream revision was absorbed. Tags from
upstream can be fetched and pushed to the private remote. They are
upstream's tags. Do not move them.

## What the weekly job does

`.github/workflows/upstream-sync.yml` fetches `upstream/main` and
attempts `git merge --no-commit --no-ff`. It does not push. A conflict
or a Magpie test failure fails the job, which is the signal to do the
merge by hand on a branch.

The job is a test of the sync, not an automated landing. Review the
merge before it reaches `main`.

## If a core change becomes unavoidable

1. Put the behaviour in `magpie/` first.
2. If the proxy must grow a hook, add the smallest field or callback
   and mark the file with a comment that names the Magpie commit:
   `// Magpie: <reason>. See docs/upstream-sync.md.`
3. List the file in `NOTICE` and in `docs/changes.md`.
4. Cover it with a test that fails if the hook disappears, so the next
   sync surfaces the conflict in CI rather than in production.

## Trademarks

Do not use the agentgateway name, logo or banner as the name or mark
of this product. Do not use Linux Foundation marks. Attribution in
`NOTICE`, `docs/UPSTREAM` and the README fork notice is a statement of
origin, not branding.
