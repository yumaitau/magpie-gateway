# Native Agentgateway console

The public root opens `/ui/` for a signed-in user with `readGatewayConfig`.
Other signed-in users land on the Magpie dashboard. The console is the upstream
React UI in `ui/`, with Magpie artwork, colours, governance navigation and an
identity/enforcement indicator. Models, providers, policies, guardrails, virtual
keys, costs, analytics, logs, client setup, chat, MCP, traffic, CEL, raw
configuration and native settings remain available. Features needing providers,
MCP targets or observability data still require their normal configuration.

The frontend is built with `VITE_MAGPIE=true` in the portal Dockerfile. Client
recipes and playgrounds then use the browser origin. The shared ingress sends
`/v1`, `/mcp` and `/a2a` to gateway port 4000. The longer `/mcp/check` and
`/guardrails` routes remain private fixed 404 responses. Route new native services
through the configured `default` gateway on port 4000; adding another internal
listener does not publish another internet port. No new CloudFront is required.

## Management access

The portal forwards only an explicit management API allowlist to
`MAGPIE_GATEWAY_ADMIN_URL` (loopback in EKS). Each request checks the current
session, disabled-user state and role grants. Native configuration reads are
sensitive: `readGatewayConfig` explicitly includes credential visibility.
Do not give this permission to ordinary model consumers. Configuration writes
check every changed section against individual model/provider/key/policy/MCP/
traffic/settings actions. Log reads and CEL have separate permissions. Request
cookies and authorization headers are not forwarded to the private admin API.
Cross-origin writes are refused, and JSON is required. Management actions are
audited without their configuration bodies or credentials.

Owner gains newly introduced actions during upgrade. Existing custom roles and
Admin grants are preserved; review and grant the new actions in Access control.
Changing role grants takes effect on the next request. A role with configuration
read access and no native edit actions receives the native read-only UI. A
partially privileged editor can encounter a permission error when attempting
an action outside its role; the API is authoritative.

## Persistence and upgrades

An init container copies the Helm baseline to `/data/gateway/config.yaml` only
when that file does not exist. Agentgateway reads/writes that file on the same
persistent volume as Magpie's database. UI changes survive pod replacement and
Helm upgrades. The admin, readiness and stats listener addresses remain managed
by deployment. The gateway working directory is `/data/gateway` so its local
catalogue/cache files can be written despite a read-only image filesystem.
Generated deployments also enable a local request-log database at
`/data/gateway/requests.sqlite`; prompt/completion capture is not enabled by
this change. Native observability settings remain separately configurable.

**After initialisation, changes to the generated ConfigMap are not automatically
merged into the live native configuration.** Use the native UI/API to change
models and API keys. `magpie.sh` key generation or model configuration changes
alone do not update that live file. For a deliberate baseline reset, back up the
live configuration and database, stop the deployment, replace the persisted
config with the reviewed baseline, then restart. Do not silently reset it during
a routine upgrade. Back up `/data/gateway` along with the portal database.

Development can set `MAGPIE_GATEWAY_UI_DIR` to `ui/dist`; if unset, the bridge
serves the running gateway's bundled UI as a fallback. The fallback has the
upstream endpoint defaults; the production image includes the public-origin
frontend. Update and test the UI and proxy together when upgrading upstream.
