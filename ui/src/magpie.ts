import { useQuery } from '@tanstack/react-query';
export const isMagpie = import.meta.env.VITE_MAGPIE === 'true';
export type MagpieSession = {
	name: string;
	email: string;
	subject: string;
	canLogout: boolean;
	canReadGateway: boolean;
	canManagePeople: boolean;
	canUseSubscriptionPools?: boolean;
	canChangeEnforcement: boolean;
	mode: string;
	enforcingGroups?: string[];
	trialGroups?: string[];
	links: { href: string; label: string }[];
};
export function enforcementLabel(session: Pick<MagpieSession, 'mode' | 'enforcingGroups' | 'trialGroups'>) {
	const enforcing = session.enforcingGroups?.length ?? 0;
	const trial = session.trialGroups?.length ?? 0;
	if (session.mode === 'log-only') {
		return enforcing === 0 ? 'Trial' : `Trial · ${enforcing} enforcing`;
	}
	return trial === 0 ? 'Enforcing' : `Enforcing · ${trial} on trial`;
}
export async function magpieApi<T>(path: string, body?: unknown): Promise<T> {
	const response = await fetch(path, {
		credentials: 'same-origin',
		headers: { 'Content-Type': 'application/json' },
		...(body === undefined ? {} : { method: 'POST', body: JSON.stringify(body) })
	});
	const result = await response.json().catch(() => ({ error: response.statusText }));
	if (!response.ok) throw new Error(result.error || 'The request could not be completed.');
	return result;
}
export function useMagpieSession() {
	return useQuery<MagpieSession>({
		queryKey: ['magpie-session'],
		enabled: isMagpie,
		queryFn: async () => {
			const r = await fetch('/api/magpie/session');
			if (r.status === 401) {
				location.assign('/ui/login');
				throw new Error('Sign in required');
			}
			if (!r.ok) throw new Error('Session unavailable');
			return r.json();
		},
		retry: false,
		refetchInterval: 30000
	});
}
