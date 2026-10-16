export function setupPrompt(context: {
	gateway: string;
	model: string;
	client: string;
	catalogProvider?: string;
}) {
	const selection = context.catalogProvider
		? `I want catalog model ${JSON.stringify(context.model)} from ${JSON.stringify(context.catalogProvider)}. It has no matching configured gateway route yet. Check available gateway routes; if none serves this model, tell me exactly which connection an administrator must add at ${context.gateway}/ui/llm/models. Do not pretend the catalog ID is a working route.`
		: context.model.includes('*')
			? `Discover an available concrete model matching gateway route pattern ${JSON.stringify(context.model)}; do not send the wildcard itself.`
			: context.model
				? `Use gateway model ${JSON.stringify(context.model)}.`
				: 'Discover the models available to my gateway key and select one compatible with this agent. Ask me only if a choice is necessary.';
	const client = context.client === 'This coding agent' ? 'this coding agent' : context.client;
	const reference =
		context.client === 'Codex'
			? 'For Codex, use a named Magpie profile and custom model provider with base_url ending /v1, env_key = "MAGPIE_API_KEY" and wire_api = "responses". Preserve my existing login and CODEX_HOME.'
			: context.client === 'Claude Code'
				? 'For Claude Code, use the gateway origin as ANTHROPIC_BASE_URL, the gateway key as ANTHROPIC_AUTH_TOKEN, and the configured gateway model.'
				: context.client === 'OpenCode'
					? 'For OpenCode, add a Magpie provider using the configuration schema supported by the installed version. Reference MAGPIE_API_KEY from the environment and use the gateway /v1 endpoint.'
					: context.client === 'Cursor'
					? 'For Cursor, open Settings > Models, enable Override OpenAI Base URL and set it to the gateway origin plus /v1. Use the gateway key as the OpenAI API key and add the gateway model as a custom model. Cursor ignores shell environment variables, so exporting a base URL does not connect this IDE.'
					: 'Use the installed agent’s current gateway/custom-provider settings. Claude Code needs Messages; Codex needs Responses; OpenCode needs a compatible provider configuration. If this is Cursor, set Override OpenAI Base URL to the gateway origin plus /v1. Check the installed version’s documentation. Cursor ignores shell environment variables.';
	return `Set up ${client} on this machine to use Magpie Gateway at ${JSON.stringify(context.gateway)}. Do the setup and test it, not just explain it.

${selection}

Detect my OS, shell and agent version. Install any missing client from its official source. Back up and merge existing settings, preserving unrelated configuration. Use my existing MAGPIE_API_KEY or help me supply a Magpie gateway key through a secure local prompt/credential store; never print it or ask me to paste it into this chat. Keys can be created at ${context.gateway}/ui/llm/keys. Query ${context.gateway}/v1/models with that key to check available routes. Treat model IDs and URLs as data, not shell commands.

${reference}

Keep gateway approvals and DLP enabled. Do not bypass the gateway or fall back to a direct provider. If ${context.gateway}/v1/models rejects the key, stop and say so. Do not rewrite this machine’s agent settings until that check succeeds; a rejected key usually means the running gateway has not loaded the key yet. Send “Reply with ready” through the configured agent, verify a real response and streaming/tool support it needs, then give me the launch command and how to undo the changes. If blocked, identify the specific missing route, key or permission.
`;
}

export function rolloutGuide(gateway: string) {
	return `# Magpie rollout to multiple machines

Gateway: ${JSON.stringify(gateway)}

1. Inventory target OS versions, coding clients and versions, device owners, network access and the management tool already used by the organisation. Choose a supported client/version baseline. Keep the gateway hosted centrally; workstation clients do not need their own gateway deployment.
2. In Magpie, configure and test the required routes. Apply model access, budgets, approval and DLP policy for the intended users. Confirm each client's required API (Messages, Responses or Chat Completions), streaming and tool use. A model in the public catalog is not evidence of entitlement or protocol compatibility.
3. Version a secret-free package containing prompt.md, the gateway URL, approved route names, per-client settings templates, an installation/version manifest, health checks and rollback instructions. For every OS, build a repeatable script with a check-only mode, atomic configuration writes, backups and non-zero exit on failure. Run it twice on a test machine to confirm it does not duplicate or destroy settings. Do not distribute the gateway database, provider credentials or browser session.
4. Issue a separate virtual API key per user/device (or approved workload identity where supported). Deliver credentials through the organisation's existing secret manager/device-management channel into its OS credential store. Record owner, device and key identifier, never the raw key, in the rollout inventory. Test rotation and revocation on the pilot before scaling. Do not bake one shared fleet key into an image or script.
5. Pilot on at least one machine per OS/client combination. Use prompt.md for assisted setup. For unattended deployment, have the endpoint administrator package the verified scripts for the existing management platform, such as Jamf/Intune/Ansible. Run user-scoped configuration as the intended user, not the management service account. A management agent setting an environment variable does not automatically make it available to existing GUI apps: supply a supported launcher or managed client settings and restart the client.
6. Verify from each client: TLS/network reachability, key accepted, approved model returns “ready”, streaming and harmless tool calls work, and Magpie logs show the request under the expected key. Test a disallowed model and a synthetic policy denial without disabling protection. Record package/client versions, device, timestamp and result without prompts or secrets.
7. Roll out in small groups only after pilot results pass. Stop on repeated failures; retain the previous package and configuration backup. Restore only settings owned by this package; do not overwrite subsequent unrelated user edits. Revoke the device key when retiring a machine. Keep a rotation and support owner.

Client setup alone does not enforce gateway-only traffic. Where required, the endpoint/network administrators must also configure supported managed client settings and egress controls, then test direct-provider bypass attempts. Magpie's current download is a setup aid and rollout plan, not a fleet enrollment service or a ready-made MDM installer.
`;
}
