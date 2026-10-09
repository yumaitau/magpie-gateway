# Detection content

These queries read Magpie audit events. They are starting points. Tune
the thresholds. They have not been run in a live Sentinel workspace or
a live Security Lake.

The event fields are the ones `sentinel_record` writes: `TimeGenerated`,
`EventId`, `Kind`, `Actor`, `AgentId`, `Tool`, `Model`, `Decision`,
`Reason`, `Detail`. Create the custom table from that schema. If the
workspace rewrites names, change the queries to match.

Full payloads are off by default, so `Detail` has hit kinds and not the
matched text. Do not turn payload capture on just to make a query match.

`yumaitau/ai-agent-threat-detection` (MIT, Yuma IT) is a separate pack
for cloud logs. It does not query this schema. Microsoft rule `D14`
counts WAF hits in a 10 minute bin. Microsoft rule `D15` is a draft
hunt for new MCP destinations. The tool-spree query below keeps the
10 minute window and a distinct count, and uses its own thresholds,
because a WAF hit is not a tool call. The other five queries are new.

| File | What it looks for |
| --- | --- |
| `sentinel/tool-call-spree.kql` | 100 tool calls, or 15 distinct tools, in 10 minutes |
| `sentinel/unapproved-tool.kql` | A decision that the tool or model was not approved |
| `sentinel/dlp-block-spike.kql` | 20 or more DLP hits in an hour |
| `sentinel/credential-in-arguments.kql` | A secret hit kind on a decision |
| `sentinel/after-hours.kql` | A tool call outside 08:00 to 18:00, Australia/Sydney, on a weekday |
| `sentinel/blocked-destination.kql` | Sovereign mode or a blocked classification route |
| `athena/*.sql` | The same checks for JSON landed in S3 |

The OCSF object the sink can write is a subset. It is not a certified
OCSF mapping. The Athena SQL reads the Magpie field names. Map them
into Security Lake if you need that schema.
