import { Link, useRouterState } from '@tanstack/react-router';
import { useState } from 'react';

import { PageHeader } from '@/components/Primitives';

type Step = { title: string; body: string; to?: string; link?: string };
type Guide = { id: string; title: string; description: string; steps: Step[]; done: string };
const guides: Guide[] = [
	{
		id: 'start',
		title: 'Get your first model working',
		description:
			'Start here. You need permission to edit gateway configuration and credentials for an AI provider. Keep advanced routing at its defaults for your first connection.',
		steps: [
			{
				title: 'Add a model',
				body: 'Open Home and choose Add your first model (or Add another model). Choose the provider and enter its credentials and provider model identifier. Leave Advanced credentials and routing closed unless your provider needs a different credential source or endpoint.',
				to: '/',
				link: 'Open Home'
			},
			{
				title: 'Save and check the model',
				body: 'Save the model. A saved configuration means the details are recorded; it does not prove the provider connection works. If a setting is read-only, ask your deployment administrator to change its source configuration.',
				to: '/llm/models',
				link: 'Open Models'
			},
			{
				title: 'Send a synthetic prompt',
				body: 'Open Chat Playground, select the model and send a harmless prompt such as “Reply with the word ready.” A model response verifies the connection. If it fails, use the troubleshooting guide before adding advanced policies.',
				to: '/llm/playground',
				link: 'Open Chat Playground'
			},
			{
				title: 'Connect your app',
				body: 'Open Client Setup. Select your app or SDK, model, gateway base URL and gateway key. Copy the generated settings into your app, send a request and confirm it worked.',
				to: '/llm/client-setup',
				link: 'Open Client Setup'
			},
			{
				title: 'Review protection before wider use',
				body: 'Open Controls to see whether Magpie is observing or enforcing. Review approvals and DLP with synthetic samples, configure alerts, then test the actual connected route. A working model connection is not proof that every guardrail is connected.',
				to: '/governance/controls',
				link: 'Open Controls'
			}
		],
		done: 'You have received a response through the gateway from your own app. Home remembers model tests and confirmed app connections in this browser.'
	},
	{
		id: 'model-sync',
		title: 'Find new Claude, Codex, Grok and OpenCode models',
		description:
			'Model suggestions sync automatically. A catalog entry describes a model; your provider account and gateway policies determine whether you can use it.',
		steps: [
			{
				title: 'Check catalog freshness',
				body: 'Open Models and expand Model catalog. Magpie checks models.dev on startup and hourly. Administrators can press Sync now. If the source is unavailable, the last successful catalog stays available and Magpie retries after five minutes.',
				to: '/llm/models',
				link: 'Open Models'
			},
			{
				title: 'Choose the correct provider',
				body: 'Claude Code uses models from your Anthropic route; Codex uses your OpenAI route. Grok uses xAI. OpenCode can use many providers; OpenCode Zen and Go are separate catalog providers. For a custom endpoint, select Catalog provider in the model picker. Model IDs may differ between providers.'
			},
			{
				title: 'Select or type a model',
				body: 'Open Models → All catalog models to browse every provider, even those not yet connected. Search by name, such as Opus 5.5 or Codex, and press Configure to start a draft. Configured routes shows only routes already added to your gateway. Existing wildcard routes can choose a Specific model in Chat Playground or Client Setup. A model missing from the public catalog can still be typed by its exact provider ID. Sync does not change cost rates or the models advertised by the gateway discovery endpoint.'
			},
			{
				title: 'Verify access and connect',
				body: 'Save an explicit route if needed, obtain any required model approval, and test a harmless prompt in Chat Playground. Then regenerate Client Setup instructions for your coding tool. Subscription-only models are not necessarily available through an API credential.',
				to: '/llm/client-setup',
				link: 'Open Client Setup'
			}
		],
		done: 'The new model responds through the gateway and your coding tool uses its configured gateway name.'
	},
	{
		id: 'clients',
		title: 'Connect an app or create a gateway key',
		description:
			'Provider credentials let the gateway call a provider. A virtual API key lets an app call the gateway. These are different credentials.',
		steps: [
			{
				title: 'Choose a working model',
				body: 'First verify your model in Chat Playground. Use the gateway model name in your client, which can differ from the provider’s model identifier.',
				to: '/llm/playground',
				link: 'Test the model'
			},
			{
				title: 'Create an app key if needed',
				body: 'Open Virtual API Keys and create a key with the models and limits that app needs. Copy the secret when it is shown and keep it private. Do not put provider credentials into the app when it should use a gateway key.',
				to: '/llm/keys',
				link: 'Open Virtual API Keys'
			},
			{
				title: 'Generate connection instructions',
				body: 'In Client Setup, choose the integration and model, enter the gateway URL reachable from your app, and select or enter its key. Use the generated configuration for that integration.',
				to: '/llm/client-setup',
				link: 'Open Client Setup'
			},
			{
				title: 'Verify from the app',
				body: 'Send a harmless request. Check Logs for the gateway request and Audit log for Magpie decisions on connected routes. Only mark the app connected after a response succeeds.',
				to: '/llm/logs',
				link: 'Open request logs'
			}
		],
		done: 'The app receives a response using a gateway URL and a gateway key.'
	},
	{
		id: 'dlp',
		title: 'Tune a DLP false positive',
		description:
			'DLP means data loss prevention. A finding is a candidate match, not proof that a value is sensitive or valid. Adjust one detector at a time.',
		steps: [
			{
				title: 'Check why it was blocked',
				body: 'Open Audit log and inspect the reason. DLP, model/tool approval, residency and prompt injection are separate checks. Use Tune next to a DLP finding, or choose its detector in Tune DLP.',
				to: '/governance/audit',
				link: 'Review the audit log'
			},
			{
				title: 'Choose a narrow adjustment',
				body: 'Choose Monitor only to keep findings without enforcing that detector’s action. For one known harmless value, open Exact-value exceptions and enter the exact detected value and a descriptive label. Exceptions apply to that detector across all connected traffic, not just one user or app.',
				to: '/dlp-tuning',
				link: 'Open Tune DLP'
			},
			{
				title: 'Compare active and draft',
				body: 'Enter a synthetic false-positive sample and run Compare active and draft. Review the matched detector, reason, byte position and resulting action. Add a separate synthetic sensitive example to check that useful detections still work. An overlapping detector may still block the text.'
			},
			{
				title: 'Save and obtain approval',
				body: 'Save the draft. If second-person approval is enabled, another authorised person must approve it under pending changes. The active rule does not change while waiting. Draft tests do not save rules or send text to a model or external analyser.',
				to: '/governance/dlp',
				link: 'Review pending DLP changes'
			},
			{
				title: 'Verify and revisit',
				body: 'After approval, send a synthetic request through the affected app and inspect the audit result. Remove an exception by checking Remove in the tuner, testing, then saving. Restore default in the full detector table enables the detector, resets its action and removes all its exceptions.'
			}
		],
		done: 'The harmless example passes as intended, useful detections still act, and the approved change is visible in the active rule.'
	},
	{
		id: 'controls',
		title: 'Understand controls and enable enforcement',
		description:
			'ISM is the Information Security Manual. Essential Eight is a set of security strategies. APPs are Australian Privacy Principles. These framework names are mappings, not on/off switches.',
		steps: [
			{
				title: 'Read the current state',
				body: 'Controls shows what is enforcing, observing, built in or managed outside Magpie. Essential Eight details identify the controls your device, identity and infrastructure teams must manage.',
				to: '/governance/controls',
				link: 'Open Controls'
			},
			{
				title: 'Prepare the rules',
				body: 'Review model and tool approvals, detector actions, Australian destination restrictions and alert destinations. Start with synthetic data. Log-only sends traffic unchanged, including sensitive values.',
				to: '/dlp-tuning',
				link: 'Test DLP rules'
			},
			{
				title: 'Check each route',
				body: 'Verify that requests and responses on your intended gateway routes reach Magpie checks. Native MCP does not automatically call the separate Magpie MCP check API. Ask the deployment owner to verify the integration.'
			},
			{
				title: 'Apply enforcement',
				body: 'In Enforcement and settings, choose Enforce under Guardrail mode and press Apply mode. This requires Change enforcement mode permission and takes effect immediately. Policy settings below it use their separate approval flow.',
				to: '/governance/settings',
				link: 'Open enforcement settings'
			},
			{
				title: 'Test and monitor',
				body: 'Send an expected-pass and expected-block synthetic request through your app. Review Audit log. If a single detector is noisy, tune that detector. Switching the whole gateway to log-only also makes kill switches observation-only.',
				to: '/governance/audit',
				link: 'Review decisions'
			}
		],
		done: 'You have verified expected pass/block behavior on the connected route, not just selected Enforce in a form.'
	},
	{
		id: 'alerts',
		title: 'Send alerts to Slack, Teams, a webhook or email',
		description:
			'An alert destination is where notifications go. Destinations start disabled. You need permission to manage settings.',
		steps: [
			{
				title: 'Add the destination',
				body: 'Open Alerts, give the destination a name and choose Send to. Paste the incoming webhook URL supplied by Slack or Teams, your webhook endpoint, or the recipient email address.',
				to: '/governance/alerts',
				link: 'Open Alerts'
			},
			{
				title: 'Choose what to send',
				body: 'Choose event and decision filters for the notifications you want. Log-only findings may have a different final decision from enforced blocks, so review the filter against your operating mode.'
			},
			{
				title: 'Configure email or authentication',
				body: 'For email, expand Email and advanced settings and supply your SMTP host, port, sender and any required authentication. For an authenticated webhook, configure its bearer token. Existing secrets are not displayed back in the form.'
			},
			{
				title: 'Save, then test delivery',
				body: 'Save destination first. Reopen it and use Save and send test alert when you are ready to notify that destination. This sends a real test message. Check the destination and delivery status.'
			},
			{
				title: 'Enable or stop notifications',
				body: 'Select Enable alerts and save when ready. To stop delivery, clear Enable alerts and save, or Delete destination. Disabling cancels queued alerts for that destination.'
			}
		],
		done: 'A test arrived at the intended destination and enabled delivery is visible in Alerts. Alerts contain summaries, not raw prompt text.'
	},
	{
		id: 'people',
		title: 'Invite people and manage permissions',
		description:
			'A role is a set of permissions. A group grants roles to its members. People need only the access required for their work.',
		steps: [
			{
				title: 'Review available roles',
				body: 'Use Roles and groups to review what each role can view or change. Changes to permissions require access-management permission.',
				to: '/governance/access',
				link: 'Open Roles and groups'
			},
			{
				title: 'Create an invitation',
				body: 'Open People, enter the colleague’s name and email, select the intended groups and create the invitation. An invitation link is private and must be shared with that colleague.',
				to: '/people',
				link: 'Open People'
			},
			{
				title: 'Complete sign-in',
				body: 'The colleague opens the private link, sets a password, and signs in. If the deployment uses organisation sign-in, group mapping and MFA are configured by the identity/deployment administrator.'
			},
			{
				title: 'Check access and recovery',
				body: 'Confirm the colleague can see the required pages. My account changes an existing password; an administrator can create a password reset link in People. Local password login does not itself enforce MFA.',
				to: '/account',
				link: 'Open My account'
			}
		],
		done: 'The colleague can sign in and sees the pages needed for their role. Missing pages usually mean missing permissions.'
	},
	{
		id: 'mcp',
		title: 'Connect MCP tools or HTTP routes',
		description:
			'These are optional advanced features. MCP connects tools to AI clients; HTTP routing forwards ordinary API traffic.',
		steps: [
			{
				title: 'Enable the feature you need',
				body: 'On Home, expand Gateway settings to enable MCP or Traffic. Use Advanced in the navigation to find the corresponding server, gateway and route screens.',
				to: '/',
				link: 'Open Home'
			},
			{
				title: 'For MCP, add a server',
				body: 'Configure the MCP server endpoint or supported transport and its authentication. Review the exposed tools and test using Tool Playground. Avoid opening every tool to every caller.',
				to: '/mcp/servers',
				link: 'Open MCP servers'
			},
			{
				title: 'For HTTP traffic, configure a route',
				body: 'Select the gateway/listener, host and path match, and backend destination. Defaults are sufficient for many simple routes; use specialist settings only when required.',
				to: '/traffic/routes',
				link: 'Open Traffic routes'
			},
			{
				title: 'Check policy coverage',
				body: 'Native gateway policies and Magpie governance checks are separate. Confirm required authentication, approvals and guardrail hooks are attached to the actual route before widening access.'
			}
		],
		done: 'A synthetic request reaches the expected tool/backend and an unauthorised request is refused.'
	},
	{
		id: 'troubleshooting',
		title: 'Troubleshoot setup and blocked requests',
		description:
			'Start with the failing step and its error message. Do not change multiple settings at once.',
		steps: [
			{
				title: 'Cannot sign in',
				body: 'Check the username/email and password or use organisation sign-in when available. After repeated failures, wait before retrying. Ask an administrator for a password reset link; creating a model key will not fix portal login.'
			},
			{
				title: 'Model returns an error',
				body: 'Check provider credentials, provider model ID, permitted region and the gateway model name. A saved model can still have an invalid credential. Use Playground to isolate the provider connection from your app.',
				to: '/llm/playground',
				link: 'Test in Playground'
			},
			{
				title: 'Playground works but the app fails',
				body: 'Check that the app can reach its gateway base URL, uses the correct API protocol, gateway key and model name. Use the integration-specific Client Setup instructions.',
				to: '/llm/client-setup',
				link: 'Open Client Setup'
			},
			{
				title: 'Enforcement blocks a request',
				body: 'Read the Audit log reason first. A missing model/tool approval, unsupported destination region, protective marking or injection finding is not a DLP false positive. For a DLP finding, use Tune DLP and compare a narrow change.',
				to: '/governance/audit',
				link: 'Find the decision'
			},
			{
				title: 'Saved change seems to do nothing',
				body: 'Check whether it is pending approval, whether the gateway is in log-only mode, and whether the route calls Magpie. Configuration reload failures mean the last valid configuration may still be running.',
				to: '/governance/controls',
				link: 'Check effective controls'
			},
			{
				title: 'Alert does not arrive',
				body: 'Check Enabled, event/decision filters, URL or SMTP details, and delivery status. Use the test action deliberately; it sends a real message. A disabled destination does not drain its queued messages.',
				to: '/governance/alerts',
				link: 'Check Alerts'
			}
		],
		done: 'You have identified whether the problem is identity, provider connectivity, client configuration, policy, or notification delivery.'
	}
];
const options: [string, string][] = [
	[
		'Model / provider',
		'A provider hosts the AI service. A model entry connects a gateway-facing name to the provider’s model and credentials.'
	],
	[
		'Provider model ID / gateway model name',
		'The provider ID identifies the upstream model. The gateway name is what clients request. An alias lets you change providers without changing every client.'
	],
	[
		'Provider credentials / virtual API key',
		'Provider credentials authenticate upstream. Virtual API keys authenticate clients at the gateway and can carry limits or model restrictions.'
	],
	[
		'Gateway base URL',
		'The address your client can reach. Use Client Setup for the correct path and protocol instead of guessing an endpoint.'
	],
	[
		'Advanced credentials',
		'Select environment, file or another supported credential source when a pasted API key is unsuitable. Credential references must exist in the running deployment.'
	],
	[
		'Advanced model routing',
		'Custom endpoints, aliases, gateway bindings and provider-specific settings. Leave defaults for a basic provider connection unless your deployment requires changes.'
	],
	[
		'Virtual model / fallback / load balancing',
		'A virtual model routes a client-facing name to backing models. Fallback tries another backend after specified failures; load balancing distributes requests.'
	],
	[
		'Gateway / listener / route / backend',
		'A gateway groups traffic configuration. A listener accepts connections. A route matches a request. A backend receives it.'
	],
	[
		'Timeout / retry',
		'A timeout limits how long a call can wait. A retry sends another attempt after eligible failures and can increase usage or repeat work.'
	],
	[
		'Rate limit / budget',
		'Rate limits cap request or token usage over time. Budgets constrain configured spend. Check scope and enforcement in the relevant key or policy.'
	],
	[
		'Guardrail mode',
		'Log-only records decisions but leaves traffic unchanged. Enforce applies Magpie actions on connected routes. Login permissions remain enforced in either mode.'
	],
	[
		'DLP action',
		'Monitor/log keeps text and findings. Redact removes matches. Mask keeps the last two characters. Tokenise uses a stable keyed pseudonym. Block/require approval stop normal processing. Allow retains findings without a protective transformation.'
	],
	[
		'Exact-value exception',
		'Suppresses one exact, case-sensitive matched value for one detector across all connected traffic. Formatting changes will not match. It does not suppress other detectors or classification routing.'
	],
	[
		'Detector enabled',
		'Off suppresses this detector’s findings and actions. It is broader than an exception. Presidio off also stops external analyser requests.'
	],
	[
		'Sovereign mode',
		'Restricts destinations by Australian region tags. Turning it off removes the blanket restriction; independent classification restrictions still apply. Region tags do not prove where a provider processes data.'
	],
	[
		'Presidio analyser URL',
		'An optional external detector endpoint. When configured and enabled it receives text for analysis. Local detectors work separately. The local tuning preview never calls it.'
	],
	[
		'Protective markings / approved PROTECTED destinations',
		'Markings affect permitted routing independently of the DLP action. PROTECTED needs an approved destination; SECRET and TOP SECRET are refused when enforcing. Approved destination configuration is managed by the deployment owner.'
	],
	[
		'Kill switches',
		'Global or per-agent stop controls. They take effect immediately when enforcing; in log-only mode they only record what would happen.'
	],
	[
		'Pending revision / approval',
		'A proposed policy change is separate from the active rule. When second-person approval is enabled, its author cannot approve it.'
	],
	[
		'Restore default',
		'Resets a detector’s action, turns it on and removes its exceptions through the approval process. It does not reset the whole gateway.'
	],
	[
		'Audit log / request logs',
		'Audit log records Magpie decisions and policy events. Request logs show native gateway traffic. Neither screen alone proves that every request was checked by every control.'
	],
	[
		'Audit retention',
		'A configured retention target. Automatic audit deletion is not implemented; storage operations must verify retained history, backups and disposal.'
	],
	[
		'Alert destination / event and decision filters',
		'A destination selects the delivery channel. Filters select which events are queued. Test messages are real outbound messages.'
	],
	[
		'Webhook / bearer token',
		'A webhook URL accepts notifications. A bearer token optionally authenticates the delivery request. Treat both as private where they grant access.'
	],
	[
		'SMTP / sender / recipient',
		'SMTP settings connect to an email server. Sender is the from-address; recipient is where the alert goes. The server may require TLS and authentication.'
	],
	[
		'Role / group',
		'Roles bundle permissions. Groups grant those roles to members. A page or action can be unavailable even when sign-in succeeds.'
	],
	[
		'OIDC / SSO / MFA',
		'OIDC connects the portal to an identity provider. SSO is organisation sign-in. MFA must be required and verified at that provider; local passwords alone are not MFA.'
	],
	[
		'CEL policies',
		'An advanced expression language for request rules. Saving an expression is not proof it is attached to every native route; verify the integration and test it.'
	],
	[
		'Telemetry and phone-home',
		'A saved policy setting, not proof that every dependency or infrastructure component has telemetry disabled. Verify deployment-level behavior with the owner.'
	],
	[
		'Read-only / configuration source',
		'Some fields are owned by a deployment file or external configuration source. Update that source when the UI cannot save a change.'
	]
];
export function helpTopic(path: string): string {
	if (path.includes('dlp')) return 'dlp';
	if (path.includes('alerts')) return 'alerts';
	if (path.includes('people') || path.includes('account') || path.includes('access'))
		return 'people';
	if (path.includes('client') || path.includes('keys')) return 'clients';
	if (path.includes('governance') || path.includes('guardrail') || path.includes('polic'))
		return 'controls';
	if (path.includes('mcp') || path.includes('traffic')) return 'mcp';
	if (path.includes('logs')) return 'troubleshooting';
	return 'start';
}
export function HelpPage() {
	const path = useRouterState().location.pathname;
	const id = path.split('/').at(-1);
	const guide = guides.find(g => g.id === id) || guides[0];
	const [search, setSearch] = useState('');
	const term = search.trim().toLowerCase();
	return (
		<div className="page-stack help-page">
			<PageHeader
				title="Getting started & help"
				description="Follow a guide, learn what a setting does, or find the next step when something fails."
			/>
			<label htmlFor="help-search">Search guides and options</label>
			<input
				id="help-search"
				type="search"
				value={search}
				onChange={e => setSearch(e.target.value)}
				placeholder="Try DLP, API key, approvals or SMTP"
			/>
			<nav aria-label="Help guides">
				<ul className="help-guide-list">
					{guides
						.filter(g => !term || JSON.stringify(g).toLowerCase().includes(term))
						.map(g => (
							<li key={g.id}>
								<Link
									to="/help/$topic"
									params={{ topic: g.id }}
									aria-current={g.id === guide.id ? 'page' : undefined}
									onClick={() => setSearch('')}
								>
									{g.title}
								</Link>
							</li>
						))}
				</ul>
			</nav>
			{!term && (
				<article key={guide.id} className="help-guide">
					<h2>{guide.title}</h2>
					<p>{guide.description}</p>
					<p>
						Pages and actions depend on your permissions. If a linked screen is unavailable, ask an
						administrator for the required access.
					</p>
					<ol>
						{guide.steps.map(step => (
							<li key={step.title}>
								<h3>{step.title}</h3>
								<p>{step.body}</p>
								{step.to && (
									<Link className="button" to={step.to}>
										{step.link}
									</Link>
								)}
							</li>
						))}
					</ol>
					<p>
						<strong>How you know it worked:</strong> {guide.done}
					</p>
				</article>
			)}
			<details className="schema-details help-options" open={term ? true : undefined}>
				<summary>What the options mean</summary>
				{term &&
					!guides.some(g => JSON.stringify(g).toLowerCase().includes(term)) &&
					!options.some(o => o.join(' ').toLowerCase().includes(term)) && (
						<p role="status">No matches. Try a detector name, screen name or setting.</p>
					)}
				{options
					.filter(o => !term || o.join(' ').toLowerCase().includes(term))
					.map(([title, body]) => (
						<details key={title} className="schema-details" open={term ? true : undefined}>
							<summary>{title}</summary>
							<p>{body}</p>
						</details>
					))}
			</details>
		</div>
	);
}
