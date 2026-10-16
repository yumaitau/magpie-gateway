# Subscription pools

Open **Subscription pools** in the native Magpie console. A user with **Configure
gateway providers** permission can create a private pool, connect subscriptions,
and share the pool with signed-in colleagues. Only that pool's owner, while they
retain provider-edit permission, can change its subscriptions or sharing. Each
colleague can issue and revoke their own API keys for an accessible pool. Users
with **Manage gateway API keys** permission can also revoke other keys in pools
they can see.

Pool access requires catalogue-read, provider-edit, or key-management permission.
Signing in with an unassigned account grants no pool access.

Connect Claude using a subscription token generated locally with
`claude setup-token`. Connect ChatGPT using the contents of `~/.codex/auth.json`
after a local `codex login`; API-key logins are refused. Credentials are imported
only through the authenticated console; the service never reads local CLI logins.
Choose the models that subscription can use from the synced model catalogue
(Anthropic models for Claude, OpenAI models for Codex), or add an exact model ID
the catalogue lacks. The provider checks model entitlements when a request
arrives. A repeated credential cannot be connected
twice; Codex accounts are also deduplicated by ChatGPT account ID.

Choose explicit models and a lifetime of 1–365 days when issuing a key. The raw
key is returned once, with `Cache-Control: no-store`; only its SHA-256 hash is
saved. Keys grant inference access to that pool and its selected models, with
no access to management APIs or provider credentials. They are separate from the
main gateway's Virtual API Keys. Making a shared pool private invalidates access
for colleagues' keys; disabling the pool stops all new requests. Pool names are
unique, ignoring case. The owner can delete a pool once no requests are in
flight; deletion removes its subscriptions and every key issued for it. Revoking a key,
expiring it, or disabling its owner's portal account prevents new requests.
Already-started requests may finish.

Each request records a `forward` audit event when it is assigned to a
subscription, and a `complete` event with the provider's token usage once the
response finishes. Both carry the client's session header and `traceparent`,
so they can be exported with the [OpenTelemetry audit export](audit-otel.md).

## Client configuration

The pool service uses the portal's public origin, with **`/pool/v1`** as its API
base. Existing deployments route this prefix to the portal without changing
the main gateway's `/v1` routing. In Compose, use the portal's exposed port.

Supported contracts:

| Endpoint | Contract |
| --- | --- |
| `GET /pool/v1/models` | Models exposed by this key and enabled subscriptions; OpenAI list fields and Anthropic pagination fields |
| `GET /pool/v1/models/{id}` | A model visible to this key |
| `POST /pool/v1/messages` | Anthropic Messages for Claude subscriptions, including tools and SSE |
| `POST /pool/v1/messages/count_tokens` | Anthropic token counting |
| `POST /pool/v1/responses` | OpenAI Responses for ChatGPT/Codex subscriptions, including tools and SSE |

Authenticate with `Authorization: Bearer <pool-key>` or `x-api-key: <pool-key>`.
Conflicting credentials are refused. Claude models use Messages; Codex models
use Responses. The service does not translate one provider's model into the
other provider's wire format, and does not expose Chat Completions.

For Claude Code, set the key in your secret store as `MAGPIE_POOL_KEY`, then:

```sh
export ANTHROPIC_BASE_URL="https://magpie.example.com.au/pool"
export ANTHROPIC_AUTH_TOKEN="$MAGPIE_POOL_KEY"
claude --model MODEL_ID_FROM_YOUR_POOL
```

Remove any competing `ANTHROPIC_API_KEY` from that client's environment; sending
two different keys is refused. Set an explicit model that your pool key allows.
If your Claude client selects a separate small model, include that model in the
pool and key as well, or configure it to use an allowed model.

For Codex, set `MAGPIE_POOL_KEY` in the client's environment and add to
`~/.codex/config.toml`:

```toml
model = "MODEL_ID_FROM_YOUR_POOL"
model_provider = "magpie_pool"

[model_providers.magpie_pool]
name = "Magpie subscription pool"
base_url = "https://magpie.example.com.au/pool/v1"
env_key = "MAGPIE_POOL_KEY"
wire_api = "responses"
```

The ChatGPT subscription backend requires `store: false` and streaming upstream.
The pool normalises a string `input` to a user message, supplies empty
`instructions` when absent, and returns the completed Responses object to a
non-stream client. Responses are stateless: send conversation history in `input`.
Requests using `store: true`, background execution, `previous_response_id`,
`conversation`, `temperature`, `top_p`, or `max_output_tokens` are refused with
a contract-shaped error. Provider-specific restrictions and model entitlements
still apply.

## Scheduling and enforcement

Selection prefers the least busy subscription, then the least recently used
one, restricted to the requested provider and model. Claude subscriptions allow
1–10 parallel requests; Codex uses one slot so rotating refresh tokens cannot
race. A SQLite lease holds the slot until a response completes, fails, or the
client disconnects. After a crashed process, abandoned leases expire after
11 minutes. Provider calls have a ten-minute timeout.

HTTP 429 responses put that account on cooldown and try another eligible
subscription before returning any provider output. Usage headers for Codex's
primary/secondary windows and Claude's five-hour/weekly windows also suppress
exhausted accounts until reset. A retry without a reset hint waits at least a
minute. Disabling/re-enabling or editing a subscription preserves its cooldown.
The console shows slots and the observed cooldown; it does not poll provider
usage or claim to combine, extend, or transfer provider quotas. An exhausted or
busy pool returns HTTP 429 with `Retry-After`.

Codex access tokens refresh before expiry and once on an upstream 401, with
rotated credentials persisted. Transient refresh failures wait a minute;
rejected logins require reconnecting. Claude setup-tokens must be reconnected
when they expire or are revoked. Wait for active requests to finish before
replacing credentials.

Every request runs Magpie governance using the key owner's identity, key ID,
requested model and a global provider destination. Client-supplied Magpie
identity or region headers are ignored. The `x-magpie-marking` classification
header is preserved and can raise the request's required protection level.
In enforcement mode, approve the model
and permit the global destination in the organisation's residency settings.
Sovereign mode blocks these global destinations. Trial mode follows the same
existing group enforcement rules as other gateway traffic.

JSON responses are checked before release. SSE events are checked before
forwarding, including accumulated text/tool deltas so splitting a value across
events does not evade DLP. A blocked stream ends with an error event; previously
delivered text cannot be retracted. Structured responses are refused rather than
rewritten when a policy requires redaction. Governance audit entries retain
findings and decisions, without storing matched values or complete payloads.
Request bodies are limited to 2 MB and buffered/provider responses to 32 MB.

Pool credentials, key hashes and leases live in separate SQLite tables, outside
policy documents, exports and revision history. Credentials use authenticated
ChaCha20-Poly1305 encryption, bound to the subscription ID, with a key derived
from `MAGPIE_SESSION_SECRET`. Keep that deployment secret stable and protect
database backups. Changing it requires reconnecting subscriptions. Management
audit entries contain resource IDs and actions, never credentials or raw keys.

Tests use local provider mocks; they do not consume or verify a live subscription.
