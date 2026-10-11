import { useQuery } from '@tanstack/react-query';

import { isMagpie, magpieApi } from '@/magpie';

export type SyncedModelCatalog = {
	providers: { provider: string; name: string; models: string[] }[];
	syncedAt: string | null;
	error: string | null;
	canSync: boolean;
	intervalSeconds: number;
};
export function useModelCatalog() {
	return useQuery({
		queryKey: ['magpie-model-catalog'],
		queryFn: () => magpieApi<SyncedModelCatalog>('/api/magpie/model-catalog'),
		enabled: isMagpie,
		staleTime: 30_000,
		refetchInterval: 60_000,
		retry: false
	});
}

// Search display names and IDs alike: “Opus 5.5” also finds “claude-opus-5-5”.
export function matchesModelSearch(value: string, query: string) {
	const normalize = (text: string) =>
		text
			.toLowerCase()
			.replace(/[^a-z0-9]+/g, ' ')
			.trim();
	const haystack = normalize(value);
	return query
		.trim()
		.split(/\s+/)
		.every(token => haystack.includes(normalize(token)));
}
