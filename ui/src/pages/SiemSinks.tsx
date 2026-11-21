import { useQuery, useQueryClient } from '@tanstack/react-query';
import { Link } from '@tanstack/react-router';
import { useState } from 'react';

import { PageHeader, Panel, StatusBanner } from '@/components/Primitives';
import { magpieApi } from '@/magpie';

type AuditSink = {
	id: string;
	name: string;
	kind: 'sentinel' | 'otlp' | 'splunk' | 'syslog';
	enabled: boolean;
	capturePayloads: boolean;
	endpoint: string;
	dcrImmutableId: string;
	streamName: string;
	authMode: 'managed-identity' | 'client-credentials';
	tenantId: string;
	clientId: string;
	hasSecret: boolean;
	status: {
		position: number;
		lastOkAt: string;
		lastError: string;
		sentCount: number;
	};
};

type SinksData = {
	sinks: AuditSink[];
	envOtlpConfigured: boolean;
	canManage: boolean;
};

export function SiemSinksPage() {
	const queryClient = useQueryClient();
	const query = useQuery<SinksData>({
		queryKey: ['magpie-sinks'],
		queryFn: () => magpieApi<SinksData>('/api/magpie/sinks')
	});

	const [busy, setBusy] = useState('');
	const [error, setError] = useState('');
	const [notice, setNotice] = useState('');

	const [name, setName] = useState('Microsoft Sentinel (Australia East)');
	const [kind, setKind] = useState<AuditSink['kind']>('sentinel');
	const [enabled, setEnabled] = useState(true);
	const [endpoint, setEndpoint] = useState('');
	const [dcrImmutableId, setDcrImmutableId] = useState('');
	const [streamName, setStreamName] = useState('Custom-MagpieAudit_CL');
	const [authMode, setAuthMode] = useState<'managed-identity' | 'client-credentials'>(
		'managed-identity'
	);
	const [tenantId, setTenantId] = useState('');
	const [clientId, setClientId] = useState('');
	const [secret, setSecret] = useState('');

	async function handleSave(event: React.FormEvent) {
		event.preventDefault();
		setError('');
		setNotice('');
		setBusy('save');
		try {
			await magpieApi('/api/magpie/sinks', {
				id: '',
				name,
				kind,
				enabled,
				capturePayloads: false,
				endpoint,
				dcrImmutableId,
				streamName,
				authMode,
				tenantId,
				clientId,
				secret
			});
			setSecret('');
			setEndpoint('');
			setDcrImmutableId('');
			await queryClient.invalidateQueries({ queryKey: ['magpie-sinks'] });
			setNotice(`Saved SIEM sink "${name}". Delivery starts from new audit events.`);
		} catch (err) {
			setError(err instanceof Error ? err.message : 'Could not save SIEM sink.');
		} finally {
			setBusy('');
		}
	}

	async function handleTest(id: string) {
		setError('');
		setNotice('');
		setBusy(`test:${id}`);
		try {
			const res = await magpieApi<{ ok: boolean; message?: string; error?: string }>(
				`/api/magpie/sinks/${encodeURIComponent(id)}/test`,
				{}
			);
			if (res.ok) setNotice(res.message || 'Test event delivered.');
			else setError(res.error || 'Test delivery failed.');
		} catch (err) {
			setError(err instanceof Error ? err.message : 'Could not test sink.');
		} finally {
			setBusy('');
		}
	}

	async function handleDelete(id: string, sinkName: string) {
		setError('');
		setNotice('');
		setBusy(`del:${id}`);
		try {
			await magpieApi(`/api/magpie/sinks/${encodeURIComponent(id)}/delete`, {});
			await queryClient.invalidateQueries({ queryKey: ['magpie-sinks'] });
			setNotice(`Removed SIEM sink "${sinkName}".`);
		} catch (err) {
			setError(err instanceof Error ? err.message : 'Could not remove sink.');
		} finally {
			setBusy('');
		}
	}

	if (query.isLoading) {
		return (
			<div className="page-stack control-centre-page">
				<PageHeader title="SIEM & Microsoft Sentinel" />
				<StatusBanner state="loading" title="Loading SIEM sinks" />
			</div>
		);
	}

	if (query.isError || !query.data) {
		return (
			<div className="page-stack control-centre-page">
				<PageHeader title="SIEM & Microsoft Sentinel" />
				<StatusBanner state="bad" title="Could not load SIEM configuration">
					{query.error instanceof Error ? query.error.message : 'Access denied or unavailable.'}
				</StatusBanner>
			</div>
		);
	}

	const data = query.data;

	return (
		<div className="page-stack control-centre-page">
			<PageHeader
				title="SIEM & Microsoft Sentinel"
				description="Stream immutable Magpie audit events directly into Microsoft Sentinel (Logs Ingestion API), OpenTelemetry, Splunk HEC, or Syslog to support ISM-2159 and ISM-1988 central logging."
				actions={
					<div className="control-centre-header-actions">
						<a className="button primary" href="/api/magpie/sinks/sentinel-pack">
							Download Sentinel DCR & KQL pack
						</a>
						<Link className="button" to="/governance/$page" params={{ page: 'alerts' }}>
							Teams / Slack / Email alerts
						</Link>
					</div>
				}
			/>

			{error ? (
				<StatusBanner state="bad" title="SIEM action failed">
					{error}
				</StatusBanner>
			) : null}
			{notice ? (
				<StatusBanner state="ok" title="Success">
					{notice}
				</StatusBanner>
			) : null}

			{data.envOtlpConfigured ? (
				<StatusBanner state="info" title="Environment OTLP exporter is also active">
					OTEL_EXPORTER_OTLP_* environment variables are configured on this deployment and run in
					addition to any sinks configured below.
				</StatusBanner>
			) : null}

			<Panel>
				<div className="cc-panel-head">
					<h2>Configured SIEM sinks ({data.sinks.length})</h2>
					<p className="muted-copy">
						Each sink tracks its own export cursor, retries transient HTTP 429/5xx failures
						automatically, and keeps secrets write-only.
					</p>
				</div>

				{data.sinks.length === 0 ? (
					<p className="muted-copy">
						No UI-configured SIEM sinks yet. Add Microsoft Sentinel, OTLP, Splunk HEC, or Syslog
						below.
					</p>
				) : (
					// biome-ignore lint/a11y/noNoninteractiveTabindex: Scrollable table region requires keyboard access (WCAG 2.1.1).
					<section className="table-wrap" tabIndex={0} aria-label="Configured SIEM sinks">
						<table className="cc-table">
							<thead>
								<tr>
									<th scope="col">Sink</th>
									<th scope="col">Destination</th>
									<th scope="col">Status</th>
									<th scope="col">Delivered</th>
									<th scope="col">Actions</th>
								</tr>
							</thead>
							<tbody>
								{data.sinks.map(sink => (
									<tr key={sink.id}>
										<th scope="row">
											<div>{sink.name}</div>
											<span className="cc-cell-sub">
												{sink.kind.toUpperCase()}
												{sink.kind === 'sentinel' ? ` · ${sink.authMode}` : ''}
											</span>
										</th>
										<td>
											<code>{sink.endpoint}</code>
											{sink.dcrImmutableId ? (
												<span className="cc-cell-sub">
													DCR: {sink.dcrImmutableId} ({sink.streamName})
												</span>
											) : null}
										</td>
										<td>
											<span
												className={`cc-pill ${sink.enabled ? (sink.status.lastError ? 'cc-pill-warn' : 'cc-pill-ok') : 'cc-pill-neutral'}`}
											>
												{!sink.enabled ? 'Paused' : sink.status.lastError ? 'Retrying' : 'Active'}
											</span>
											{sink.status.lastError ? (
												<span className="cc-cell-sub">{sink.status.lastError}</span>
											) : null}
										</td>
										<td>
											<div>{sink.status.sentCount} events</div>
											{sink.status.lastOkAt ? (
												<span className="cc-cell-sub">
													Last ok: {new Date(sink.status.lastOkAt).toLocaleString()}
												</span>
											) : null}
										</td>
										<td>
											<div className="cc-inline-actions">
												<button
													type="button"
													className="button"
													disabled={!data.canManage || busy !== ''}
													onClick={() => void handleTest(sink.id)}
												>
													{busy === `test:${sink.id}` ? 'Testing…' : 'Send test'}
												</button>
												<button
													type="button"
													className="button"
													disabled={!data.canManage || busy !== ''}
													onClick={() => void handleDelete(sink.id, sink.name)}
												>
													Remove
												</button>
											</div>
										</td>
									</tr>
								))}
							</tbody>
						</table>
					</section>
				)}
			</Panel>

			{data.canManage ? (
				<Panel>
					<div className="cc-panel-head">
						<h2>Add SIEM sink</h2>
						<p className="muted-copy">
							Raw prompt and response text is redacted by default so central SIEM logs never store
							sensitive user input.
						</p>
					</div>
					<form className="cc-form" onSubmit={e => void handleSave(e)}>
						<div className="cc-form-grid">
							<div className="cc-field">
								<label htmlFor="sink-name">Sink name</label>
								<input
									id="sink-name"
									required
									value={name}
									onChange={e => setName(e.target.value)}
								/>
							</div>
							<div className="cc-field">
								<label htmlFor="sink-kind">Destination type</label>
								<select
									id="sink-kind"
									value={kind}
									onChange={e => setKind(e.target.value as AuditSink['kind'])}
								>
									<option value="sentinel">Microsoft Sentinel (Logs Ingestion API)</option>
									<option value="otlp">OpenTelemetry (OTLP/HTTP JSON)</option>
									<option value="splunk">Splunk HEC</option>
									<option value="syslog">Syslog over HTTPS</option>
								</select>
							</div>
							<div className="cc-field">
								<label htmlFor="sink-endpoint">
									{kind === 'sentinel' ? 'Data Collection Endpoint (DCE) URL' : 'Endpoint URL'}
								</label>
								<input
									id="sink-endpoint"
									type="url"
									required
									placeholder={
										kind === 'sentinel'
											? 'https://dce-magpie-au.australiaeast-1.ingest.monitor.azure.com'
											: 'https://collector.example.internal/v1/logs'
									}
									value={endpoint}
									onChange={e => setEndpoint(e.target.value)}
								/>
							</div>
						</div>

						{kind === 'sentinel' ? (
							<div className="cc-form-grid">
								<div className="cc-field">
									<label htmlFor="sink-dcr">DCR immutable ID</label>
									<input
										id="sink-dcr"
										required
										placeholder="dcr-0123456789abcdef0123456789abcdef"
										value={dcrImmutableId}
										onChange={e => setDcrImmutableId(e.target.value)}
									/>
								</div>
								<div className="cc-field">
									<label htmlFor="sink-stream">Stream name</label>
									<input
										id="sink-stream"
										value={streamName}
										onChange={e => setStreamName(e.target.value)}
									/>
								</div>
								<div className="cc-field">
									<label htmlFor="sink-auth">Azure authentication</label>
									<select
										id="sink-auth"
										value={authMode}
										onChange={e =>
											setAuthMode(e.target.value as 'managed-identity' | 'client-credentials')
										}
									>
										<option value="managed-identity">
											Azure Managed Identity (recommended on Azure)
										</option>
										<option value="client-credentials">
											Entra App Registration (client credentials)
										</option>
									</select>
								</div>
								{authMode === 'client-credentials' ? (
									<>
										<div className="cc-field">
											<label htmlFor="sink-tenant">Entra tenant ID</label>
											<input
												id="sink-tenant"
												required
												value={tenantId}
												onChange={e => setTenantId(e.target.value)}
											/>
										</div>
										<div className="cc-field">
											<label htmlFor="sink-client">Entra application (client) ID</label>
											<input
												id="sink-client"
												required
												value={clientId}
												onChange={e => setClientId(e.target.value)}
											/>
										</div>
										<div className="cc-field">
											<label htmlFor="sink-secret">Client secret (write-only)</label>
											<input
												id="sink-secret"
												type="password"
												required
												value={secret}
												onChange={e => setSecret(e.target.value)}
											/>
										</div>
									</>
								) : (
									<div className="cc-field">
										<label htmlFor="sink-mi-client">
											User-assigned Managed Identity client ID (optional)
										</label>
										<input
											id="sink-mi-client"
											placeholder="Leave blank for system-assigned identity"
											value={clientId}
											onChange={e => setClientId(e.target.value)}
										/>
									</div>
								)}
							</div>
						) : (
							<div className="cc-field">
								<label htmlFor="sink-token">Bearer / HEC token (optional, write-only)</label>
								<input
									id="sink-token"
									type="password"
									value={secret}
									onChange={e => setSecret(e.target.value)}
								/>
							</div>
						)}

						<label className="magpie-check" htmlFor="sink-enabled">
							<input
								id="sink-enabled"
								type="checkbox"
								checked={enabled}
								onChange={e => setEnabled(e.target.checked)}
							/>
							<span>Enable live export immediately after saving</span>
						</label>

						<button type="submit" className="button primary" disabled={busy !== ''}>
							{busy === 'save' ? 'Saving…' : 'Save SIEM sink'}
						</button>
					</form>
				</Panel>
			) : null}
		</div>
	);
}
