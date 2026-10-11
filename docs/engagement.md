# Running an engagement

`./magpie.sh` stands up Magpie Gateway in a client's AWS account so
their developers can use Claude Code, Codex and GitHub Copilot through it
on day one. Each request is checked for Australian identifiers and PSPF
markings, routed only to models approved in the deployment settings,
and logged against the person who sent it.

## What you need

- `terraform` 1.6 or later, `aws`, `jq`, `openssl` and `curl`.
  `kubectl` is optional, for `./magpie.sh kubectl`.
- An AWS CLI profile signed in to the client's account, with rights to
  create a VPC, an EKS cluster, an internal load balancer, CloudFront and
  IAM roles. With an existing cluster, the profile also needs Kubernetes
  admin rights on it through an EKS access entry.
- Anthropic model access turned on in that account's Bedrock console in
  `ap-southeast-2`.
- For single sign-on, the `az` CLI signed in to the client's Entra
  tenant as someone who can register applications.

## The run

```bash
./magpie.sh new acme            # about ten questions
./magpie.sh up acme             # about 30 minutes the first time
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
then `./magpie.sh up acme`. The pod is replaced, which takes about a
minute, because the portal's SQLite volume has one writer.

`./magpie.sh kubectl acme <args>` runs kubectl in the `magpie`
namespace with its own kubeconfig in the engagement folder, for example
`./magpie.sh kubectl acme logs deploy/magpie -c proxy`.

At the end, `./magpie.sh report acme` downloads the audit log, the AI
use-case register and the policy export. `./magpie.sh down acme`
deletes everything after you type the client name.

## What gets built

Magpie runs on EKS. `new` asks for a cluster name.

- **Blank:** Terraform creates a VPC (two private and two public
  subnets, one NAT gateway) and an EKS Auto Mode cluster. Its Kubernetes
  API answers only the public address of the machine that ran `up`;
  the next `up` from somewhere else updates that. Expect about US$110 a
  month for the control plane and NAT gateway, plus the Auto Mode node.
- **An existing cluster in `ap-southeast-2`:** Magpie installs into the
  `magpie` namespace. An Auto Mode cluster needs nothing else. Otherwise
  it needs the `eks-pod-identity-agent` add-on, the AWS Load Balancer
  Controller (for `TargetGroupBinding`) and an EBS-backed storage class;
  `new` checks the add-on and asks for the class.

The Helm chart in `deploy/helm/magpie-gateway` runs one pod with two
containers, the portal and agentgateway. The proxy calls the portal's
guardrail webhook on localhost with a shared token, so that route is
never on the network. The portal database is SQLite on an encrypted gp3
volume, so the pod runs one replica and is replaced, not rolled.

Terraform owns the front door: an internal load balancer in the
cluster's private subnets, and CloudFront through a VPC origin for the
HTTPS address, so no domain or certificate is needed. The pod joins the
load balancer's target groups through `TargetGroupBinding`, which also
opens the pod ports to the load balancer's security group. `/v1/*` goes
to the proxy and everything else to the portal; the guardrail routes are
refused.

The proxy signs Bedrock calls through EKS Pod Identity with a role that
may invoke only `au.*` inference profiles and the foundation models in
Sydney and Melbourne they route to. Secrets are Kubernetes Secrets, set
by Terraform from `engagements/<client>/secrets.env` and kept out of
plan output.

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

- If the account encrypts EBS by default with a customer KMS key, the
  chart's storage class uses the AWS managed `aws/ebs` key instead,
  because the Auto Mode cluster role usually cannot use the customer
  key and every volume would be created and then deleted. Set
  `ebs_kms_key_arn` to a key the cluster role can use if policy needs a
  customer key.
- The portal image is amd64 only, so the pod is pinned to amd64 nodes.
  Auto Mode clusters with Graviton node pools would otherwise schedule
  it on arm64.
- A failed install rolls itself back, so `up` can simply be run again.
- `./magpie.sh use <client> [person]` points this machine's Claude Code,
  Codex, opencode and Copilot in VS Code at the gateway; `unuse` undoes
  it. The key lives in `~/.config/magpie/<client>.key` and each tool
  reads it from there.

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

## Trial before enforcement

New engagements set `GUARDRAIL_MODE=log-only` in `engagements/<client>/magpie.conf`.
This seeds the initial mode. Existing deployments keep their current mode when
upgrading. After first start, use **Settings → Guardrail mode → Apply mode** to
switch between Log-only and Enforcement without redeploying. The choice persists
across restarts and deployments; editing `magpie.conf` does not replace it.

The portal shows a trial banner. Requests and responses pass unchanged while
findings appear in the audit log and CSV as `log`, with the would-be action and
reason. Kill switches also only log during a trial. Authentication stays required.
Review findings, adjust policy and approvals, then select Enforcement in Settings.
`./magpie.sh check <client>` reads the live mode and tests the corresponding behavior.
The proxy still fails closed on a webhook outage. See `magpie/README.md` for
independent MCP proxy controls and `docs/access-control.md` for role configuration.
