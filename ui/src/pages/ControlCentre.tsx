import { useQuery, useQueryClient } from '@tanstack/react-query';
import { Link } from '@tanstack/react-router';
import { useEffect, useState } from 'react';

import { PageHeader, Panel, StatusBanner } from '@/components/Primitives';
import { magpieApi } from '@/magpie';

type PresetChange = {
	area: string;
	label: string;
	current: string;
	target: string;
	requiresApproval: boolean;
};

type PosturePreset = {
	id: string;
	name: string;
	audience: string;
	summary: string;
	honestNote: string;
	targetMode: 'log-only' | 'enforce';
	targetSovereignMode: boolean;
	matchesCurrent: boolean;
	changes: PresetChange[];
	canApply: boolean;
	missingPermissions: string[];
};

type Cohort = {
	id: string;
	name: string;
	roles: string[];
	memberCount: number;
	memberNames: string[];
	identityGroups: string[];
	trafficNames: string[];
	enforcement: 'follow' | 'log-only' | 'enforce';
};

type ControlCentreData = {
	mode: 'log-only' | 'enforce';
	sovereignMode: boolean;
	telemetryEnabled: boolean;
	presidioAnalyzerUrl: string;
	retentionDays: number;
	requireApproval: boolean;
	approvedProtected: string[];
	killSwitch: {
		global: boolean;
		agents: string[];
	};
	oidcEnabled: boolean;
	breakGlassEnabled: boolean;
	pendingRevisionsCount: number;
	pendingApprovalsCount: number;
	dlpSummary: {
		enabled: number;
		total: number;
		blocking: number;
		logging: number;
	};
	readiness: {
		recentDecisions: number;
		wouldActCount: number;
		message: string;
	};
	cohorts: Cohort[];
	presets: PosturePreset[];
	permissions: {
		canChangeEnforcement: boolean;
		canManageSettings: boolean;
		canEditDlp: boolean;
		canApprove: boolean;
		canSubmitApproval: boolean;
		canManageAccess: boolean;
		canReadAudit: boolean;
		canReadPolicies: boolean;
		canReadSettings: boolean;
	};
};

type PresetResult = {
	presetId: string;
	presetName: string;
	appliedImmediate: string[];
	pendingApproval: string[];
	skippedPending: string[];
};

function enforcementLabel(value: Cohort['enforcement']): string {
	switch (value) {
		case 'enforce':
			return 'Enforcing';
		case 'log-only':
			return 'Trial (log-only)';
		default:
			return 'Follow organisation';
	}
}

export function ControlCentrePage() {
	const queryClient = useQueryClient();
	const query = useQuery<ControlCentreData>({
		queryKey: ['magpie-control-centre'],
		queryFn: () => magpieApi<ControlCentreData>('/api/magpie/control-centre')
	});

	const [selectedPresetId, setSelectedPresetId] = useState<string>('');
	const [busy, setBusy] = useState<string>('');
	const [error, setError] = useState('');
	const [notice, setNotice] = useState('');

	const [sovereignMode, setSovereignMode] = useState(true);
	const [telemetryEnabled, setTelemetryEnabled] = useState(false);
	const [retentionDays, setRetentionDays] = useState('365');
	const [presidioUrl, setPresidioUrl] = useState('');

	const [killGlobalConfirm, setKillGlobalConfirm] = useState(false);
	const [killAgentsText, setKillAgentsText] = useState('');

	const [cohortDrafts, setCohortDrafts] = useState<
		Record<string, { enforcement: Cohort['enforcement']; trafficNames: string }>
	>({});

	const data = query.data;

	useEffect(() => {
		if (!data) return;
		setSovereignMode(data.sovereignMode);
		setTelemetryEnabled(data.telemetryEnabled);
		setRetentionDays(String(data.retentionDays));
		setPresidioUrl(data.presidioAnalyzerUrl);
		setKillAgentsText(data.killSwitch.agents.join('\n'));
		const nextDrafts: Record<string, { enforcement: Cohort['enforcement']; trafficNames: string }> =
			{};
		for (const cohort of data.cohorts) {
			nextDrafts[cohort.id] = {
				enforcement: cohort.enforcement,
				trafficNames: cohort.trafficNames.join('\n')
			};
		}
		setCohortDrafts(nextDrafts);
	}, [data]);

	async function refreshAll() {
		await queryClient.invalidateQueries({ queryKey: ['magpie-control-centre'] });
		await queryClient.invalidateQueries({ queryKey: ['magpie-session'] });
		await queryClient.invalidateQueries({ queryKey: ['magpie-approvals'] });
		window.dispatchEvent(new Event('magpie-settings-changed'));
	}

	async function handleModeChange(nextMode: 'log-only' | 'enforce') {
		setError('');
		setNotice('');
		setBusy('mode');
		try {
			await magpieApi('/api/magpie/control-centre/mode', { mode: nextMode });
			await refreshAll();
			setNotice(
				nextMode === 'enforce'
					? 'Organisation default is now Enforcing. Groups explicitly set to Trial remain log-only.'
					: 'Organisation default is now Log-only trial. Groups explicitly set to Enforcing stay enforced.'
			);
		} catch (err) {
			setError(err instanceof Error ? err.message : 'Could not change enforcement mode.');
		} finally {
			setBusy('');
		}
	}

	async function handleApplyPreset(preset: PosturePreset) {
		setError('');
		setNotice('');
		setBusy(`preset:${preset.id}`);
		try {
			const res = await magpieApi<PresetResult>('/api/magpie/control-centre/preset', {
				presetId: preset.id
			});
			await refreshAll();
			const parts: string[] = [];
			if (res.appliedImmediate.length) {
				parts.push(`Applied immediately: ${res.appliedImmediate.length}.`);
			}
			if (res.pendingApproval.length) {
				parts.push(
					`Staged for second-person approval: ${res.pendingApproval.length}. Open Approvals to review.`
				);
			}
			if (res.skippedPending.length) {
				parts.push(`Skipped (already pending): ${res.skippedPending.length}.`);
			}
			setNotice(`${res.presetName}: ${parts.join(' ')}`);
			setSelectedPresetId('');
		} catch (err) {
			setError(err instanceof Error ? err.message : 'Could not apply preset.');
		} finally {
			setBusy('');
		}
	}

	async function handleCohortSave(cohort: Cohort) {
		const draft = cohortDrafts[cohort.id];
		if (!draft) return;
		setError('');
		setNotice('');
		setBusy(`cohort:${cohort.id}`);
		try {
			const trafficNames = draft.trafficNames
				.split('\n')
				.map(v => v.trim())
				.filter(Boolean);
			await magpieApi(`/api/magpie/control-centre/cohorts/${encodeURIComponent(cohort.id)}`, {
				enforcement: draft.enforcement,
				trafficNames
			});
			await refreshAll();
			setNotice(`Saved enforcement for ${cohort.name} (${enforcementLabel(draft.enforcement)}).`);
		} catch (err) {
			setError(err instanceof Error ? err.message : 'Could not update group enforcement.');
		} finally {
			setBusy('');
		}
	}

	async function handleSettingsSubmit(event: React.FormEvent) {
		event.preventDefault();
		setError('');
		setNotice('');
		setBusy('settings');
		try {
			const parsedRetention = Number.parseInt(retentionDays, 10);
			if (!Number.isFinite(parsedRetention) || parsedRetention < 1 || parsedRetention > 3650) {
				throw new Error('Audit retention target must be between 1 and 3650 days.');
			}
			const res = await magpieApi<{ pending: boolean }>('/api/magpie/control-centre/settings', {
				sovereignMode,
				telemetryEnabled,
				retentionDays: parsedRetention,
				presidioAnalyzerUrl: presidioUrl
			});
			await refreshAll();
			setNotice(
				res.pending
					? 'Governance settings saved for second-person approval. Active settings have not changed yet.'
					: 'Governance settings updated.'
			);
		} catch (err) {
			setError(err instanceof Error ? err.message : 'Could not save governance settings.');
		} finally {
			setBusy('');
		}
	}

	async function handleKillSwitchSave(nextGlobal: boolean) {
		setError('');
		setNotice('');
		setBusy('kill');
		try {
			const agents = killAgentsText
				.split(/[\n,]/)
				.map(v => v.trim())
				.filter(Boolean);
			await magpieApi('/api/magpie/control-centre/kill-switch', {
				global: nextGlobal,
				agents
			});
			setKillGlobalConfirm(false);
			await refreshAll();
			setNotice(
				nextGlobal
					? 'Global kill switch is ACTIVE. Enforced traffic is blocked immediately.'
					: agents.length
						? `Global kill switch is off. Per-agent kill switch active for ${agents.length} agent(s).`
						: 'All kill switches are cleared.'
			);
		} catch (err) {
			setError(err instanceof Error ? err.message : 'Could not update kill switch.');
		} finally {
			setBusy('');
		}
	}

	if (query.isLoading) {
		return (
			<div className="page-stack control-centre-page">
				<PageHeader
					title="Control Centre"
					description="Manage security posture, staged cohort enforcement, governance settings, and emergency kill switches from one place."
				/>
				<StatusBanner state="loading" title="Loading Control Centre" />
			</div>
		);
	}

	if (query.isError || !data) {
		return (
			<div className="page-stack control-centre-page">
				<PageHeader title="Control Centre" />
				<StatusBanner state="bad" title="Could not load Control Centre">
					{query.error instanceof Error ? query.error.message : 'Access denied or unavailable.'}
				</StatusBanner>
			</div>
		);
	}

	const totalPending = data.pendingRevisionsCount + data.pendingApprovalsCount;
	const selectedPreset = data.presets.find(p => p.id === selectedPresetId) ?? null;

	return (
		<div className="page-stack control-centre-page">
			<PageHeader
				title="Control Centre"
				description="Control enforcement posture, Microsoft & government readiness presets, group rollout cohorts, and emergency stops."
				actions={
					<div className="control-centre-header-actions">
						{data.permissions.canReadPolicies || data.permissions.canApprove ? (
							<Link className="button" to="/approvals">
								Approvals inbox{totalPending > 0 ? ` (${totalPending})` : ''}
							</Link>
						) : null}
						{data.permissions.canReadPolicies ? (
							<Link className="button" to="/dlp-tuning">
								Tune DLP
							</Link>
						) : null}
						{data.permissions.canReadAudit ? (
							<Link className="button" to="/governance/$page" params={{ page: 'audit' }}>
								Decision audit
							</Link>
						) : null}
					</div>
				}
			/>

			{error ? (
				<StatusBanner state="bad" title="Action could not be completed">
					{error}
				</StatusBanner>
			) : null}
			{notice ? (
				<StatusBanner state="ok" title="Updated">
					{notice}
				</StatusBanner>
			) : null}

			{data.killSwitch.global ? (
				<StatusBanner state="bad" title="Global kill switch is active">
					All enforced AI requests and tool calls are being blocked immediately. Cohorts explicitly
					set to Trial continue to log only.
				</StatusBanner>
			) : null}

			<section className="cc-kpi-grid" aria-label="Operational summary">
				<div className="cc-kpi-card">
					<span className="cc-kpi-label">Organisation default</span>
					<strong className="cc-kpi-value">
						{data.mode === 'enforce' ? 'Enforcing' : 'Log-only trial'}
					</strong>
					<span className="cc-kpi-sub">
						{data.mode === 'enforce'
							? 'Policy actions block or gate traffic'
							: 'Records would-be actions without blocking'}
					</span>
				</div>
				<div className="cc-kpi-card">
					<span className="cc-kpi-label">Australian residency</span>
					<strong className="cc-kpi-value">
						{data.sovereignMode ? 'Sovereign mode on' : 'Flexible routing'}
					</strong>
					<span className="cc-kpi-sub">
						{data.sovereignMode
							? 'Destinations restricted to AU regions'
							: 'PSPF marking rules still restrict sensitive traffic'}
					</span>
				</div>
				<div className="cc-kpi-card">
					<span className="cc-kpi-label">Australian DLP detectors</span>
					<strong className="cc-kpi-value">
						{data.dlpSummary.enabled} / {data.dlpSummary.total} enabled
					</strong>
					<span className="cc-kpi-sub">
						{data.dlpSummary.blocking} active block/redact · {data.dlpSummary.logging} log-only
					</span>
				</div>
				<div className="cc-kpi-card">
					<span className="cc-kpi-label">Pending approvals</span>
					<strong className="cc-kpi-value">{totalPending}</strong>
					<span className="cc-kpi-sub">
						{data.pendingRevisionsCount} policy · {data.pendingApprovalsCount} model/tool
					</span>
				</div>
			</section>

			<Panel>
				<div className="cc-panel-head">
					<h2>Governance posture presets</h2>
					<p className="muted-copy">
						Apply a coherent baseline for commercial Microsoft enterprises or Australian Government
						agencies. Review exact changes before confirming; policy and DLP edits follow your
						second-person approval rule.
					</p>
				</div>
				<ul className="cc-preset-grid">
					{data.presets.map(preset => {
						const active = selectedPresetId === preset.id;
						return (
							<li
								key={preset.id}
								className={`cc-preset-card${preset.matchesCurrent ? ' is-matched' : ''}${active ? ' is-selected' : ''}`}
							>
								<div className="cc-preset-top">
									<div>
										<span className="cc-preset-audience">{preset.audience}</span>
										<h3>{preset.name}</h3>
									</div>
									<span
										className={`cc-pill ${preset.matchesCurrent ? 'cc-pill-ok' : 'cc-pill-neutral'}`}
									>
										{preset.matchesCurrent
											? 'Active'
											: `${preset.changes.length} change${preset.changes.length === 1 ? '' : 's'}`}
									</span>
								</div>
								<p className="cc-preset-summary">{preset.summary}</p>
								<p className="cc-preset-note">{preset.honestNote}</p>
								<div className="cc-preset-footer">
									<button
										type="button"
										className={`button${active ? ' primary' : ''}`}
										onClick={() => setSelectedPresetId(active ? '' : preset.id)}
										aria-expanded={active}
									>
										{preset.matchesCurrent
											? 'Matches current'
											: active
												? 'Hide preview'
												: 'Review changes'}
									</button>
								</div>
							</li>
						);
					})}
				</ul>

				{selectedPreset ? (
					<section className="cc-preset-preview" aria-label={`Preview ${selectedPreset.name}`}>
						<h3>Changes for {selectedPreset.name}</h3>
						{selectedPreset.matchesCurrent ? (
							<p>Your active gateway configuration already matches this preset.</p>
						) : (
							<>
								{/* biome-ignore lint/a11y/noNoninteractiveTabindex: Scrollable table region requires keyboard access (WCAG 2.1.1). */}
								<section className="table-wrap" tabIndex={0} aria-label="Preset changes">
									<table className="cc-table">
										<thead>
											<tr>
												<th scope="col">Control</th>
												<th scope="col">Current</th>
												<th scope="col">Proposed</th>
												<th scope="col">Activation</th>
											</tr>
										</thead>
										<tbody>
											{selectedPreset.changes.map(change => (
												<tr key={`${change.area}:${change.label}`}>
													<th scope="row">{change.label}</th>
													<td>{change.current}</td>
													<td>
														<strong>{change.target}</strong>
													</td>
													<td>
														{change.requiresApproval
															? 'Pending second-person approval'
															: 'Immediate'}
													</td>
												</tr>
											))}
										</tbody>
									</table>
								</section>
								{selectedPreset.missingPermissions.length > 0 ? (
									<StatusBanner state="warn" title="Additional permissions needed">
										Ask an administrator to grant: {selectedPreset.missingPermissions.join(', ')}.
									</StatusBanner>
								) : (
									<div className="cc-preview-actions">
										<button
											type="button"
											className="button primary"
											disabled={!selectedPreset.canApply || busy !== ''}
											onClick={() => void handleApplyPreset(selectedPreset)}
										>
											{busy === `preset:${selectedPreset.id}`
												? 'Applying…'
												: data.requireApproval
													? `Apply / stage ${selectedPreset.name}`
													: `Apply ${selectedPreset.name}`}
										</button>
										<button
											type="button"
											className="button"
											onClick={() => setSelectedPresetId('')}
										>
											Cancel
										</button>
									</div>
								)}
							</>
						)}
					</section>
				) : null}
			</Panel>

			<Panel>
				<div className="cc-panel-head">
					<h2>Enforcement & staged cohort rollout</h2>
					<p className="muted-copy">
						Switch the organisation fallback or roll out enforcement one Microsoft Entra / user
						group at a time without affecting everyone else.
					</p>
				</div>
				<div className="cc-mode-row">
					<div>
						<h3>Organisation fallback mode</h3>
						<p className="muted-copy">
							Applies to users not in a group with an explicit override, and to requests without a
							matched identity. Takes effect on the next request and is audited atomically.
						</p>
						{data.readiness.message ? (
							<p className="cc-readiness-note">{data.readiness.message}</p>
						) : null}
					</div>
					<fieldset className="cc-mode-actions cc-fieldset-bare">
						<legend className="sr-only">Organisation enforcement mode</legend>
						<button
							type="button"
							className={`button${data.mode === 'log-only' ? ' primary' : ''}`}
							aria-pressed={data.mode === 'log-only'}
							disabled={
								!data.permissions.canChangeEnforcement || busy !== '' || data.mode === 'log-only'
							}
							onClick={() => void handleModeChange('log-only')}
						>
							Log-only trial
						</button>
						<button
							type="button"
							className={`button${data.mode === 'enforce' ? ' primary' : ''}`}
							aria-pressed={data.mode === 'enforce'}
							disabled={
								!data.permissions.canChangeEnforcement || busy !== '' || data.mode === 'enforce'
							}
							onClick={() => void handleModeChange('enforce')}
						>
							Enforcing
						</button>
					</fieldset>
				</div>

				{data.cohorts.length > 0 ? (
					<div className="cc-cohorts-block">
						<div className="cc-cohorts-header">
							<h3>Group rollout cohorts</h3>
							{data.permissions.canManageAccess ? (
								<Link to="/governance/$page" params={{ page: 'access' }}>
									Edit group membership & Entra ID mappings
								</Link>
							) : null}
						</div>
						{/* biome-ignore lint/a11y/noNoninteractiveTabindex: Scrollable table region requires keyboard access (WCAG 2.1.1). */}
						<section className="table-wrap" tabIndex={0} aria-label="Group rollout cohorts">
							<table className="cc-table">
								<thead>
									<tr>
										<th scope="col">Group</th>
										<th scope="col">Entra / IdP & members</th>
										<th scope="col">Gateway key names</th>
										<th scope="col">Cohort mode</th>
										<th scope="col">Action</th>
									</tr>
								</thead>
								<tbody>
									{data.cohorts.map(cohort => {
										const draft = cohortDrafts[cohort.id] ?? {
											enforcement: cohort.enforcement,
											trafficNames: cohort.trafficNames.join('\n')
										};
										const dirty =
											draft.enforcement !== cohort.enforcement ||
											draft.trafficNames.trim() !== cohort.trafficNames.join('\n').trim();
										return (
											<tr key={cohort.id}>
												<th scope="row">
													<div>{cohort.name}</div>
													<span className="cc-cell-sub">
														{cohort.roles.join(', ') || 'No roles'}
													</span>
												</th>
												<td>
													<div>{cohort.memberCount} portal user(s)</div>
													{cohort.identityGroups.length > 0 ? (
														<span className="cc-cell-sub">
															IdP: {cohort.identityGroups.join(', ')}
														</span>
													) : null}
												</td>
												<td>
													<label className="sr-only" htmlFor={`cohort-keys-${cohort.id}`}>
														Gateway key names for {cohort.name}
													</label>
													<textarea
														id={`cohort-keys-${cohort.id}`}
														className="cc-cohort-keys"
														rows={2}
														placeholder="One virtual key name per line"
														disabled={!data.permissions.canChangeEnforcement}
														value={draft.trafficNames}
														onChange={e =>
															setCohortDrafts(prev => ({
																...prev,
																[cohort.id]: { ...draft, trafficNames: e.target.value }
															}))
														}
													/>
												</td>
												<td>
													<label className="sr-only" htmlFor={`cohort-mode-${cohort.id}`}>
														Enforcement mode for {cohort.name}
													</label>
													<select
														id={`cohort-mode-${cohort.id}`}
														disabled={!data.permissions.canChangeEnforcement}
														value={draft.enforcement}
														onChange={e =>
															setCohortDrafts(prev => ({
																...prev,
																[cohort.id]: {
																	...draft,
																	enforcement: e.target.value as Cohort['enforcement']
																}
															}))
														}
													>
														<option value="follow">Follow organisation</option>
														<option value="log-only">Trial (log-only)</option>
														<option value="enforce">Enforcing</option>
													</select>
												</td>
												<td>
													<button
														type="button"
														className={`button${dirty ? ' primary' : ''}`}
														disabled={
															!data.permissions.canChangeEnforcement || !dirty || busy !== ''
														}
														onClick={() => void handleCohortSave(cohort)}
													>
														{busy === `cohort:${cohort.id}` ? 'Saving…' : 'Save'}
													</button>
												</td>
											</tr>
										);
									})}
								</tbody>
							</table>
						</section>
					</div>
				) : null}
			</Panel>

			<div className="cc-two-col">
				<Panel>
					<div className="cc-panel-head">
						<h2>Governance & residency settings</h2>
						<p className="muted-copy">
							{data.requireApproval
								? 'Second-person approval is enabled: changes saved here enter the Approvals inbox before taking effect.'
								: 'Changes take effect immediately and are recorded in Decision Audit.'}
						</p>
					</div>
					<form className="cc-form" onSubmit={e => void handleSettingsSubmit(e)}>
						<label className="magpie-check" htmlFor="cc-sovereign">
							<input
								id="cc-sovereign"
								type="checkbox"
								checked={sovereignMode}
								disabled={!data.permissions.canManageSettings}
								onChange={e => setSovereignMode(e.target.checked)}
							/>
							<span>
								<strong>Australian Sovereign Mode</strong> — restrict routes to Australian regions
								(APP 8)
							</span>
						</label>

						<label className="magpie-check" htmlFor="cc-telemetry">
							<input
								id="cc-telemetry"
								type="checkbox"
								checked={telemetryEnabled}
								disabled={!data.permissions.canManageSettings}
								onChange={e => setTelemetryEnabled(e.target.checked)}
							/>
							<span>
								<strong>Telemetry & phone-home</strong> — keep unchecked for zero external
								phone-home
							</span>
						</label>

						<div className="cc-field">
							<label htmlFor="cc-retention">Audit retention target (days)</label>
							<input
								id="cc-retention"
								type="number"
								min={1}
								max={3650}
								disabled={!data.permissions.canManageSettings}
								value={retentionDays}
								onChange={e => setRetentionDays(e.target.value)}
							/>
							<span className="cc-cell-sub">
								ISM-1988 recommends at least 365 days searchable retention in your central store.
							</span>
						</div>

						<details className="cc-disclosure">
							<summary>Advanced analyser & deployment flags</summary>
							<div className="cc-field">
								<label htmlFor="cc-presidio">Optional Presidio analyser URL</label>
								<input
									id="cc-presidio"
									type="url"
									placeholder="Leave blank to keep detection local"
									disabled={!data.permissions.canManageSettings}
									value={presidioUrl}
									onChange={e => setPresidioUrl(e.target.value)}
								/>
							</div>
							<dl className="cc-meta-list">
								<div>
									<dt>Identity provider (OIDC / Entra)</dt>
									<dd>{data.oidcEnabled ? 'Configured' : 'Local accounts only'}</dd>
								</div>
								<div>
									<dt>Second-person approval</dt>
									<dd>{data.requireApproval ? 'Required' : 'Not required'}</dd>
								</div>
								<div>
									<dt>Break-glass recovery</dt>
									<dd>{data.breakGlassEnabled ? 'Enabled & audited' : 'Disabled'}</dd>
								</div>
								<div>
									<dt>Approved PROTECTED destinations</dt>
									<dd>
										{data.approvedProtected.length > 0
											? data.approvedProtected.join(', ')
											: 'None configured'}
									</dd>
								</div>
							</dl>
						</details>

						<button
							type="submit"
							className="button primary"
							disabled={!data.permissions.canManageSettings || busy !== ''}
						>
							{busy === 'settings'
								? 'Saving…'
								: data.requireApproval
									? 'Save for approval'
									: 'Save settings'}
						</button>
					</form>
				</Panel>

				<Panel>
					<div className="cc-panel-head">
						<h2>Emergency kill switches</h2>
						<p className="muted-copy">
							Kill switches take effect immediately without waiting for second-person approval. They
							block enforced traffic and log-only for cohorts still on trial.
						</p>
					</div>
					<div className={`cc-kill-box${data.killSwitch.global ? ' is-active' : ''}`}>
						<div className="cc-kill-status">
							<span>Global kill switch</span>
							<strong className={data.killSwitch.global ? 'cc-text-bad' : 'cc-text-ok'}>
								{data.killSwitch.global ? 'ACTIVE — blocking traffic' : 'Off'}
							</strong>
						</div>

						{data.killSwitch.global ? (
							<button
								type="button"
								className="button primary"
								disabled={!data.permissions.canManageSettings || busy !== ''}
								onClick={() => void handleKillSwitchSave(false)}
							>
								{busy === 'kill' ? 'Updating…' : 'Clear global kill switch'}
							</button>
						) : (
							<div className="cc-kill-arm">
								<label className="magpie-check" htmlFor="cc-kill-confirm">
									<input
										id="cc-kill-confirm"
										type="checkbox"
										checked={killGlobalConfirm}
										disabled={!data.permissions.canManageSettings}
										onChange={e => setKillGlobalConfirm(e.target.checked)}
									/>
									<span>Confirm I want to block all enforced AI traffic immediately</span>
								</label>
								<button
									type="button"
									className="button danger"
									disabled={
										!data.permissions.canManageSettings || !killGlobalConfirm || busy !== ''
									}
									onClick={() => void handleKillSwitchSave(true)}
								>
									{busy === 'kill' ? 'Activating…' : 'Activate global kill switch'}
								</button>
							</div>
						)}
					</div>

					<div className="cc-field">
						<label htmlFor="cc-kill-agents">Per-agent kill switch (one agent ID per line)</label>
						<textarea
							id="cc-kill-agents"
							rows={3}
							placeholder="Leave empty for none"
							disabled={!data.permissions.canManageSettings}
							value={killAgentsText}
							onChange={e => setKillAgentsText(e.target.value)}
						/>
						<button
							type="button"
							className="button"
							disabled={!data.permissions.canManageSettings || busy !== ''}
							onClick={() => void handleKillSwitchSave(data.killSwitch.global)}
						>
							{busy === 'kill' ? 'Saving…' : 'Save per-agent list'}
						</button>
					</div>
				</Panel>
			</div>
		</div>
	);
}
