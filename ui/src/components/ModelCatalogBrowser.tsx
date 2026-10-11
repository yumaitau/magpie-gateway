import { useMemo, useState } from 'react';

import { makeEmptyModel } from '@/config';
import { matchesModelSearch, useModelCatalog } from '@/modelCatalog';
import { modelCatalogProvider } from '@/modelResolution';
import type { LlmModel, LlmProvider } from '@/types';

export function ModelCatalogBrowser(props: {
	providers: LlmProvider[];
	configuredNames: string[];
	configureUnavailableReason?: string | null;
	onConfigure: (model: LlmModel) => void;
}) {
	const catalog = useModelCatalog();
	const [query, setQuery] = useState('');
	const [provider, setProvider] = useState('');
	const [page, setPage] = useState(0);
	const size = 30;
	const providers = useMemo(() => {
		const preferred = [
			'anthropic',
			'openai',
			'xai',
			'opencode',
			'opencode-go',
			'copilot',
			'aws.bedrock'
		];
		const rank = (id: string) =>
			preferred.includes(id) ? preferred.indexOf(id) : preferred.length;
		return [...(catalog.data?.providers ?? [])].sort(
			(a, b) => rank(a.provider) - rank(b.provider) || a.name.localeCompare(b.name)
		);
	}, [catalog.data]);
	const rows = useMemo(
		() =>
			providers
				.filter(item => !provider || item.provider === provider)
				.flatMap(item => item.models.map(model => ({ ...item, model })))
				.filter(item =>
					matchesModelSearch(
						`${item.name} ${item.provider} ${item.model} ${item.provider === 'openai' ? 'Codex ChatGPT' : ''}`,
						query
					)
				),
		[providers, provider, query]
	);
	const current = Math.min(page, Math.max(0, Math.ceil(rows.length / size) - 1));
	const total = catalog.data?.providers.reduce((count, item) => count + item.models.length, 0) ?? 0;
	function configure(source: string, model: string) {
		const shared = props.providers.find(
			item => modelCatalogProvider(item, props.providers) === source
		);
		const native: Record<string, LlmModel['provider']> = {
			anthropic: 'anthropic',
			openai: 'openai',
			'aws.bedrock': 'bedrock',
			'gcp.gemini': 'gemini',
			'gcp.vertex_ai': 'vertex',
			copilot: 'copilot',
			azure: 'azure'
		};
		let name = `${source}/${model}`;
		let suffix = 2;
		while (props.configuredNames.includes(name)) name = `${source}/${model}-${suffix++}`;
		// Reuse a configured connection without copying its credentials into the model.
		props.onConfigure({
			...makeEmptyModel(),
			name,
			provider: shared ? { reference: shared.name } : (native[source] ?? null),
			params: { model }
		} as LlmModel);
	}
	return (
		<section className="panel" aria-label="Browse model catalog">
			<h2>Browse model catalog</h2>
			<p>
				{total.toLocaleString()} model entries across {catalog.data?.providers.length ?? 0}{' '}
				providers. These are catalog entries; configure a connection and obtain any required
				approval to use one.
			</p>
			<div className="form-grid">
				<label className="field">
					<span className="field-label">Search all models</span>
					<input
						type="search"
						value={query}
						placeholder="Try Opus 5.5, Codex, Grok or OpenCode"
						onChange={event => {
							setQuery(event.target.value);
							setPage(0);
						}}
					/>
				</label>
				<label className="field">
					<span className="field-label">Catalog provider</span>
					<select
						aria-label="Catalog provider"
						value={provider}
						onChange={event => {
							setProvider(event.target.value);
							setPage(0);
						}}
					>
						<option value="">All providers</option>
						{providers.map(item => (
							<option key={item.provider} value={item.provider}>
								{item.name}
							</option>
						))}
					</select>
				</label>
			</div>
			{catalog.error ? <p role="alert">{catalog.error.message}</p> : null}
			{props.configureUnavailableReason ? (
				<p role="status">{props.configureUnavailableReason}</p>
			) : null}
			{!total ? (
				<p>
					{catalog.isLoading
						? 'Loading model catalog…'
						: 'The catalog has not synced yet. Expand Model catalog above to check sync status or sync now.'}
				</p>
			) : null}
			{total > 0 && !rows.length ? (
				<p>No matches. Try fewer words or choose All providers.</p>
			) : null}
			{rows.length > 0 ? (
				<>
					<p aria-live="polite">
						Showing {current * size + 1}–{Math.min((current + 1) * size, rows.length)} of{' '}
						{rows.length.toLocaleString()} matching entries
					</p>
					<div className="table-wrap">
						<table>
							<thead>
								<tr>
									<th>Model ID</th>
									<th>Provider</th>
									<th>Setup</th>
								</tr>
							</thead>
							<tbody>
								{rows.slice(current * size, (current + 1) * size).map(item => (
									<tr key={`${item.provider}/${item.model}`}>
										<td className="catalog-model-id">{item.model}</td>
										<td>{item.name}</td>
										<td>
											<button
												className="button"
												type="button"
												aria-label={`Configure ${item.model} via ${item.name}`}
												disabled={Boolean(props.configureUnavailableReason)}
												onClick={() => configure(item.provider, item.model)}
											>
												Configure
											</button>
										</td>
									</tr>
								))}
							</tbody>
						</table>
					</div>
					<div className="button-row">
						<button
							type="button"
							className="button"
							disabled={current === 0}
							onClick={() => setPage(current - 1)}
						>
							Previous models
						</button>
						<button
							type="button"
							className="button"
							disabled={(current + 1) * size >= rows.length}
							onClick={() => setPage(current + 1)}
						>
							Next models
						</button>
					</div>
				</>
			) : null}
		</section>
	);
}
