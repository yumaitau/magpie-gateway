import { useQuery } from '@tanstack/react-query';
import { useState } from 'react';

import { PageHeader, Panel, StatusBanner } from '@/components/Primitives';
import { magpieApi } from '@/magpie';

type EvidenceRow = {
	framework: string;
	reference: string;
	source: string;
	magpieControl: string;
	observation: string;
	relationship: string;
};

type EvidenceData = {
	periodLabel: string;
	from: string;
	to: string;
	generatedAt: string;
	stats: {
		requests: number;
		blocks: number;
		dlpHits: number;
		policyChanges: number;
		logins: number;
		approvals: number;
		sovereignDenies: number;
	};
	posture: {
		mode: string;
		sovereignMode: boolean;
		requireApproval: boolean;
		retentionDays: number;
		oidcEnabled: boolean;
		enabledDetectors: number;
		totalDetectors: number;
	};
	rows: EvidenceRow[];
	canExport: boolean;
	canExportPolicy: boolean;
};

function isoDateOffset(daysAgo: number): string {
	const d = new Date(Date.now() - daysAgo * 86_400_000);
	return d.toISOString().slice(0, 10);
}

export function EvidencePackPage() {
	const [from, setFrom] = useState('');
	const [to, setTo] = useState('');

	const params = new URLSearchParams();
	if (from) params.set('from', from);
	if (to) params.set('to', to);
	const qs = params.toString() ? `?${params.toString()}` : '';

	const query = useQuery<EvidenceData>({
		queryKey: ['magpie-evidence', from, to],
		queryFn: () => magpieApi<EvidenceData>(`/api/magpie/evidence${qs}`)
	});

	if (query.isLoading) {
		return (
			<div className="page-stack control-centre-page">
				<PageHeader title="Compliance & Evidence Pack" />
				<StatusBanner state="loading" title="Aggregating live audit evidence" />
			</div>
		);
	}

	if (query.isError || !query.data) {
		return (
			<div className="page-stack control-centre-page">
				<PageHeader title="Compliance & Evidence Pack" />
				<StatusBanner state="bad" title="Could not load evidence report">
					{query.error instanceof Error ? query.error.message : 'Access denied or unavailable.'}
				</StatusBanner>
			</div>
		);
	}

	const data = query.data;

	return (
		<div className="page-stack control-centre-page">
			<PageHeader
				title="Compliance & Evidence Pack"
				description="Live operational evidence aggregated directly from immutable gateway audit records for ASD ISM September 2026, Essential Eight, PSPF, and DTA Responsible AI 2.0."
				actions={
					data.canExport ? (
						<div className="control-centre-header-actions">
							<a className="button primary" href={`/api/magpie/evidence-pack.json${qs}`}>
								Download signed bundle (JSON + SHA-256)
							</a>
							<a className="button" href={`/api/magpie/evidence.pdf${qs}`}>
								Download PDF
							</a>
							<a className="button" href={`/api/magpie/evidence.csv${qs}`}>
								Download CSV
							</a>
							{data.canExportPolicy ? (
								<a className="button" href="/use-cases.csv">
									AI Use-Case Register (CSV)
								</a>
							) : null}
						</div>
					) : undefined
				}
			/>

			<Panel>
				<div className="cc-panel-head">
					<h2>Reporting period</h2>
					<p className="muted-copy">
						Active window: <strong>{data.periodLabel}</strong>. Every framework row explicitly
						states whether the mapping is direct or interpretive — a gateway alone does not satisfy
						agency endpoint or governance obligations.
					</p>
				</div>
				<div className="cc-form-grid">
					<div className="cc-field">
						<label htmlFor="ev-from">From date</label>
						<input id="ev-from" type="date" value={from} onChange={e => setFrom(e.target.value)} />
					</div>
					<div className="cc-field">
						<label htmlFor="ev-to">To date</label>
						<input id="ev-to" type="date" value={to} onChange={e => setTo(e.target.value)} />
					</div>
				</div>
				<div className="cc-preview-actions" style={{ marginTop: 12 }}>
					<button
						type="button"
						className="button"
						onClick={() => {
							setFrom(isoDateOffset(30));
							setTo(isoDateOffset(0));
						}}
					>
						Last 30 days
					</button>
					<button
						type="button"
						className="button"
						onClick={() => {
							setFrom(isoDateOffset(90));
							setTo(isoDateOffset(0));
						}}
					>
						Last 90 days
					</button>
					<button
						type="button"
						className="button"
						onClick={() => {
							setFrom('');
							setTo('');
						}}
					>
						All recorded history
					</button>
				</div>
			</Panel>

			<section className="cc-kpi-grid" aria-label="Live operational evidence counts">
				<div className="cc-kpi-card">
					<span className="cc-kpi-label">Evaluated requests</span>
					<strong className="cc-kpi-value">{data.stats.requests}</strong>
					<span className="cc-kpi-sub">{data.stats.blocks} blocked/denied</span>
				</div>
				<div className="cc-kpi-card">
					<span className="cc-kpi-label">Australian DLP findings</span>
					<strong className="cc-kpi-value">{data.stats.dlpHits}</strong>
					<span className="cc-kpi-sub">
						{data.posture.enabledDetectors}/{data.posture.totalDetectors} detectors enabled
					</span>
				</div>
				<div className="cc-kpi-card">
					<span className="cc-kpi-label">Policy & access changes</span>
					<strong className="cc-kpi-value">{data.stats.policyChanges}</strong>
					<span className="cc-kpi-sub">{data.stats.approvals} approval events</span>
				</div>
				<div className="cc-kpi-card">
					<span className="cc-kpi-label">Residency / PSPF denies</span>
					<strong className="cc-kpi-value">{data.stats.sovereignDenies}</strong>
					<span className="cc-kpi-sub">
						Sovereign mode: {data.posture.sovereignMode ? 'On' : 'Off'} · Retention target:{' '}
						{data.posture.retentionDays}d
					</span>
				</div>
			</section>

			<Panel>
				<div className="cc-panel-head">
					<h2>Framework assurance mapping ({data.rows.length} controls)</h2>
					<p className="muted-copy">
						Generated {new Date(data.generatedAt).toLocaleString()} from immutable SQLite audit
						records.
					</p>
				</div>
				{/* biome-ignore lint/a11y/noNoninteractiveTabindex: Scrollable table region requires keyboard access (WCAG 2.1.1). */}
				<section className="table-wrap" tabIndex={0} aria-label="Framework evidence rows">
					<table className="cc-table">
						<thead>
							<tr>
								<th scope="col">Framework & reference</th>
								<th scope="col">Magpie control</th>
								<th scope="col">Live observation</th>
								<th scope="col">Relationship & scope</th>
							</tr>
						</thead>
						<tbody>
							{data.rows.map(row => (
								<tr key={`${row.framework}:${row.reference}`}>
									<th scope="row">
										<div>{row.reference}</div>
										<span className="cc-cell-sub">{row.framework}</span>
									</th>
									<td>{row.magpieControl}</td>
									<td>
										<strong>{row.observation}</strong>
									</td>
									<td>{row.relationship}</td>
								</tr>
							))}
						</tbody>
					</table>
				</section>
			</Panel>
		</div>
	);
}
