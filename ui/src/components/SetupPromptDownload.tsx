import { useState } from 'react';

import { llmGatewayOrigin } from '@/gatewayUrls';
import { useLlmConfigData } from '@/hooks';
import { rolloutGuide, setupPrompt } from '@/setupPrompt';

function download(name: string, content: string) {
	const url = URL.createObjectURL(new Blob([content], { type: 'text/markdown;charset=utf-8' }));
	const link = document.createElement('a');
	link.href = url;
	link.download = name;
	link.click();
	setTimeout(() => URL.revokeObjectURL(url), 1000);
}

export function SetupPromptDownload() {
	const { config, models, virtualModels } = useLlmConfigData();
	const [client, setClient] = useState('Detect installed clients');
	const [model, setModel] = useState<string | null>(null);
	const [gateway, setGateway] = useState<string | null>(null);
	const [copied, setCopied] = useState('');
	const selectedModel =
		model ?? [...models, ...virtualModels].find(item => !item.name.includes('*'))?.name ?? '';
	const origin = gateway ?? llmGatewayOrigin(config.data);
	let validUrl = false;
	try {
		const url = new URL(origin);
		validUrl =
			['https:', 'http:'].includes(url.protocol) &&
			!url.username &&
			!url.password &&
			!url.search &&
			!url.hash &&
			url.pathname === '/';
	} catch {
		/* Show validation below. */
	}
	const content = setupPrompt({ gateway: origin.replace(/\/$/, ''), model: selectedModel, client });
	return (
		<section className="panel" aria-label="Set up your machine">
			<h2>Set up your machine</h2>
			<p>
				Download prompt.md and give it to your coding agent. It will detect your machine, configure
				your client and test the connection. Supply your gateway key locally when asked; the
				download contains no keys.
			</p>
			<div className="form-grid">
				<label className="field">
					<span>Client to configure</span>
					<select value={client} onChange={event => setClient(event.target.value)}>
						{['Detect installed clients', 'Claude Code', 'Codex', 'OpenCode'].map(item => (
							<option key={item}>{item}</option>
						))}
					</select>
				</label>
				<label className="field">
					<span>Gateway URL</span>
					<input value={origin} onChange={event => setGateway(event.target.value)} />
				</label>
				<label className="field">
					<span>Gateway model name</span>
					<input
						list="setup-prompt-models"
						value={selectedModel}
						onChange={event => setModel(event.target.value)}
						placeholder="Leave blank to choose during setup"
					/>
					<datalist id="setup-prompt-models">
						{[...models, ...virtualModels]
							.filter(item => !item.name.includes('*'))
							.map(item => (
								<option key={item.name} value={item.name} />
							))}
					</datalist>
				</label>
			</div>
			{!validUrl && (
				<p role="alert">
					Enter a gateway origin such as https://gateway.example.com, without credentials, a path or
					query parameters.
				</p>
			)}
			<div className="button-row">
				<button
					type="button"
					className="button primary"
					disabled={!validUrl}
					onClick={() => download('prompt.md', content)}
				>
					Download prompt.md
				</button>
				<button
					type="button"
					className="button"
					disabled={!validUrl}
					onClick={async () => {
						try {
							await navigator.clipboard.writeText(content);
							setCopied('Setup prompt copied.');
						} catch {
							setCopied('Copy unavailable. Use Download prompt.md.');
						}
					}}
				>
					Copy setup prompt
				</button>
				<button
					type="button"
					className="button"
					disabled={!validUrl}
					onClick={() => download('rollout.md', rolloutGuide(origin))}
				>
					Download rollout.md
				</button>
			</div>
			{copied && <p role="status">{copied}</p>}
			<details className="schema-details">
				<summary>Preview setup prompt</summary>
				<pre className="code-block" style={{ whiteSpace: 'pre-wrap', overflowWrap: 'anywhere' }}>
					{content}
				</pre>
			</details>
			<p>
				For multiple machines, reuse the shared settings and issue a separate key for each user or
				device. The rollout guide covers piloting, managed deployment, verification and rollback.
			</p>
		</section>
	);
}
