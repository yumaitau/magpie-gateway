import { useMemo } from 'react';

import { Dropdown } from '@/components/Primitives';
import { isMagpie } from '@/magpie';
import { matchesModelSearch, useModelCatalog } from '@/modelCatalog';
import { isWildcardModelName, modelCatalogProvider, resolveModelName } from '@/modelResolution';
import type { LlmModel, LlmProvider } from '@/types';

export type CatalogSelection = { provider: string; model: string };
export function GatewayModelPicker(props: {
	ariaLabel?: string;
	purpose?: 'playground' | 'setup' | 'routing';
	value: string;
	models: LlmModel[];
	providers: LlmProvider[];
	configuredOptions: { value: string; label: string; searchText: string; description?: string }[];
	catalogSelection: CatalogSelection | null;
	onCatalogSelection: (selection: CatalogSelection | null) => void;
	onSelect: (name: string, specific?: string) => void;
}) {
	const catalog = useModelCatalog();
	const entries = useMemo(
		() =>
			(catalog.data?.providers ?? []).flatMap(provider =>
				provider.models.map(model => ({
					provider: provider.provider,
					providerName: provider.name,
					model
				}))
			),
		[catalog.data]
	);
	function routeFor(item: CatalogSelection) {
		const routes = props.models.filter(
			route => modelCatalogProvider(route, props.providers) === item.provider
		);
		return (
			routes.find(
				route =>
					!isWildcardModelName(route.name) && (route.params?.model ?? route.name) === item.model
			) ??
			routes.find(
				route =>
					route.name.endsWith('*') &&
					route.name.indexOf('*') === route.name.length - 1 &&
					!route.params?.model
			)
		);
	}
	return (
		<>
			<Dropdown
				ariaLabel={props.ariaLabel ?? 'Model'}
				resultLimit={100}
				searchMatcher={matchesModelSearch}
				allowEmpty
				value={
					props.catalogSelection
						? JSON.stringify(props.catalogSelection)
						: JSON.stringify({ route: props.value })
				}
				placeholder="Choose a model"
				searchable
				options={[
					...props.configuredOptions.map(item => ({
						...item,
						value: JSON.stringify({ route: item.value }),
						description: item.description
					})),
					...entries.map(item => ({
						value: JSON.stringify({ provider: item.provider, model: item.model }),
						label: item.model,
						description: `${item.providerName} · ${routeFor(item) ? 'Configured route available' : 'Setup required'}`,
						searchText: `${item.providerName} ${item.provider} ${item.model} ${item.provider === 'openai' ? 'Codex ChatGPT' : ''}`
					}))
				]}
				onChange={value => {
					const item = JSON.parse(value) as CatalogSelection & { route?: string };
					if (item.route !== undefined) {
						props.onCatalogSelection(null);
						props.onSelect(item.route);
						return;
					}
					const route = routeFor(item);
					if (route) {
						props.onCatalogSelection(null);
						props.onSelect(
							route.name,
							isWildcardModelName(route.name) ? resolveModelName(route, item.model) : undefined
						);
					} else props.onCatalogSelection(item);
				}}
			/>
			{isMagpie && (
				<small>
					{entries.length.toLocaleString()} synced catalog entries plus configured routes. Search by
					model or provider.
				</small>
			)}
			{catalog.error && (
				<p role="alert">
					Could not load the catalog. Configured routes remain available. Reload to retry.
				</p>
			)}
			{props.catalogSelection && (
				<div role="status">
					<p>
						{props.catalogSelection.model}{' '}
						{props.purpose === 'setup'
							? 'needs a gateway connection. The bootstrap prompt includes this setup requirement.'
							: 'needs a configured connection before it can be used.'}{' '}
						Your gateway key and approval rules still apply.
					</p>
					<a
						className="button"
						href={`/ui/llm/models?${new URLSearchParams({ catalogProvider: props.catalogSelection.provider, catalogModel: props.catalogSelection.model })}#add=model`}
					>
						Set up this model
					</a>
				</div>
			)}
		</>
	);
}
