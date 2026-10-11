# Accounts, invitations and password recovery

All product screens use the native Agentgateway shell. **My account** and
**People** appear at the top of the sidebar; the account menu also links to
password changes and invitations. Governance, Australian privacy, approvals,
audit, settings, custom roles and user groups open under `/ui/governance/`.
Old bookmarked page URLs redirect into that shell.

## Invite a colleague

Open **People → Invite colleague**, enter their name and email, select the user
groups they need, and choose **Create invitation**. Copy the private link or
open it in your email app. The portal does not send email automatically. The
colleague opens the link, chooses their password, then signs in with their email.
Invitations expire after 24 hours. They grant the selected groups on acceptance,
using a new stable local subject, never by linking an existing SSO identity on
an email match. Group permission changes continue to apply to existing sessions.
Manage access permission is required to invite, reset or revoke links. Owner
groups grant full control, so choose groups deliberately.

Links are shown only when created, stored as SHA-256 digests, single-use, and
revocable from People. Creating a replacement invalidates the previous link.
The secret travels in the URL fragment rather than the request URL. Share it
only with its named recipient. An invitation holder can establish that account;
this is not independent email-ownership verification. No invitation is sent by
this feature without the operator using their own delivery channel.

## Change or reset a password

**My account → Change password** requires the current password and confirmation
of the new one. Use a passphrase of at least 16 characters (maximum 72 UTF-8
bytes, matching the existing bcrypt credential storage). Passwords are stored as
bcrypt hashes, separately from exportable policies. A successful change revokes
all sessions and outstanding account links. Sign in again with the new password.

If a colleague forgets their password, an access administrator can choose
**People → Create password reset link**. The reset link lasts one hour and ends
existing sessions when used. Disabled users cannot redeem links. SSO passwords
remain managed by the organisation's identity provider; local password reset
cannot take over an SSO identity. The sign-in page explains these recovery paths.

The recovery administrator changes its own password through My account. Other
users cannot issue reset links for that recovery identity. Its deployment
password initialises the database credential once; future deployments do not
overwrite UI password changes. Keep the existing deployment secret configured,
but after a UI change use the new password for scripts such as `magpie.sh check`
and `uitest` (update the ignored engagement `secrets.env` locally as needed).
If every administrator loses access, an authorised operator must back up the
SQLite database, stop the portal, delete only that recovery administrator's row
from `local_accounts`, then restart with a new deployment password. Delete its
`sessions` and `account_links` rows at the same time. Do not delete the database
or reset other users as a recovery shortcut.

Back up the entire portal SQLite database before upgrades and protect those
backups as credential material. Account mutations enforce current permissions,
same-origin browser requests and session revocation. Login/password attempts
are rate limited per account, with an additional global login/acceptance limit;
limits are in process and reset on restart. Audit records omit passwords and
link tokens. The service does not claim MFA or email delivery for local accounts.

Recovery design follows the single-use, expiring-token and session-revocation
principles in the [OWASP Forgot Password Cheat Sheet](https://cheatsheetseries.owasp.org/cheatsheets/Forgot_Password_Cheat_Sheet.html).
