# Automatic model suggestions

Magpie fetches the public [models.dev catalog](https://models.dev) on startup and
hourly while the gateway console is enabled. This is also a catalog source used
by [OpenCode](https://opencode.ai/v2/docs/models). No provider credentials or
local coding-tool sessions are sent to this source. New entries become searchable
in outgoing model, wildcard model, playground and client setup selectors.

Open **Models → All catalog models** to search every synced provider, including
providers you have not connected yet. Search accepts names such as “Opus 5.5”
and “Codex”. Use **Configure** to start a model draft, or **Configured routes**
to view existing routes. Drafts reuse matching shared provider connections and
choose an unused gateway name; nothing is activated until you save.

Expand **Model catalog** to see freshness and errors. Users with **Edit
gateway settings** and **Read gateway configuration** can use **Sync now**.
Concurrent syncs are coalesced, with at most one source request per minute.
Automatic failures retry after five minutes. Responses have a 30-second deadline
and a 64 MiB limit. Invalid, empty or incomplete responses leave the last good
snapshot intact. That snapshot is stored in the portal SQLite database and
survives restarts. Bundled/custom suggestions remain available as well.

Anthropic, OpenAI, xAI, Copilot, OpenCode Zen, OpenCode Go and other source
providers retain their own model IDs. Compatible endpoints are matched by exact
hostname; unknown/custom endpoints expose a catalog-provider selector. A model
can always be entered manually. The source may lag a provider's rollout, and
subscription-only coding models may not be offered through API credentials.

Syncing suggestions does not create routes, grant access, approve models, change
cost rates or alter the proxy's `/v1/models` response. Configure an explicit route
or select a concrete model on an existing wildcard route, obtain required
approval and test the connection. Coding tools which cache a model list may need
updated Client Setup configuration. The built-in Help guide covers these steps.

## Try a prompt

A shared **Model** picker in the playground, Getting Started, Client Setup and
weighted/failover/conditional virtual routing searches configured routes and every synced
catalog entry. “Opus 5.5” matches punctuation variants; “Codex” also searches
OpenAI entries. The menu renders the first 100 matches to stay responsive; refine
the search to reach any entry. A matching explicit route takes precedence over a
provider wildcard. Choosing a catalog model on a wildcard fills its concrete
request name. Entries without a matching route show **Setup required** and link
to a prefilled draft; sending remains disabled until a configured route is selected.
Access, keys and approval checks still run at the gateway.

## Machine setup and rollout

**Getting started & help → Get your first model working** includes **Download
prompt.md** and **Download rollout.md** under **Options and downloads**. The
primary action is **Copy bootstrap prompt**, with the short prompt visible and
**Let my agent choose** selected by default. No settings need to be filled in
before copying. The optional model picker searches the full catalog. An
unconfigured choice is included as a setup requirement, never misrepresented as
a working route. Client and gateway overrides are optional; credentials are not
exported.
Give it to a local coding agent to detect the OS/client version, merge settings,
obtain a device credential locally, test the client and explain rollback.

The rollout guide describes a central gateway, versioned client settings,
separate device/user credentials, a pilot for each OS/client, managed distribution,
verification, rotation and rollback. It is a setup aid and rollout plan, not a
fleet enrollment service or a prebuilt Jamf/Intune/Ansible package. Packaging must
use the organisation's actual endpoint platform and supported client versions.

## Model-picker audit

The request/setup selectors share GatewayModelPicker rather than independently
building configured-only lists. CatalogModelSelector already supplies synced
provider IDs to outgoing-model editors and wildcard fields. Provider aliases
(Bedrock, Gemini and Vertex) use the same catalog identity when matching routes.
Unconfigured selections block client recipes and virtual target saves, preventing
a stale previous model from silently being used.

Read-only configuration dumps describe deployed resources; log filters describe
observed/configured traffic; API-key patterns refer to gateway request names.
Those are intentionally scoped inventories, not provider model catalogs. Key
patterns and moderation model IDs also accept manual values without truncation.
