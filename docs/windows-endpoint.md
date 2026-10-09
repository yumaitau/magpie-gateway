# Windows endpoint enforcement

How to point managed coding agents at Magpie, and how to make the
direct public endpoints harder to reach. Checked against the vendor
pages on 9 October 2026. Paths and setting names change. Read the
linked page again before you deploy a profile.

This is not an Intune package. Several of these products do not publish
an Intune administrative template. Where that is the case, deliver the
file with a platform script or a Win32 app, and say so in the change
record.

The proxy repository also has `examples/microsoft-intune/`. Those
scripts check Claude Desktop `inferenceGatewayBaseUrl` and a Codex
`base_url`. They are not Claude Code settings. Do not copy the Desktop
key into Claude Code.

## Claude Code

Source: [Deploy managed settings](https://code.claude.com/docs/en/managed-settings)
and [Connect Claude Code to an LLM gateway](https://code.claude.com/docs/en/llm-gateway-connect).

| Platform | Managed settings file |
| --- | --- |
| Windows | `C:\Program Files\ClaudeCode\managed-settings.json` |
| macOS | `/Library/Application Support/ClaudeCode/managed-settings.json` |
| Linux and WSL | `/etc/claude-code/managed-settings.json` |

Windows registry: a `REG_SZ` or `REG_EXPAND_SZ` value named `Settings`
under `HKLM\SOFTWARE\Policies\ClaudeCode`. Claude Code does not read
the legacy path `C:\ProgramData\ClaudeCode\managed-settings.json`.

To send API traffic to Magpie, set `ANTHROPIC_BASE_URL` in the managed
`env` block. That is the LLM gateway base URL.

`forceLoginMethod` set to `gateway`, and `forceLoginGatewayUrl`, are
Anthropic's own gateway sign-in. The gateway-connect page says those
keys cannot be combined with `ANTHROPIC_BASE_URL`, `ANTHROPIC_API_KEY`
or `ANTHROPIC_AUTH_TOKEN`. Use one or the other. Magpie's portal is not
that sign-in product.

Managed MCP servers go in `managed-mcp.json` beside the settings file,
or in the `managedMcpServers` key. `allowedMcpServers` is the allow
list. `allowManagedMcpServersOnly` set to true, on Claude Code
v2.1.273 or later, ignores user-added servers. `deniedMcpServers` still
merges. Point the allowed URL at the proxy's MCP listener, not at a
public server.

`DISABLE_TELEMETRY` in the managed `env` block turns off Claude Code's
own operational telemetry for the sessions that policy reaches. Magpie
does not enable its own telemetry.

Example, with the host replaced:

```json
{
  "env": {
    "ANTHROPIC_BASE_URL": "https://magpie.example/anthropic",
    "DISABLE_TELEMETRY": "1"
  },
  "allowManagedMcpServersOnly": true,
  "allowedMcpServers": [
    { "serverUrl": "https://magpie.example/mcp" }
  ]
}
```

Confirm the `allowedMcpServers` object shape against the managed MCP
page before you roll it out. An invalid allow list fails closed and
blocks user-added servers.

## Codex CLI

Sources: [Managed configuration](https://developers.openai.com/codex/enterprise/managed-configuration)
and the [configuration reference](https://developers.openai.com/codex/config-reference).
On 9 October 2026 the managed-configuration page still showed
`~/.codex/managed_config.toml` as the Windows managed-defaults path.
Codex itself now ignores that default file on Windows and warns at
startup. Enforced settings belong in
`%ProgramData%\OpenAI\Codex\requirements.toml`. See
[openai/codex#38947](https://github.com/openai/codex/pull/38947).

| Purpose | Where |
| --- | --- |
| User defaults, including `openai_base_url` | `~/.codex/config.toml` (`%USERPROFILE%\.codex\config.toml`) |
| Enforced requirements on Windows | `%ProgramData%\OpenAI\Codex\requirements.toml` |
| Enforced requirements on Linux and macOS | `/etc/codex/requirements.toml` |
| Managed defaults on Linux and macOS | `/etc/codex/managed_config.toml` |
| macOS MDM | managed preferences domain `com.openai.codex`, value `requirements_toml_base64` |

`openai_base_url` is the base URL for the built-in OpenAI provider. Put
it in the user config. A project `.codex/config.toml` is not allowed to
set it. There is no published Intune ADMX for Codex. Copy
`requirements.toml` with a platform script.

MCP servers use `mcp_servers.<name>.url` for streamable HTTP. A
requirements allow list matches that URL. An example requirements
fragment:

```toml
[mcp_servers.magpie]
identity = { url = "https://magpie.example/mcp" }
```

Confirm the `identity` table against the current managed-configuration
page. The page uses `url` for HTTP servers and `command` for stdio.

## VS Code and GitHub Copilot

Sources, read on 9 October 2026:

- [Add and manage MCP servers](https://code.visualstudio.com/docs/agent-customization/mcp-servers)
- [MCP configuration reference](https://code.visualstudio.com/docs/agents/reference/mcp-configuration)
- [Enterprise AI settings](https://code.visualstudio.com/docs/enterprise/ai-settings)
- [Copilot managed settings](https://docs.github.com/copilot/reference/enterprise-managed-settings-reference)
- [MDM for Copilot settings](https://github.blog/changelog/2026-07-08-deploy-managed-copilot-settings-via-mdm-in-vs-code-and-cli/)

There is no single "Copilot base URL" that sends every model call
through Magpie. Constrain MCP, and use Defender for the direct model
endpoints.

Current MCP file locations from the VS Code page:

| Scope | File | Server object |
| --- | --- | --- |
| Workspace, VS Code format | `.vscode/mcp.json` | `servers` |
| Workspace, portable format | `.mcp.json` | `mcpServers` |
| User, portable format | `$COPILOT_HOME/mcp-config.json`, or `~/.copilot/mcp-config.json` | `mcpServers` |

The same page says the Add Server flow lists `.vscode/mcp.json` as a
compatibility destination and prefers the portable files for new
servers. An HTTP server uses `type` `http` and `url`.

Enterprise controls:

- `ChatMCP` sets `chat.mcp.access`. The enterprise page lists
  `allowed`, `registryOnly` and `off`. Older notes that say
  `all`, `registry` and `none` are out of date.
- `McpGalleryServiceUrl` points VS Code at a private MCP registry.
- Copilot `allowedMcpServers` and `deniedMcpServers` match `serverUrl`
  or `serverCommand`. A deny wins.

Device delivery for Copilot managed settings:

| Platform | Location |
| --- | --- |
| Windows MDM | `HKLM\SOFTWARE\Policies\GitHubCopilot` |
| Windows file | `%ProgramFiles%\GitHubCopilot\managed-settings.json` |
| macOS MDM | managed preferences `com.github.copilot` |
| macOS file | `/Library/Application Support/GitHubCopilot/managed-settings.json` |
| Linux file | `/etc/github-copilot/managed-settings.json` |

The file must be owned by an administrator and must not be a symlink
or world-writable. Structured values are a JSON string in the registry.

## Defender for Endpoint

Block the public model hosts the agency has not approved, and allow the
Magpie host. Indicators are created in the Defender portal under
Settings, Endpoints, Indicators. The product documents URL, domain and
IP indicators, with an action such as Block and an alert.

Source: [Manage indicators](https://learn.microsoft.com/en-us/defender-endpoint/indicator-manage).

Do not paste a hostname list from this file into production. The agency
has to choose the hosts, or the indicator will either miss a new
endpoint or block something it still needs. Generate an alert as well
as a block so the attempt shows up. This does not see traffic that
never crosses Defender.

## App Control for Business

Application control is a separate Essential Eight strategy. Restricting
MCP tools in Magpie is not application control. Use App Control for
Business to allow the signed Claude Code, Codex and VS Code binaries
you actually deploy, by publisher, and leave everything else blocked at
the maturity level you are assessing.

Source: [App Control for Business](https://learn.microsoft.com/en-us/windows/security/application-security/application-control/app-control-for-business/appcontrol).

This repository does not ship a WDAC policy. Publisher rules go stale
when the vendor changes a certificate, and a policy copied from an
example would allow the wrong thing. Build it in a pilot ring.
