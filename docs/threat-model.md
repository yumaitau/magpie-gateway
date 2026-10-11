# Threat model

This note covers Magpie Gateway: the portal, the guardrail webhook, and
the way it attaches to an unmodified agentgateway proxy. It is a design
note, not a penetration test. Checked against the tree on 9 October 2026.

## What we are protecting

- Requests and responses that contain Australian identifiers, secrets or
  protective markings.
- The decision about which tool, model and region is allowed.
- The audit record of those decisions, and of portal logins and policy
  changes.
- Portal sessions and the SCIM directory.

## Trust boundaries

1. The user and the agent, which we do not trust. Tool arguments and
   model text are untrusted input.
2. The proxy, which we trust to send the webhook and to enforce the CEL
   allow list it was given. Magpie does not sit in the proxy process.
3. The identity provider, which we trust to authenticate people. The
   portal checks the token it is configured to check. The mock identity
   provider is for tests and must be off in a deployment.
4. Presidio, a prompt-injection webhook, and audit sinks. They are off
   until an operator sets a URL. A URL is an outbound trust decision.
5. The SQLite file and anyone who can read the disk. Triggers stop the
   application updating or deleting audit rows. They do not stop a
   person with the file.

## What we assume

- TLS is terminated in front of the portal and the proxy. The Compose
  and Helm examples do not ship a certificate.
- Outside demo mode the process exits if the session secret is missing
  or is `change-me`, `dev-only-change-me`, or `magpie-example-key`.
  Demo mode may omit it and then uses `dev-only-change-me`.
- Break-glass stays off except during an identity-provider outage, and
  that use is reviewed.
- Sovereign mode is on for clients who need Australian residency.
- The generated CEL is actually attached to the proxy. Without it, an
  empty proxy policy allows every tool. The portal's own check still
  denies unapproved tools when the caller uses the webhook or `/mcp/check`.

## Abuse we try to limit

| Threat | What Magpie does | What it does not do |
| --- | --- | --- |
| An agent calls a tool nobody approved | The decision denies it, and the generated CEL is `false` or an allow list | It does not see a call that bypasses the proxy |
| A prompt tells the model to ignore its instructions | The baseline scanner blocks a few phrases on tool output and on responses. It is not a model | It is not Llama Prompt Guard. Weights are not shipped |
| A tool argument contains a TFN, ABN or a private key | In-process recognisers redact, mask, tokenise or block. Presidio is optional | TFN candidates use labelled length or a non-authoritative Bower-compatible heuristic; CRN uses labelled format. Tokenise is keyed and not reversible |
| A marked document is sent offshore | Sovereign mode and the marking check block the destination | The region comes from configuration and headers. A wrong tag is a wrong decision |
| A portal user changes policy alone | Second-person approval can hold the change | The kill switch is immediate, on purpose. The person who wrote a change cannot approve it |
| Someone edits the audit log | SQLite triggers abort updates and deletes | A database file can still be replaced. Retention counts old rows and does not delete them |
| A stolen session cookie | Idle and absolute timeouts, HttpOnly, SameSite=Lax, Secure when configured, back-channel logout | There is no device binding |

## Things we are explicit about

- SAML is not a login method. The route returns 501. Unsigned assertions
  are not accepted because they are not parsed as a session.
- There is no gRPC ExtMcp server. MCP enforcement on the proxy is the
  CEL list. The JSON check is the same engine, for a caller that can
  reach the portal.
- The portal's demo mode renders pages without a session and refuses
  changes. Do not enable it on a network that is not yours.
- A gateway does not, by itself, meet an ISM control or an Essential
  Eight strategy. See `docs/compliance.md`.
