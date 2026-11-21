import { useState } from 'react';

import { Panel } from '@/components/Primitives';

function triggerDownload(filename: string, content: string, mime = 'text/plain;charset=utf-8') {
	const blob = new Blob([content], { type: mime });
	const url = URL.createObjectURL(blob);
	const a = document.createElement('a');
	a.href = url;
	a.download = filename;
	document.body.appendChild(a);
	a.click();
	a.remove();
	URL.revokeObjectURL(url);
}

function hostFromUrl(raw: string): string {
	try {
		return new URL(raw).host;
	} catch {
		return 'magpie.agency.gov.au';
	}
}

export function FleetRolloutPanel(props: { baseUrl: string }) {
	const cleanBase = (props.baseUrl.trim() || window.location.origin).replace(/\/$/, '');
	const host = hostFromUrl(cleanBase);
	const [agent, setAgent] = useState<'claude' | 'codex' | 'copilot'>('claude');

	const claudeManagedJson = JSON.stringify(
		{
			env: {
				ANTHROPIC_BASE_URL: `${cleanBase}/v1`,
				DISABLE_TELEMETRY: '1'
			},
			allowManagedMcpServersOnly: true,
			allowedMcpServers: [{ serverUrl: `${cleanBase}/mcp` }]
		},
		null,
		2
	);

	const claudeIntuneRemediate = `# Microsoft Intune Remediation Script — Claude Code Managed Settings
# Deploy via Intune -> Devices -> Scripts and remediations (Run as System, 64-bit)
$ErrorActionPreference = 'Stop'
$Dir = 'C:\\Program Files\\ClaudeCode'
$Path = Join-Path $Dir 'managed-settings.json'
$Json = @'
${claudeManagedJson}
'@
New-Item -ItemType Directory -Path $Dir -Force | Out-Null
Set-Content -Path $Path -Value $Json -Encoding UTF8
$RegPath = 'HKLM:\\SOFTWARE\\Policies\\ClaudeCode'
New-Item -Path $RegPath -Force | Out-Null
Set-ItemProperty -Path $RegPath -Name 'Settings' -Value $Json -Type String
Write-Output "Applied Magpie managed settings for Claude Code -> ${cleanBase}"
`;

	const claudeIntuneDetect = `# Microsoft Intune Detection Script — Claude Code Managed Settings
$Path = 'C:\\Program Files\\ClaudeCode\\managed-settings.json'
if (-not (Test-Path $Path)) { exit 1 }
$Raw = Get-Content $Path -Raw
if ($Raw -match [regex]::Escape('${cleanBase}')) { exit 0 }
exit 1
`;

	const codexReqToml = `# Enforced Codex CLI requirements — %ProgramData%\\OpenAI\\Codex\\requirements.toml
[mcp_servers.magpie]
identity = { url = "${cleanBase}/mcp" }
`;

	const codexIntuneRemediate = `# Microsoft Intune Remediation Script — Codex CLI Enforced Requirements
$ErrorActionPreference = 'Stop'
$Dir = Join-Path $env:ProgramData 'OpenAI\\Codex'
$Path = Join-Path $Dir 'requirements.toml'
$Toml = @'
${codexReqToml}
'@
New-Item -ItemType Directory -Path $Dir -Force | Out-Null
Set-Content -Path $Path -Value $Toml -Encoding UTF8
Write-Output "Applied Magpie Codex requirements -> ${cleanBase}/mcp"
`;

	const copilotManagedJson = JSON.stringify(
		{
			ChatMCP: 'registryOnly',
			allowedMcpServers: [{ serverUrl: `${cleanBase}/mcp` }]
		},
		null,
		2
	);

	const copilotIntuneRemediate = `# Microsoft Intune Remediation Script — GitHub Copilot & VS Code Managed MCP Policy
$ErrorActionPreference = 'Stop'
$Dir = Join-Path $env:ProgramFiles 'GitHubCopilot'
$Path = Join-Path $Dir 'managed-settings.json'
$Json = @'
${copilotManagedJson}
'@
New-Item -ItemType Directory -Path $Dir -Force | Out-Null
Set-Content -Path $Path -Value $Json -Encoding UTF8
$RegPath = 'HKLM:\\SOFTWARE\\Policies\\GitHubCopilot'
New-Item -Path $RegPath -Force | Out-Null
Set-ItemProperty -Path $RegPath -Name 'ChatMCP' -Value 'registryOnly' -Type String
Write-Output "Applied Magpie GitHub Copilot policy -> ${cleanBase}/mcp"
`;

	const defenderCsv = [
		'IndicatorType,IndicatorValue,ExpirationTime,Action,Severity,Title,Description,RecommendedActions,RbacGroups,Category,MitreTechniques,GenerateAlert',
		`DomainName,${host},,Allowed,Informational,"Allow Magpie Gateway","Approved sovereign AI gateway endpoint","Permit traffic to approved Magpie Gateway",,,TRUE`,
		'DomainName,api.openai.com,,Warn,Medium,"Review direct OpenAI traffic","Example template: change Action to Block after pilot sign-off","Route coding agents through Magpie Gateway instead",,,TRUE',
		'DomainName,api.anthropic.com,,Warn,Medium,"Review direct Anthropic traffic","Example template: change Action to Block after pilot sign-off","Route coding agents through Magpie Gateway instead",,,TRUE'
	].join('\n');

	const activeScript =
		agent === 'claude'
			? claudeIntuneRemediate
			: agent === 'codex'
				? codexIntuneRemediate
				: copilotIntuneRemediate;

	const activeFilename =
		agent === 'claude'
			? 'Intune-Remediate-ClaudeCode-Magpie.ps1'
			: agent === 'codex'
				? 'Intune-Remediate-Codex-Magpie.ps1'
				: 'Intune-Remediate-Copilot-Magpie.ps1';

	return (
		<Panel>
			<div className="cc-panel-head">
				<h2>Enterprise Fleet Rollout (Microsoft Intune & Defender)</h2>
				<p className="muted-copy">
					Generate secret-free managed configuration scripts pre-filled with your live gateway URL (
					<code>{cleanBase}</code>) and MCP endpoint (<code>{cleanBase}/mcp</code>). Never bundle a
					shared API key into an Intune package — distribute per-user keys or Entra tokens
					separately.
				</p>
			</div>

			<fieldset className="cc-filter-bar cc-fieldset-bare">
				<legend className="sr-only">Choose managed coding agent</legend>
				<button
					type="button"
					className={`button${agent === 'claude' ? ' primary' : ''}`}
					aria-pressed={agent === 'claude'}
					onClick={() => setAgent('claude')}
				>
					Claude Code (Intune)
				</button>
				<button
					type="button"
					className={`button${agent === 'codex' ? ' primary' : ''}`}
					aria-pressed={agent === 'codex'}
					onClick={() => setAgent('codex')}
				>
					Codex CLI (Intune)
				</button>
				<button
					type="button"
					className={`button${agent === 'copilot' ? ' primary' : ''}`}
					aria-pressed={agent === 'copilot'}
					onClick={() => setAgent('copilot')}
				>
					VS Code & GitHub Copilot (Intune)
				</button>
			</fieldset>

			<pre className="cc-code-block">{activeScript}</pre>

			<div className="cc-preview-actions">
				<button
					type="button"
					className="button primary"
					onClick={() => triggerDownload(activeFilename, activeScript)}
				>
					Download Intune Remediation (.ps1)
				</button>
				{agent === 'claude' ? (
					<button
						type="button"
						className="button"
						onClick={() =>
							triggerDownload('Intune-Detect-ClaudeCode-Magpie.ps1', claudeIntuneDetect)
						}
					>
						Download Intune Detection (.ps1)
					</button>
				) : null}
				<button
					type="button"
					className="button"
					onClick={() =>
						triggerDownload('Defender-Magpie-Indicators.csv', defenderCsv, 'text/csv;charset=utf-8')
					}
				>
					Download Defender Indicators (.csv)
				</button>
			</div>
		</Panel>
	);
}
