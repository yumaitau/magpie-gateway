import { useQuery } from '@tanstack/react-query';
import { useMemo, useState } from 'react';

import { listCostModels } from '@/api/costsApi';
import { FreeformCombobox } from '@/components/FreeformCombobox';
import { useModelCatalog } from '@/modelCatalog';

export function CatalogModelSelector(props: {
	ariaLabel: string;
	value: string;
	onChange: (value: string) => void;
	provider?: string | null;
	placeholder?: string;
}) {
	const catalog = useQuery({
		queryKey: ['cost-models'],
		queryFn: listCostModels,
		staleTime: 60_000,
		retry: false
	});
	const synced = useModelCatalog();
	const [selection, setSelection] = useState({ provider: props.provider, source: '' });
	const needsSource = !props.provider || props.provider === 'custom';
	const source = selection.provider === props.provider ? selection.source : '';
	const selectedProvider = needsSource ? source : props.provider;
	const options = useMemo(() => {
		const providers = [...(catalog.data?.providers ?? []), ...(synced.data?.providers ?? [])];
		const keys = catalogProviderKeys(selectedProvider);
		const matched = keys.length
			? providers.filter(provider => keys.includes(provider.provider.toLowerCase()))
			: providers;
		const source = matched.length ? matched : [];
		return [...new Set(source.flatMap(provider => provider.models))].sort((a, b) =>
			a.localeCompare(b)
		);
	}, [catalog.data, synced.data, selectedProvider]);

	return (
		<div className="catalog-model-selector">
			{needsSource && synced.data?.providers.length ? (
				<div className="field">
					<span className="field-label">Catalog provider</span>
					<select
						aria-label={`${props.ariaLabel} catalog provider`}
						value={source}
						onChange={event =>
							setSelection({ provider: props.provider, source: event.target.value })
						}
					>
						<option value="">All catalog providers</option>
						{synced.data.providers.map(item => (
							<option key={item.provider} value={item.provider}>
								{item.name}
							</option>
						))}
					</select>
				</div>
			) : null}
			<FreeformCombobox
				ariaLabel={props.ariaLabel}
				value={props.value}
				options={options}
				onChange={props.onChange}
				placeholder={props.placeholder ?? 'Select or type a model'}
				emptyText={
					catalog.isLoading
						? 'Loading model catalog...'
						: 'No catalog matches. Custom model names are allowed.'
				}
			/>
		</div>
	);
}

function catalogProviderKeys(provider: string | null | undefined) {
	const normalized = (provider ?? '').trim().toLowerCase();
	const aliases: Record<string, string[]> = {
		bedrock: ['aws.bedrock'],
		gemini: ['gcp.gemini', 'google'],
		google: ['gcp.gemini', 'google'],
		vertex: ['gcp.vertex_ai', 'google-vertex'],
		'vertex ai': ['gcp.vertex_ai', 'google-vertex'],
		openai: ['openai'],
		openaii: ['openai'],
		openai_legacy: ['openai'],
		anthropic: ['anthropic'],
		claude: ['anthropic'],
		codex: ['openai'],
		grok: ['xai'],
		'github-copilot': ['copilot'],
		azure: ['azure'],
		copilot: ['copilot'],
		typesafe: ['typesafe']
	};
	return [normalized, ...(aliases[normalized] ?? [])]
		.filter(Boolean)
		.map(item => item.toLowerCase());
}
