import { useState } from 'react';

import { isMagpie, magpieApi } from '@/magpie';
import { type SyncedModelCatalog, useModelCatalog } from '@/modelCatalog';

export function ModelCatalogStatus() {
	const catalog = useModelCatalog();
	const [syncing, setSyncing] = useState(false);
	const [error, setError] = useState('');
	if (!isMagpie) return null;
	async function sync() {
		setSyncing(true);
		setError('');
		try {
			await magpieApi<SyncedModelCatalog>('/api/magpie/model-catalog', {});
			await catalog.refetch();
		} catch (error) {
			setError(error instanceof Error ? error.message : 'Sync failed.');
		} finally {
			setSyncing(false);
		}
	}
	return (
		<details className="panel">
			<summary>
				Model catalog ·{' '}
				{catalog.error || catalog.data?.error
					? 'Sync needs attention'
					: catalog.data?.syncedAt
						? 'Auto-sync hourly'
						: 'Waiting for first sync'}
			</summary>
			<p>
				Suggestions sync from models.dev, the catalog used by OpenCode, on startup and every hour.
				New models still need a configured provider, account access and any required approval.
			</p>
			<p>
				{catalog.data?.syncedAt
					? `Last successful sync: ${new Date(catalog.data.syncedAt).toLocaleString()}.`
					: 'Using bundled and custom suggestions until the first sync succeeds.'}
			</p>
			{error || catalog.data?.error || catalog.error ? (
				<p role="alert">{error || catalog.data?.error || catalog.error?.message}</p>
			) : null}
			{catalog.data?.canSync ? (
				<button type="button" className="button" disabled={syncing} onClick={sync}>
					{syncing ? 'Syncing models…' : 'Sync now'}
				</button>
			) : null}
		</details>
	);
}
