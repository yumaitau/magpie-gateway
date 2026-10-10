# Running an engagement

`./magpie.sh` stands up Magpie Gateway in a client's AWS account so
their developers can use Claude Code, Codex and GitHub Copilot through it
on day one. Each request is checked for Australian identifiers and PSPF
markings, routed only to models approved in the deployment settings,
and logged against the person who sent it.

## What you need

- `terraform` 1.6 or later, `aws`, `jq`, `openssl` and `curl`.
- An AWS CLI profile signed in to the client's account, with rights to
  create a VPC, ECS, EFS, an internal load balancer, CloudFront, Secrets
  Manager and IAM roles.
- Anthropic model access turned on in that account's Bedrock console in
  `ap-southeast-2`.
- For single sign-on, the `az` CLI signed in to the client's Entra
  tenant as someone who can register applications.

## The run

```bash
./magpie.sh new acme            # about ten questions
./magpie.sh up acme             # 15 to 20 minutes the first time
./magpie.sh check acme          # live smoke test, including a DLP block
./magpie.sh uitest acme         # browser test of the portal against the live stack
./magpie.sh connect acme alex   # setup files for one developer
./magpie.sh sso acme            # optional: Entra ID sign-in for the portal
```

`up` prints the portal address. Sign in as `admin` with
`MAGPIE_ADMIN_PASSWORD` from `engagements/acme/secrets.env`. After
`sso`, the person who ran it is an Owner and signs in with Entra.
Assign other people in Entra under Enterprise applications, Magpie
Gateway (acme), Users and groups. App roles are `Magpie.Owner`,
`Magpie.Admin`, `Magpie.Approver` and `Magpie.Auditor`.

`connect` writes `engagements/acme/handover/alex/` with a Claude Code
`settings.json` fragment, a Codex `config.toml` fragment, a Copilot
`chatLanguageModels.json`, a Copilot CLI env file, the key and a
README. Send that folder over a channel the client trusts.

Add or remove people with `./magpie.sh key add|remove acme <name>`,
then `./magpie.sh up acme`. The gateway restarts, which takes about a
minute, because the portal's SQLite database has one writer.

At the end, `./magpie.sh report acme` downloads the audit log, the AI
use-case register and the policy export. `./magpie.sh down acme`
deletes everything after you type the client name.

## What gets built

One VPC with no NAT gateway. One Fargate task runs the portal and
agentgateway side by side; the proxy calls the portal's guardrail
webhook on localhost with a shared token. The portal database is on
encrypted EFS with AWS Backup turned on. An internal load balancer
sends `/v1/*` to the proxy and everything else to the portal, and
refuses the guardrail routes. CloudFront, through a VPC origin, gives
the HTTPS address, so no domain or certificate is needed.

The proxy signs Bedrock calls with the task role, which may invoke only
`au.*` inference profiles and the foundation models in Sydney and
Melbourne they route to.

## Models and residency

`new` makes a one-token call to each Claude AU inference profile,
newest first, and keeps the newest Opus, Sonnet and Haiku the account
can actually call. Accounts often have older models turned on and newer
ones not, so the names follow what is there, for example
`claude-sonnet-4-6` or `claude-sonnet-5-5`. `au.anthropic.*` profiles
process only in `ap-southeast-2` and `ap-southeast-4`. The
gateway translates the OpenAI Chat Completions and Responses APIs to
Bedrock, so Codex and Copilot can use the same models.

Azure OpenAI is optional. In Australia East, GPT-5 and Codex models are
available only as Data Zone (Asia Pacific) or Global deployments. Tell
the wizard so: the portal's sovereign mode is on by default and blocks
those models until someone turns it off in the portal's Settings page.
That is the client's decision, and the audit log records it.

Fable models have no AU inference profile and are not offered.

## Things to know

- `up` and `down` show the Terraform plan and ask. Set
  `MAGPIE_AUTO_APPROVE=1` to skip that in a script.

- A request whose DLP rule says redact, mask or tokenise is blocked,
  not rewritten. Rewriting would break coding agents' tool calls. The
  error says why and asks for a new conversation, because the earlier
  messages are sent again on every turn.
- CloudFront waits up to 60 seconds for each part of a reply. A model
  that thinks longer than that before streaming anything is cut off.
  Ask AWS for a higher origin response timeout (up to 180 seconds) if
  that happens.
- The default proxy image is agentgateway `v1.6.0`. Some Bedrock fixes
  (tool choice, streamed reasoning in the Responses API) landed after
  it. Set `PROXY_IMAGE` in `magpie.conf` to a later release when there
  is one.
- `engagements/<client>/` holds the secrets, keys and Terraform state.
  It is git-ignored. Back it up somewhere the client has agreed to, and
  hand it over at the end.
- Only AWS is wired up. Azure Container Apps is the next target.
