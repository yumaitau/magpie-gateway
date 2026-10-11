# Automatic model suggestions

Magpie fetches the public [models.dev catalog](https://models.dev) on startup and
hourly while the gateway console is enabled. This is also a catalog source used
by [OpenCode](https://opencode.ai/v2/docs/models). No provider credentials or
local coding-tool sessions are sent to this source. New entries become searchable
in outgoing model, wildcard model, playground and client setup selectors.

Open **Models → Model catalog** to see freshness and errors. Users with **Edit
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
