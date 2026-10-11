# Alerts

Open **Alerts** in the console as an administrator with **Manage settings**.
Add a named destination, select a trigger, enter its connection details and
turn on **Enable alerts**. Save, then use **Save and send test alert** to check it.
This sends a real notification to the destination. No destinations are enabled
by default.

- **Slack:** paste an [incoming webhook URL](https://docs.slack.dev/messaging/sending-messages-using-incoming-webhooks/).
- **Teams:** use a [Workflows incoming webhook](https://learn.microsoft.com/microsoftteams/platform/webhooks-and-connectors/how-to/add-incoming-webhook)
  configured to accept anyone with its secret URL. Magpie sends an Adaptive Card.
- **Webhook:** HTTPS POST containing `source`, `deliveryId`, and `event`.
  An optional bearer token is under advanced settings. Deduplicate using
  `deliveryId` or the `X-Magpie-Delivery-Id` header.
- **Email:** set recipient, sender, SMTP hostname and credentials under email
  settings. Port 587 requires STARTTLS; port 465 uses implicit TLS. Certificate
  validation stays enabled. SMTP acceptance does not guarantee inbox delivery.

Security findings cover detected hits and block, deny, reject, redact, mask,
tokenise, require-approval and error **audit decisions**, including `wouldAction` findings in log-only mode. The
blocked trigger selects block/deny/reject/require-approval; changes selects policy, settings,
access, account and gateway-config audit events. This does not monitor external service uptime
or proxy failures that do not create audit events. Prompts, actor identities
and free-form audit detail/reasons are excluded from notifications; use the
event ID to look them up in the gateway.

The portal processes audit events in order into a SQLite queue, retaining its
cursor across restarts. New destinations start with new events. Delivery is
at least once: a crash after remote acceptance can produce a duplicate. Failures
retry up to five attempts, with exponential delays starting at 60 seconds.
The page shows the latest 50 deliveries and sanitized errors. Refresh to update.
Disabling or deleting a destination cancels pending work; a request already in
flight may still complete. A test is queued through the same delivery path.

Destination credentials live in the portal's SQLite database, outside policy
exports and revision history, and are never rendered back into forms. Protect
the database and backups as secrets. Blank secret fields retain saved values;
the explicit clear checkbox removes SMTP passwords and bearer tokens. Changes
to destinations are audited without their secrets. Destination administrators
can configure outbound network endpoints, so restrict Manage settings and use
deployment egress controls where needed. Delivery history currently follows the
portal database lifecycle; automatic history pruning is not implemented.
