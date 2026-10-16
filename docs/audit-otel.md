# Audit export with OpenTelemetry

Magpie sends its audit log to any OpenTelemetry collector or backend that
accepts OTLP/HTTP JSON logs. It uses the standard exporter variables and the
[OpenTelemetry GenAI semantic conventions](https://opentelemetry.io/docs/specs/semconv/gen-ai/),
so Magpie policy decisions sit next to Claude Platform, Claude Code and gateway
traces in the same tools.

## Configure

| Variable | Use |
| --- | --- |
| `OTEL_EXPORTER_OTLP_LOGS_ENDPOINT` | Full logs URL. Takes precedence. |
| `OTEL_EXPORTER_OTLP_ENDPOINT` | Base URL; Magpie appends `/v1/logs`. |
| `OTEL_EXPORTER_OTLP_LOGS_HEADERS` or `OTEL_EXPORTER_OTLP_HEADERS` | `name=value` pairs separated by commas, values percent-encoded, for example `authorization=Bearer%20<token>`. Keep the token in your secret store. |
| `OTEL_EXPORTER_OTLP_LOGS_PROTOCOL` or `OTEL_EXPORTER_OTLP_PROTOCOL` | Only `http/json` is supported. Any other value disables export, and the portal logs why. |
| `OTEL_SDK_DISABLED=true` | Turns export off. |

Without an endpoint nothing is sent. Export starts at the newest audit event
when first enabled, then resumes from its saved position after restarts. Events
go in batches of up to 512 every five seconds. A 429, 502, 503 or 504 response,
or an unreachable collector, keeps the batch for retry. Other rejections skip
the batch and log the status.

## Attributes

| Attribute | Source |
| --- | --- |
| `gen_ai.operation.name` | `execute_tool` for tool checks, otherwise `chat` |
| `gen_ai.provider.name` | `anthropic` or `openai` for subscription pools |
| `gen_ai.request.model` | Model |
| `gen_ai.agent.id` | Agent ID; the API key ID for pool traffic |
| `gen_ai.tool.name` | Tool |
| `gen_ai.conversation.id` | Client session header, for example `x-claude-code-session-id` |
| `gen_ai.usage.input_tokens`, `gen_ai.usage.output_tokens` | Provider usage on pool `complete` events. Input includes cache reads and writes. |
| `user.id` | Actor |
| `magpie.event.id`, `magpie.kind`, `magpie.decision` | Magpie audit fields with no convention |
| `magpie.detail` | Detail keys only, or the full detail when `capturePayloads` is on |

When the request carried a W3C `traceparent`, the log record's `traceId` and
`spanId` join the caller's trace. Records have resource `service.name` of
`magpie-gateway` and scope `magpie.audit`.

The Sentinel, Splunk, OCSF and syslog bodies keep their product-specific field
names, so the queries in `detections/` are unchanged.
