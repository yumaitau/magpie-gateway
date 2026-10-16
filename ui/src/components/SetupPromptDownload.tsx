import { useRef, useState } from 'react';

import { type CatalogSelection, GatewayModelPicker } from '@/components/GatewayModelPicker';
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
	const { config, models, virtualModels, providers } = useLlmConfigData();
	const [client, setClient] = useState('This coding agent');
	const [model, setModel] = useState('');
	const [specificModel, setSpecificModel] = useState('');
	const [catalogSelection, setCatalogSelection] = useState<CatalogSelection | null>(null);
	const [gateway, setGateway] = useState<string | null>(null);
	const [copied, setCopied] = useState('');
	const promptRef = useRef<HTMLTextAreaElement>(null);
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
	const content = setupPrompt({
		gateway: origin.replace(/\/$/, ''),
		model: catalogSelection?.model ?? (model.includes('*') ? specificModel || model : model),
		client,
		catalogProvider: catalogSelection?.provider
	});
	return (
		<section className="panel" aria-label="Set up your machine">
			<h2>Set up your machine</h2>
			<p>
				Copy this prompt into your coding agent. It will set up the connection and test it. You can
				leave the model on automatic.
			</p>
			<div className="field">
				<span>Model (optional)</span>
				<GatewayModelPicker
					purpose="setup"
					value={model}
					models={models}
					providers={providers}
					configuredOptions={[
						{
							value: '',
							label: 'Let my agent choose',
							searchText: 'automatic auto detect choose',
							description: 'Discover models using your gateway key'
						},
						...[...models, ...virtualModels].map(item => ({
							value: item.name,
							label: item.name,
							searchText: item.name
						}))
					]}
					catalogSelection={catalogSelection}
					onCatalogSelection={item => {
						setCatalogSelection(item);
						setCopied('');
					}}
					onSelect={(name, specific) => {
						setModel(name);
						setSpecificModel(specific ?? '');
						setCopied('');
					}}
				/>
			</div>
			<label className="field">
				<span>Bootstrap prompt</span>
				<textarea ref={promptRef} readOnly rows={10} value={content} spellCheck={false} />
			</label>
			<button
				type="button"
				className="button primary"
				disabled={!validUrl}
				onClick={async () => {
					try {
						await navigator.clipboard.writeText(content);
						setCopied('Prompt copied. Paste it into your coding agent.');
					} catch {
						promptRef.current?.focus();
						promptRef.current?.select();
						setCopied('Prompt selected. Press Ctrl+C or Command+C to copy.');
					}
				}}
			>
				Copy bootstrap prompt
			</button>
			{copied && <p role="status">{copied}</p>}
			{!validUrl && (
				<p role="alert">
					Enter a gateway origin such as https://gateway.example.com, without credentials, a path or
					query parameters.
				</p>
			)}
			<details className="schema-details">
				<summary>Options and downloads</summary>
				<div className="form-grid">
					<label className="field">
						<span>Client to configure</span>
						<select
							value={client}
							onChange={event => {
								setClient(event.target.value);
								setCopied('');
							}}
						>
							{['This coding agent', 'Cursor', 'Claude Code', 'Codex', 'OpenCode'].map(item => (
								<option key={item}>{item}</option>
							))}
						</select>
					</label>
					<label className="field">
						<span>Gateway URL</span>
						<input
							value={origin}
							onChange={event => {
								setGateway(event.target.value);
								setCopied('');
							}}
						/>
					</label>
					<label className="field">
						<span>Gateway model name (manual override)</span>
						<input
							value={model.includes('*') ? specificModel || model : model}
							onChange={event => {
								setCatalogSelection(null);
								setModel(event.target.value);
								setSpecificModel('');
								setCopied('');
							}}
							placeholder="Optional: exact gateway route name"
						/>
					</label>
				</div>
				<div className="button-row">
					<button
						type="button"
						className="button"
						disabled={!validUrl}
						onClick={() => download('prompt.md', content)}
					>
						Download prompt.md
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
				<p>
					Downloads contain no API keys. For multiple machines, use the rollout guide and separate
					credentials for each user or device.
				</p>
			</details>
		</section>
	);
}
