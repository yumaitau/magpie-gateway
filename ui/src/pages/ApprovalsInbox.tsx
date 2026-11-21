import { useQuery, useQueryClient } from '@tanstack/react-query';
import { useState } from 'react';

import { PageHeader, Panel, StatusBanner } from '@/components/Primitives';
import { magpieApi } from '@/magpie';

type PendingRevision = {
	id: string;
	kind: string;
	docId: string;
	actor: string;
	createdAt: string;
	diff: string[];
	canApprove: boolean;
	canReject: boolean;
	cannotApproveReason?: string | null;
};

type ItemApproval = {
	id: string;
	kind: 'tool' | 'model' | 'mcpServer';
	name: string;
	businessOwner: string;
	purpose: string;
	classification: string;
	expiresOn: string;
	status: 'pending' | 'approved' | 'rejected' | 'expired';
	submittedBy: string;
	updatedAt: string;
	canApprove: boolean;
	canReject: boolean;
	cannotApproveReason?: string | null;
};

type ApprovalsData = {
	revisions: PendingRevision[];
	items: ItemApproval[];
	canApprove: boolean;
	canSubmitApproval: boolean;
	canExportPolicy: boolean;
	viewerEmail: string;
};

function kindLabel(kind: string): string {
	switch (kind) {
		case 'dlp_rule':
			return 'DLP rule';
		case 'settings':
			return 'Governance settings';
		case 'policy':
			return 'Policy';
		case 'mcpServer':
			return 'MCP server';
		case 'model':
			return 'Model';
		case 'tool':
			return 'Tool';
		default:
			return kind;
	}
}

function statusClass(status: ItemApproval['status']): string {
	switch (status) {
		case 'approved':
			return 'cc-pill cc-pill-ok';
		case 'pending':
			return 'cc-pill cc-pill-warn';
		case 'rejected':
		case 'expired':
			return 'cc-pill cc-pill-bad';
	}
}

export function ApprovalsInboxPage() {
	const queryClient = useQueryClient();
	const query = useQuery<ApprovalsData>({
		queryKey: ['magpie-approvals'],
		queryFn: () => magpieApi<ApprovalsData>('/api/magpie/approvals')
	});

	const [filter, setFilter] = useState<'all' | 'pending' | 'approved' | 'expired' | 'rejected'>(
		'all'
	);
	const [busy, setBusy] = useState('');
	const [error, setError] = useState('');
	const [notice, setNotice] = useState('');

	const [name, setName] = useState('');
	const [kind, setKind] = useState<'tool' | 'model' | 'mcpServer'>('model');
	const [businessOwner, setBusinessOwner] = useState('');
	const [purpose, setPurpose] = useState('');
	const [classification, setClassification] = useState('OFFICIAL');
	const [expiresOn, setExpiresOn] = useState('2027-06-30');

	async function refreshAll() {
		await queryClient.invalidateQueries({ queryKey: ['magpie-approvals'] });
		await queryClient.invalidateQueries({ queryKey: ['magpie-control-centre'] });
		await queryClient.invalidateQueries({ queryKey: ['magpie-session'] });
		window.dispatchEvent(new Event('magpie-settings-changed'));
	}

	async function decideRevision(id: string, decision: 'approve' | 'reject') {
		setError('');
		setNotice('');
		setBusy(`rev:${id}`);
		try {
			await magpieApi(`/api/magpie/approvals/revisions/${encodeURIComponent(id)}/decide`, {
				decision
			});
			await refreshAll();
			setNotice(
				decision === 'approve'
					? 'Change approved and activated.'
					: 'Pending change rejected. Active configuration is unchanged.'
			);
		} catch (err) {
			setError(err instanceof Error ? err.message : 'Could not record decision.');
		} finally {
			setBusy('');
		}
	}

	async function decideItem(id: string, decision: 'approve' | 'reject') {
		setError('');
		setNotice('');
		setBusy(`item:${id}`);
		try {
			await magpieApi(`/api/magpie/approvals/items/${encodeURIComponent(id)}/decide`, {
				decision
			});
			await refreshAll();
			setNotice(
				decision === 'approve'
					? 'Approval granted and active allow list updated.'
					: 'Approval request rejected.'
			);
		} catch (err) {
			setError(err instanceof Error ? err.message : 'Could not record decision.');
		} finally {
			setBusy('');
		}
	}

	async function handleSubmitNew(event: React.FormEvent) {
		event.preventDefault();
		setError('');
		setNotice('');
		setBusy('submit');
		try {
			await magpieApi('/api/magpie/approvals', {
				name,
				kind,
				businessOwner,
				purpose,
				classification,
				expiresOn
			});
			setName('');
			setBusinessOwner('');
			setPurpose('');
			await refreshAll();
			setNotice(
				'Approval request queued. A different authorised approver must release it before enforcement allows it.'
			);
		} catch (err) {
			setError(err instanceof Error ? err.message : 'Could not submit approval request.');
		} finally {
			setBusy('');
		}
	}

	if (query.isLoading) {
		return (
			<div className="page-stack control-centre-page">
				<PageHeader title="Approvals" />
				<StatusBanner state="loading" title="Loading approval queue" />
			</div>
		);
	}

	if (query.isError || !query.data) {
		return (
			<div className="page-stack control-centre-page">
				<PageHeader title="Approvals" />
				<StatusBanner state="bad" title="Could not load approvals">
					{query.error instanceof Error ? query.error.message : 'Access denied or unavailable.'}
				</StatusBanner>
			</div>
		);
	}

	const data = query.data;
	const filteredItems =
		filter === 'all' ? data.items : data.items.filter(item => item.status === filter);
	const pendingItemsCount = data.items.filter(item => item.status === 'pending').length;

	return (
		<div className="page-stack control-centre-page">
			<PageHeader
				title="Approvals"
				description="Review staged policy, DLP, and settings changes alongside model, tool, and MCP server approvals. Separation of duties prevents self-approval."
				actions={
					data.canExportPolicy ? (
						<a className="button" href="/use-cases.csv">
							Export AI use-case register (CSV)
						</a>
					) : undefined
				}
			/>

			{error ? (
				<StatusBanner state="bad" title="Could not complete action">
					{error}
				</StatusBanner>
			) : null}
			{notice ? (
				<StatusBanner state="ok" title="Saved">
					{notice}
				</StatusBanner>
			) : null}

			<Panel>
				<div className="cc-panel-head">
					<h2>Pending policy & settings changes ({data.revisions.length})</h2>
					<p className="muted-copy">
						Active rules stay in force until a second authorised person approves each staged change.
					</p>
				</div>
				{data.revisions.length === 0 ? (
					<p className="muted-copy">No policy, DLP, or settings changes are awaiting approval.</p>
				) : (
					<div className="cc-revision-list">
						{data.revisions.map(rev => (
							<article key={rev.id} className="cc-revision-card">
								<div className="cc-revision-header">
									<div>
										<span className="cc-pill cc-pill-warn">{kindLabel(rev.kind)}</span>{' '}
										<strong>{rev.docId}</strong>
									</div>
									<span className="cc-cell-sub">
										Submitted by {rev.actor} · {new Date(rev.createdAt).toLocaleString()}
									</span>
								</div>
								{rev.diff.length > 0 ? (
									<ul className="cc-diff-list">
										{rev.diff.map(line => (
											<li key={line}>
												<code>{line}</code>
											</li>
										))}
									</ul>
								) : (
									<p className="muted-copy">No field differences reported.</p>
								)}
								{rev.cannotApproveReason ? (
									<p className="cc-cell-sub">{rev.cannotApproveReason}</p>
								) : null}
								<div className="cc-preview-actions">
									<button
										type="button"
										className="button primary"
										disabled={!rev.canApprove || busy !== ''}
										onClick={() => void decideRevision(rev.id, 'approve')}
									>
										{busy === `rev:${rev.id}` ? 'Processing…' : 'Approve & activate'}
									</button>
									{rev.canReject ? (
										<button
											type="button"
											className="button"
											disabled={busy !== ''}
											onClick={() => void decideRevision(rev.id, 'reject')}
										>
											Reject
										</button>
									) : null}
								</div>
							</article>
						))}
					</div>
				)}
			</Panel>

			<Panel>
				<div className="cc-panel-head">
					<h2>Models, tools & MCP servers ({pendingItemsCount} pending)</h2>
					<p className="muted-copy">
						Every approved model, tool, or MCP server carries a business owner, purpose, PSPF
						classification, and expiry date.
					</p>
				</div>
				<fieldset className="cc-filter-bar cc-fieldset-bare">
					<legend className="sr-only">Filter approval status</legend>
					{(['all', 'pending', 'approved', 'expired', 'rejected'] as const).map(status => (
						<button
							key={status}
							type="button"
							className={`button${filter === status ? ' primary' : ''}`}
							aria-pressed={filter === status}
							onClick={() => setFilter(status)}
						>
							{status === 'all' ? 'All' : status[0].toUpperCase() + status.slice(1)}
						</button>
					))}
				</fieldset>

				{filteredItems.length === 0 ? (
					<p className="muted-copy">No approval records match this filter.</p>
				) : (
					// biome-ignore lint/a11y/noNoninteractiveTabindex: Scrollable table region requires keyboard access (WCAG 2.1.1).
					<section className="table-wrap" tabIndex={0} aria-label="Approval records">
						<table className="cc-table">
							<thead>
								<tr>
									<th scope="col">Name</th>
									<th scope="col">Type</th>
									<th scope="col">Business owner</th>
									<th scope="col">Classification</th>
									<th scope="col">Expiry</th>
									<th scope="col">Status</th>
									<th scope="col">Actions</th>
								</tr>
							</thead>
							<tbody>
								{filteredItems.map(item => (
									<tr key={item.id}>
										<th scope="row">
											<div>{item.name}</div>
											<span className="cc-cell-sub">{item.purpose}</span>
										</th>
										<td>{kindLabel(item.kind)}</td>
										<td>
											<div>{item.businessOwner}</div>
											<span className="cc-cell-sub">By {item.submittedBy}</span>
										</td>
										<td>{item.classification}</td>
										<td>{item.expiresOn}</td>
										<td>
											<span className={statusClass(item.status)}>{item.status}</span>
										</td>
										<td>
											{item.status === 'pending' ? (
												<div className="cc-inline-actions">
													<button
														type="button"
														className="button primary"
														disabled={!item.canApprove || busy !== ''}
														title={item.cannotApproveReason ?? undefined}
														onClick={() => void decideItem(item.id, 'approve')}
													>
														Approve
													</button>
													{item.canReject ? (
														<button
															type="button"
															className="button"
															disabled={busy !== ''}
															onClick={() => void decideItem(item.id, 'reject')}
														>
															Reject
														</button>
													) : null}
													{item.cannotApproveReason ? (
														<span className="cc-cell-sub">{item.cannotApproveReason}</span>
													) : null}
												</div>
											) : (
												<span className="cc-cell-sub">—</span>
											)}
										</td>
									</tr>
								))}
							</tbody>
						</table>
					</section>
				)}

				{data.canSubmitApproval ? (
					<details className="cc-disclosure">
						<summary>Submit new model, tool, or MCP server approval request</summary>
						<form className="cc-form" onSubmit={e => void handleSubmitNew(e)}>
							<div className="cc-form-grid">
								<div className="cc-field">
									<label htmlFor="appr-name">Name</label>
									<input
										id="appr-name"
										required
										value={name}
										placeholder="e.g. azure-openai/gpt-5 or sharepoint_search"
										onChange={e => setName(e.target.value)}
									/>
								</div>
								<div className="cc-field">
									<label htmlFor="appr-kind">Type</label>
									<select
										id="appr-kind"
										value={kind}
										onChange={e => setKind(e.target.value as 'tool' | 'model' | 'mcpServer')}
									>
										<option value="model">Model</option>
										<option value="tool">Tool</option>
										<option value="mcpServer">MCP server</option>
									</select>
								</div>
								<div className="cc-field">
									<label htmlFor="appr-owner">Business owner</label>
									<input
										id="appr-owner"
										required
										value={businessOwner}
										placeholder="Name or email of accountable owner"
										onChange={e => setBusinessOwner(e.target.value)}
									/>
								</div>
								<div className="cc-field">
									<label htmlFor="appr-classification">Data classification</label>
									<select
										id="appr-classification"
										value={classification}
										onChange={e => setClassification(e.target.value)}
									>
										<option value="OFFICIAL">OFFICIAL</option>
										<option value="OFFICIAL: Sensitive">OFFICIAL: Sensitive</option>
										<option value="PROTECTED">PROTECTED</option>
									</select>
								</div>
								<div className="cc-field">
									<label htmlFor="appr-expiry">Expiry date</label>
									<input
										id="appr-expiry"
										type="date"
										required
										value={expiresOn}
										onChange={e => setExpiresOn(e.target.value)}
									/>
								</div>
							</div>
							<div className="cc-field">
								<label htmlFor="appr-purpose">Purpose</label>
								<input
									id="appr-purpose"
									required
									value={purpose}
									placeholder="Describe approved agency or business use"
									onChange={e => setPurpose(e.target.value)}
								/>
							</div>
							<button type="submit" className="button primary" disabled={busy !== ''}>
								{busy === 'submit' ? 'Submitting…' : 'Queue approval request'}
							</button>
						</form>
					</details>
				) : null}
			</Panel>
		</div>
	);
}
