import { useQuery } from '@tanstack/react-query';
import { useState } from 'react';

import { PageHeader, StatusBanner } from '@/components/Primitives';
import { isMagpie, magpieApi } from '@/magpie';

type Pool = { id: string; name: string; owner: string; shared: boolean; enabled: boolean };
type Subscription = {
	id: string;
	name: string;
	provider: 'claude' | 'codex';
	models: string[];
	enabled: boolean;
	maxParallel: number;
	active: number;
	status: string;
	cooldownUntil: number;
};
type PoolKey = { id: string; name: string; models: string[]; expiresAt: number; revoked: boolean };
type Entry = {
	pool: Pool;
	subscriptions: Subscription[];
	keys: PoolKey[];
	canManage: boolean;
	canManageKeys: boolean;
	canIssue: boolean;
};
type Pools = { pools: Entry[]; canCreate: boolean };
type Issued = { token: string; key: PoolKey };

export function SubscriptionPoolsPage() {
	const query = useQuery({
		queryKey: ['subscription-pools'],
		enabled: isMagpie,
		queryFn: () => magpieApi<Pools>('/api/magpie/pools'),
		refetchInterval: 15000
	});
	const [error, setError] = useState('');
	const [busy, setBusy] = useState(false);
	const [issued, setIssued] = useState<Issued | null>(null);
	const [poolName, setPoolName] = useState('');
	const [shared, setShared] = useState(false);
	const [copied, setCopied] = useState(false);
	const [showKey, setShowKey] = useState(false);
	async function change(body: unknown) {
		setBusy(true);
		setError('');
		try {
			const result = await magpieApi<Partial<Issued>>('/api/magpie/pools', body);
			if (result.token && result.key) {
				setIssued({ token: result.token, key: result.key });
				setCopied(false);
				setShowKey(false);
			}
			await query.refetch();
			return true;
		} catch (cause) {
			setError(cause instanceof Error ? cause.message : 'Could not save the change.');
			return false;
		} finally {
			setBusy(false);
		}
	}
	if (!isMagpie) return <p>Subscription pools are available in Magpie Gateway.</p>;
	return (
		<div className="page-stack">
			<PageHeader
				title="Subscription pools"
				description="Share Claude and ChatGPT subscription capacity through scoped API keys. Each subscription keeps its own usage limits."
			/>
			{error || query.error ? (
				<StatusBanner state="bad" title="Pool operation failed">
					{error || query.error?.message}
				</StatusBanner>
			) : null}
			{query.isLoading ? <p role="status">Loading subscription pools…</p> : null}
			{issued ? (
				<section className="card page-stack" aria-label="New pool API key">
					<h3>Save your key for {issued.key.name}</h3>
					<p>This key appears once. Keep it in your client’s secret store.</p>
					<label>
						API key
						<input
							readOnly
							type={showKey ? 'text' : 'password'}
							value={issued.token}
							autoComplete="off"
						/>
					</label>
					<div className="button-row">
						<button
							className="button"
							type="button"
							aria-pressed={showKey}
							onClick={() => setShowKey(value => !value)}
						>
							{showKey ? 'Hide API key' : 'Show API key'}
						</button>
						<button
							className="button"
							type="button"
							onClick={() => {
								if (!navigator.clipboard) {
									setError('Clipboard unavailable. Use HTTPS or localhost to copy this key.');
									return;
								}
								void navigator.clipboard.writeText(issued.token).then(
									() => setCopied(true),
									() => setError('Clipboard unavailable. Select and copy the key from the field.')
								);
							}}
						>
							{copied ? 'Copied' : 'Copy API key'}
						</button>
						<button className="button" type="button" onClick={() => setIssued(null)}>
							Dismiss key
						</button>
					</div>
				</section>
			) : null}
			<section className="card page-stack" aria-label="Connect clients to a pool">
				<h3>Connect your client</h3>
				<p>
					Use <code>{location.origin}/pool/v1</code> as the API base URL. Claude models use
					<code> /pool/v1/messages</code>; Codex models use <code>/pool/v1/responses</code>. Both
					support streaming. Use an exact model ID allowed by your key.
				</p>
				<pre>{`# Claude Code\nexport ANTHROPIC_BASE_URL=${location.origin}/pool\nexport ANTHROPIC_AUTH_TOKEN="$MAGPIE_POOL_KEY"\n# Select a Claude model from your pool with --model MODEL_ID\n\n# Codex: ~/.codex/config.toml\nmodel = "MODEL_ID_FROM_YOUR_POOL"\nmodel_provider = "magpie_pool"\n[model_providers.magpie_pool]\nname = "Magpie subscription pool"\nbase_url = "${location.origin}/pool/v1"\nenv_key = "MAGPIE_POOL_KEY"\nwire_api = "responses"`}</pre>
				<p className="muted-copy">
					Pool traffic uses Magpie’s approvals, DLP and enforcement settings. Models must be
					approved for enforced users. Subscription destinations are global.
				</p>
			</section>
			{query.data?.canCreate ? (
				<form
					className="card page-stack"
					onSubmit={event => {
						event.preventDefault();
						void change({ action: 'savePool', name: poolName, shared, enabled: true }).then(ok => {
							if (ok) setPoolName('');
						});
					}}
				>
					<h3>Create a pool</h3>
					<label>
						Pool name
						<input
							required
							maxLength={100}
							value={poolName}
							onChange={e => setPoolName(e.target.value)}
						/>
					</label>
					<label className="checkbox-row">
						<input type="checkbox" checked={shared} onChange={e => setShared(e.target.checked)} />
						Share with signed-in colleagues so they can issue their own keys
					</label>
					<button className="button primary" type="submit" disabled={busy}>
						Create pool
					</button>
				</form>
			) : null}
			{query.data?.pools.length === 0 ? (
				<p>No subscription pools are available to you yet.</p>
			) : null}
			{query.data?.pools.map(entry => (
				<PoolPanel key={entry.pool.id} entry={entry} busy={busy} change={change} />
			))}
		</div>
	);
}

function PoolPanel({
	entry,
	busy,
	change
}: {
	entry: Entry;
	busy: boolean;
	change: (body: unknown) => Promise<boolean>;
}) {
	const { pool } = entry;
	const [editing, setEditing] = useState<Subscription | 'new' | null>(null);
	const [keyName, setKeyName] = useState('');
	const [days, setDays] = useState(30);
	const [selected, setSelected] = useState<string[]>([]);
	const models = [
		...new Set(entry.subscriptions.filter(s => s.enabled).flatMap(s => s.models))
	].sort();
	const allowed = selected.filter(model => models.includes(model));
	return (
		<section className="card page-stack" aria-label={`Pool ${pool.name}`}>
			<h3>
				{pool.name} <span className="badge">{pool.shared ? 'Shared' : 'Private'}</span>{' '}
				<span className="badge">{pool.enabled ? 'Enabled' : 'Disabled'}</span>
			</h3>
			{entry.canManage ? (
				<div className="button-row">
					<button
						className="button"
						type="button"
						disabled={busy}
						onClick={() => void change({ action: 'savePool', ...pool, enabled: !pool.enabled })}
					>
						{pool.enabled ? 'Disable pool' : 'Enable pool'}
					</button>
					<button
						className="button"
						type="button"
						disabled={busy}
						onClick={() => void change({ action: 'savePool', ...pool, shared: !pool.shared })}
					>
						{pool.shared ? 'Make private' : 'Share pool'}
					</button>
					<button
						className="button"
						type="button"
						disabled={busy}
						onClick={() => setEditing('new')}
					>
						Connect subscription
					</button>
				</div>
			) : null}
			{entry.subscriptions.map(s => (
				<article key={s.id} className="page-stack">
					<strong>
						{s.name} · {s.provider === 'claude' ? 'Claude' : 'ChatGPT / Codex'}
					</strong>
					<p>{s.models.join(', ')}</p>
					<p className="muted-copy">
						{s.active}/{s.maxParallel} slots in use ·{' '}
						{!s.enabled
							? 'Disabled'
							: s.status === 'reauthenticate'
								? 'Reconnect credentials'
								: s.cooldownUntil > Date.now() / 1000
									? `Limited until ${new Date(s.cooldownUntil * 1000).toLocaleString()}`
									: 'Available'}
					</p>
					{entry.canManage ? (
						<div className="button-row">
							<button
								className="button small"
								type="button"
								disabled={busy}
								onClick={() => setEditing(s)}
							>
								Edit or reconnect {s.name}
							</button>
							<button
								className="button small"
								type="button"
								disabled={busy}
								onClick={() =>
									void change({
										action: 'saveSubscription',
										poolId: pool.id,
										...s,
										enabled: !s.enabled
									})
								}
							>
								{s.enabled ? 'Disable' : 'Enable'} {s.name}
							</button>
							<button
								className="button small danger"
								type="button"
								disabled={busy}
								onClick={() =>
									void change({ action: 'removeSubscription', poolId: pool.id, id: s.id })
								}
							>
								Disconnect {s.name}
							</button>
						</div>
					) : null}
				</article>
			))}
			{entry.subscriptions.length === 0 ? (
				<p>No subscriptions connected. Connect a Claude setup-token or a Codex login file.</p>
			) : null}
			{editing && entry.canManage ? (
				<SubscriptionForm
					key={editing === 'new' ? 'new' : editing.id}
					initial={editing === 'new' ? undefined : editing}
					busy={busy}
					onCancel={() => setEditing(null)}
					onSave={async body => {
						if (await change({ action: 'saveSubscription', poolId: pool.id, ...body }))
							setEditing(null);
					}}
				/>
			) : null}
			{entry.canIssue && pool.enabled && models.length > 0 ? (
				<form
					className="page-stack"
					onSubmit={event => {
						event.preventDefault();
						void change({
							action: 'issueKey',
							poolId: pool.id,
							name: keyName,
							models: allowed,
							expiresInDays: days
						}).then(ok => {
							if (ok) setKeyName('');
						});
					}}
				>
					<h4>Issue an API key</h4>
					<label>
						Key name
						<input
							required
							maxLength={100}
							value={keyName}
							onChange={e => setKeyName(e.target.value)}
						/>
					</label>
					<fieldset>
						<legend>Allowed models</legend>
						{models.map(model => (
							<label className="checkbox-row" key={model}>
								<input
									type="checkbox"
									checked={selected.includes(model)}
									onChange={e =>
										setSelected(current =>
											e.target.checked ? [...current, model] : current.filter(m => m !== model)
										)
									}
								/>
								{model}
							</label>
						))}
					</fieldset>
					<label>
						Expires after (days)
						<input
							type="number"
							required
							min={1}
							max={365}
							value={days}
							onChange={e => setDays(e.target.valueAsNumber)}
						/>
					</label>
					<button className="button primary" type="submit" disabled={busy || allowed.length === 0}>
						Issue pool API key
					</button>
				</form>
			) : null}
			{entry.keys.length > 0 ? <h4>API keys</h4> : null}
			{entry.keys.map(key => (
				<div key={key.id} className="page-stack">
					<strong>
						{key.name} ·{' '}
						{key.revoked
							? 'Revoked'
							: key.expiresAt <= Date.now() / 1000
								? 'Expired'
								: `Expires ${new Date(key.expiresAt * 1000).toLocaleDateString()}`}
					</strong>
					<span>{key.models.join(', ')}</span>
					{!key.revoked ? (
						<button
							className="button small danger"
							type="button"
							disabled={busy}
							onClick={() => void change({ action: 'revokeKey', poolId: pool.id, id: key.id })}
						>
							Revoke {key.name}
						</button>
					) : null}
				</div>
			))}
		</section>
	);
}

function SubscriptionForm({
	initial,
	busy,
	onCancel,
	onSave
}: {
	initial?: Subscription;
	busy: boolean;
	onCancel: () => void;
	onSave: (body: Record<string, unknown>) => Promise<void>;
}) {
	const [name, setName] = useState(initial?.name ?? '');
	const [provider, setProvider] = useState<'claude' | 'codex'>(initial?.provider ?? 'claude');
	const [models, setModels] = useState(initial?.models.join('\n') ?? '');
	const [credential, setCredential] = useState('');
	const [parallel, setParallel] = useState(initial?.maxParallel ?? 1);
	return (
		<form
			className="page-stack"
			onSubmit={event => {
				event.preventDefault();
				void onSave({
					id: initial?.id,
					name,
					provider,
					models: [...new Set(models.split(/[\s,]+/).filter(Boolean))],
					credential: credential || undefined,
					enabled: initial?.enabled ?? true,
					maxParallel: provider === 'codex' ? 1 : parallel
				});
			}}
		>
			<h4>{initial ? 'Edit subscription' : 'Connect subscription'}</h4>
			<label>
				Subscription name
				<input required maxLength={100} value={name} onChange={e => setName(e.target.value)} />
			</label>
			<label>
				Subscription provider
				<select value={provider} onChange={e => setProvider(e.target.value as 'claude' | 'codex')}>
					<option value="claude">Claude</option>
					<option value="codex">ChatGPT / Codex</option>
				</select>
			</label>
			<label>
				Model IDs (one per line)
				<textarea required rows={3} value={models} onChange={e => setModels(e.target.value)} />
			</label>
			<p className="muted-copy">
				List the exact model IDs this subscription can use. Model access is checked by the provider.
			</p>
			<label>
				{provider === 'claude' ? 'Claude setup-token' : 'Codex auth.json'}
				<textarea
					required={!initial || initial.provider !== provider}
					rows={4}
					autoComplete="off"
					spellCheck={false}
					value={credential}
					onChange={e => setCredential(e.target.value)}
				/>
			</label>
			<p className="muted-copy">
				{provider === 'claude'
					? 'Run claude setup-token locally and paste the subscription token.'
					: 'Sign in locally with codex login and paste ~/.codex/auth.json. Refreshed login tokens are saved automatically.'}{' '}
				{initial ? 'Leave credentials blank to keep the existing login.' : ''}
			</p>
			{provider === 'claude' ? (
				<label>
					Parallel request limit
					<input
						required
						type="number"
						min={1}
						max={10}
						value={parallel}
						onChange={e => setParallel(e.target.valueAsNumber)}
					/>
				</label>
			) : null}
			<div className="button-row">
				<button className="button primary" type="submit" disabled={busy}>
					Save subscription
				</button>
				<button className="button" type="button" disabled={busy} onClick={onCancel}>
					Cancel
				</button>
			</div>
		</form>
	);
}
