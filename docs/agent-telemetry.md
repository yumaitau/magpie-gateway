# Agent telemetry with OpenTelemetry

Magpie receives OpenTelemetry from Claude Platform, Claude Code, Codex and Grok
Build, and records each span or log event as a `telemetry` audit event. Agent
activity that does not pass through the proxy, such as Claude Managed Agents
sessions or a developer's local Codex, then sits in the same audit log, CSV
export and [OTLP audit export](audit-otel.md) as gateway decisions.

## Turn it on

Set `MAGPIE_OTLP_TOKEN` on the portal to a long random value, for example
`openssl rand -hex 32`, and keep it in your secret store. With Helm, pass it as
`--set secrets.MAGPIE_OTLP_TOKEN=...`. Without it the receiver answers 404.

The portal serves OTLP/HTTP at:

| Path | Signal |
| --- | --- |
| `/otlp/v1/traces` | Spans, stored |
| `/otlp/v1/logs` | Log events, stored |
| `/otlp/v1/metrics` | Accepted and discarded, so agents that export metrics beside logs do not report failures |

Every request needs `authorization: Bearer <MAGPIE_OTLP_TOKEN>`. Bodies can be
protobuf (`application/x-protobuf`) or JSON (`application/json`), optionally
gzip-compressed, up to 16 MiB. The Helm ingress already sends `/otlp` to the
portal.

## Claude Platform (Claude API and Managed Agents)

The Claude API sends traces of Messages API requests and Managed Agents
sessions to a destination you add in the Console. See
[OpenTelemetry for the Claude API](https://platform.claude.com/docs/en/manage-claude/opentelemetry).

1. Open the [OpenTelemetry settings page](https://platform.claude.com/settings/telemetry-destinations) as an organisation admin and click **Add destination**.
2. **Provider:** Custom.
3. **OTLP endpoint URL:** `https://magpie.example.com.au/otlp`. The API adds
   `/v1/traces`. It must be HTTPS on port 443 with a hostname.
4. **Request headers:** `authorization` = `Bearer <MAGPIE_OTLP_TOKEN>`.
5. **Content mode:** Metadata only. Magpie drops prompts and responses either
   way, so Full content only adds transfer.
6. Click **Test**, then check **Audit log** with kind `telemetry` for the
   `anthropic.test` span.

Or with the Admin API:

```bash
curl --fail-with-body -sS "https://api.anthropic.com/v1/organizations/telemetry_destinations" \
  -H "x-api-key: $ANTHROPIC_ADMIN_KEY" \
  -H "anthropic-version: 2023-06-01" \
  -H "anthropic-beta: telemetry-destinations-2026-08-11" \
  -H "content-type: application/json" \
  -d '{
    "endpoint_url": "https://magpie.example.com.au/otlp",
    "headers": {"authorization": "Bearer <MAGPIE_OTLP_TOKEN>"},
    "traces": {"content_mode": "metadata_only"},
    "workspaces": {"type": "all"}
  }'
```

Claude Platform spans carry no user identity, so the actor is
`workspace:<workspace ID>`. Token usage is kept only on the spans Anthropic says
to sum (the `SERVER` `anthropic.messages.create` span, `anthropic.compaction`,
`anthropic.session.outcome_evaluation`, and `anthropic.model_request` in
sessions), so totals count each token once. Every span's own usage attributes
stay in the event detail.

## Claude Code

Set these in the managed settings `env` block or the shell. See
[Claude Code monitoring](https://code.claude.com/docs/en/monitoring-usage).

```bash
export CLAUDE_CODE_ENABLE_TELEMETRY=1
export OTEL_LOGS_EXPORTER=otlp
export OTEL_EXPORTER_OTLP_PROTOCOL=http/protobuf
export OTEL_EXPORTER_OTLP_ENDPOINT=https://magpie.example.com.au/otlp
export OTEL_EXPORTER_OTLP_HEADERS="authorization=Bearer <MAGPIE_OTLP_TOKEN>"
```

Events such as `claude_code.api_request` and `claude_code.tool_result` are
recorded against `user.email`. Claude Code reports cache reads and writes
beside `input_tokens`; Magpie adds them, so input includes cached tokens as on
every other event.

## Codex

Add an `[otel]` section to `~/.codex/config.toml`, or to the managed
configuration you push to developers. Codex does not append the signal path,
so give the full URL. See
[Codex advanced configuration](https://developers.openai.com/codex/config-advanced).

```toml
[otel]
environment = "prod"
log_user_prompt = false
exporter = { otlp-http = { endpoint = "https://magpie.example.com.au/otlp/v1/logs", protocol = "binary", headers = { "authorization" = "Bearer <MAGPIE_OTLP_TOKEN>" } } }
```

`codex.sse_event` events with `event.kind` `response.completed` carry token
usage. `codex.tool_result` records the tool, success and duration; Codex sends
the full tool arguments and output, which Magpie drops.

## Grok Build

xAI's terminal agent, [Grok Build](https://github.com/xai-org/grok-build),
exports logs and metrics over OTLP/HTTP protobuf. It sends no traces.

```bash
export GROK_EXTERNAL_OTEL=1
export OTEL_LOGS_EXPORTER=otlp
export OTEL_EXPORTER_OTLP_PROTOCOL=http/protobuf
export OTEL_EXPORTER_OTLP_ENDPOINT=https://magpie.example.com.au/otlp
export OTEL_EXPORTER_OTLP_HEADERS="authorization=Bearer <MAGPIE_OTLP_TOKEN>"
```

Events such as `grok_code.api_request` and `grok_code.tool_result` are recorded
with provider `x_ai`. Grok's telemetry is alpha, so names may change. Spans from
the xAI SDK (`gen_ai.provider.name` `xai`) are also recorded as Grok.

## What is recorded

| Audit field | Source |
| --- | --- |
| Kind, decision | `telemetry`, `log` |
| Actor | `user.email`, `enduser.id`, `user.id`, `user.account_uuid` or `user.account_id`; else `workspace:<id>`; else the agent |
| Agent | `gen_ai.agent.id` or `gen_ai.agent.name`; else `claude-platform`, `claude-code`, `codex`, `grok` or `otlp` |
| Model | `gen_ai.response.model`, `gen_ai.request.model` or `model` |
| Tool | `gen_ai.tool.name` or `tool_name` |
| Reason | Agent and event name, with `failed` when the span status is error, `error.type` is set or `success` is false |
| Conversation | `gen_ai.conversation.id`, `conversation.id`, `session.id` or `anthropic.session.id` |
| Trace | The span's or log record's trace and span IDs |
| Tokens | `gen_ai.usage.*`, `input_tokens`/`output_tokens` or `input_token_count`/`output_token_count` |
| Detail | Span kind, status, duration, and the remaining metadata attributes |

Magpie never stores prompts, responses, tool arguments, tool output or log
bodies. Known content attributes (for example `gen_ai.input.messages`,
`prompt`, `arguments`, `output`, `full_command`) and any value over 1 KB are
dropped, and their keys are listed in the detail's `contentOmitted`. Identity
and tool fields are reported by the client, so treat them as attribution, not
authentication: the token proves only that the sender was configured by you.

Records from Magpie's own audit export (`service.name` `magpie-gateway`) are ignored, so
pointing the export at this receiver cannot loop.
