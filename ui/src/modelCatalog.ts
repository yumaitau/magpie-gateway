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
