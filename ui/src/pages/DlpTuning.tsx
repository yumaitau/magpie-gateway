import { useQuery } from '@tanstack/react-query';
import { Link } from '@tanstack/react-router';
import { useState } from 'react';

import { PageHeader, StatusBanner } from '@/components/Primitives';
import { magpieApi } from '@/magpie';

type Rule = {
	kind: string;
	name: string;
	action: string;
	enabled: boolean;
	revision: number;
	pending: boolean;
	exceptions: { id: string; label: string }[];
};
type Settings = {
	rules: Rule[];
	mode: string;
	requiresApproval: boolean;
	canEdit: boolean;
	canAudit: boolean;
	recent: Record<string, number>;
};
type Hit = { kind: string; action: string; note: string; start: number; end: number };
type Result = {
	active: { action: string; hits: Hit[] };
	draft: { action: string; hits: Hit[] };
	sanitized: string;
	mode: string;
};
const actions = [
	['log', 'Monitor only: record findings, pass text'],
	['redact', 'Redact: remove the matched value'],
	['mask', 'Mask: reveal only the last two characters'],
	['tokenise', 'Pseudonym: replace with a stable keyed token'],
	['block', 'Block the request or response'],
	['requireApproval', 'Hold: require approval'],
	['allow', 'Allow: pass text and keep findings']
];
const names: Record<string, string> = {
	abn: 'Australian Business Number (ABN)',
	acn: 'Australian Company Number (ACN)',
	tfn: 'Tax File Number (TFN)',
	medicare: 'Medicare number',
	ihi: 'Individual Healthcare Identifier (IHI)',
	dva: 'Veteran identifier (DVA)',
	email: 'Email address',
	paymentCard: 'Payment card',
	crn: 'Centrelink reference (CRN)',
	passport: 'Passport number',
	passportMrz: 'Passport machine-readable zone',
	driverLicence: 'Driver licence',
	bsb: 'Bank branch (BSB)',
	accountNumber: 'Bank account number',
	phone: 'Phone number',
	address: 'Street address',
	marking: 'Protective marking',
	caveat: 'Handling caveat',
	secret: 'Credential or secret',
	presidio: 'External Presidio analyser'
};
const name = (kind: string) => names[kind] || kind;
const actionName = (action: string) => (action === 'requireApproval' ? 'require approval' : action);

export function DlpTuningPage() {
	const query = useQuery({
		queryKey: ['dlp-tuning'],
		queryFn: () => magpieApi<Settings>('/api/magpie/dlp')
	});
	const [chosen, setChosen] = useState('');
	const [notice, setNotice] = useState('');
	const initial = new URLSearchParams(window.location.search).get('detector');
	const rule =
		query.data?.rules.find(r => r.kind === (chosen || initial)) ||
		query.data?.rules.find(r => r.kind === 'email');
	return (
		<div className="page-stack dlp-page">
			<PageHeader
				title="Tune DLP"
				description="Find the detector behind a false positive, compare a change, then save it for review."
			/>
			<p>
				<Link to="/help/$topic" params={{ topic: 'dlp' }}>
					Step-by-step tuning guide
				</Link>{' '}
				·{' '}
				<Link to="/governance/$page" params={{ page: 'audit' }}>
					Review blocked requests
				</Link>
			</p>
			{query.isPending && <p role="status">Loading detectors…</p>}
			{query.error && (
				<div role="alert">
					<StatusBanner state="bad" title="Detectors unavailable">
						{query.error.message}
					</StatusBanner>
				</div>
			)}
			{notice && <p role="status">{notice}</p>}
			{query.data && rule && (
				<>
					<p>
						<strong>
							Gateway mode: {query.data.mode === 'log-only' ? 'Observe only' : 'Enforcing'}
						</strong>
						. This test checks local DLP only. Model approvals, routing, protective markings and
						prompt injection may also block a request.
					</p>
					<details className="schema-details">
						<summary>Which detector is causing the problem?</summary>
						<p>
							Findings from the last 100 gateway decisions you can audit. These counts are findings,
							not confirmed false positives.
						</p>
						{!query.data.canAudit ? (
							<p>
								Your role cannot view audit findings. Ask an administrator to identify the detector.
							</p>
						) : Object.keys(query.data.recent).length ? (
							<ul>
								{Object.entries(query.data.recent).map(([kind, count]) => (
									<li key={kind}>
										<button
											type="button"
											className="button"
											onClick={() => {
												setChosen(kind);
												setNotice('');
											}}
										>
											{name(kind)}: {count} findings
										</button>
									</li>
								))}
							</ul>
						) : (
							<p>
								No recent DLP findings. Check the audit reason for approval, residency or injection
								blocks.
							</p>
						)}
					</details>
					<label htmlFor="tuning-detector">1. Choose a detector</label>
					<select
						id="tuning-detector"
						value={rule.kind}
						onChange={e => {
							setChosen(e.target.value);
							setNotice('');
						}}
					>
						{query.data.rules.map(r => (
							<option key={r.kind} value={r.kind}>
								{name(r.kind)}
							</option>
						))}
					</select>
					<Tuner
						key={`${rule.kind}:${rule.revision}:${rule.pending}`}
						rule={rule}
						settings={query.data}
						onSaved={async pending => {
							setNotice(
								pending
									? 'Saved for approval. The active rule has not changed. A different authorised person must approve it.'
									: 'Saved and active for new requests.'
							);
							await query.refetch();
						}}
					/>
				</>
			)}
		</div>
	);
}
function Tuner({
	rule,
	settings,
	onSaved
}: {
	rule: Rule;
	settings: Settings;
	onSaved: (pending: boolean) => Promise<void>;
}) {
	const [action, setAction] = useState(rule.action);
	const [enabled, setEnabled] = useState(rule.enabled);
	const [text, setText] = useState('');
	const [exactValue, setExactValue] = useState('');
	const [exceptionLabel, setLabel] = useState('');
	const [removed, setRemoved] = useState<string[]>([]);
	const [result, setResult] = useState<Result>();
	const [error, setError] = useState('');
	const [busy, setBusy] = useState(false);
	const draft = {
		kind: rule.kind,
		expectedRevision: rule.revision,
		action,
		enabled,
		exactValue,
		exceptionLabel,
		removeExceptions: removed
	};
	function changed() {
		setResult(undefined);
		setError('');
	}
	async function preview() {
		setResult(undefined);
		setBusy(true);
		setError('');
		try {
			setResult(await magpieApi<Result>('/api/magpie/dlp/preview', { ...draft, text }));
		} catch (e) {
			setError(e instanceof Error ? e.message : 'Could not test this sample.');
		} finally {
			setBusy(false);
		}
	}
	async function save() {
		setBusy(true);
		setError('');
		try {
			const saved = await magpieApi<{ pending: boolean }>('/api/magpie/dlp/save', draft);
			setExactValue('');
			setLabel('');
			setResult(undefined);
			await onSaved(saved.pending);
		} catch (e) {
			setError(e instanceof Error ? e.message : 'Could not save.');
		} finally {
			setBusy(false);
		}
	}
	return (
		<section className="dlp-tuner" aria-label={`${name(rule.kind)} tuning`}>
			<p>
				<strong>Active rule:</strong> {rule.enabled ? actionName(rule.action) : 'off'} ·{' '}
				{rule.exceptions.length} exact exceptions{rule.pending ? ' · change pending approval' : ''}.
			</p>
			{rule.pending && (
				<p>
					A pending change already exists.{' '}
					<Link to="/governance/$page" params={{ page: 'dlp' }}>
						Review pending changes
					</Link>{' '}
					before saving another.
				</p>
			)}
			{!settings.canEdit && (
				<p>Your role can read these settings. Ask a policy editor to test or change them.</p>
			)}
			{error && <p role="alert">{error}</p>}
			<fieldset disabled={!settings.canEdit || busy}>
				<legend>2. Adjust this detector</legend>
				<label htmlFor="tuning-action">When this detector matches</label>
				<select
					id="tuning-action"
					value={action}
					onChange={e => {
						setAction(e.target.value);
						changed();
					}}
				>
					{actions.map(([value, label]) => (
						<option key={value} value={value}>
							{label}
						</option>
					))}
				</select>
				<p>
					Monitor only leaves this detector’s matches unchanged while other detectors keep their own
					actions. Overlapping findings can still redact or block the same text.
				</p>
				<details className="schema-details">
					<summary>Exact-value exceptions and advanced options</summary>
					<label className="magpie-check">
						<input
							type="checkbox"
							checked={enabled}
							onChange={e => {
								setEnabled(e.target.checked);
								changed();
							}}
						/>
						<span>Detector enabled</span>
					</label>
					<p>
						Off removes this detector’s findings and actions. Turning off Presidio stops analyser
						requests. Marking and caveat settings do not disable classification routing.
					</p>
					{!['marking', 'caveat'].includes(rule.kind) && (
						<>
							<label htmlFor="exception-value">Exact value to ignore</label>
							<input
								id="exception-value"
								value={exactValue}
								maxLength={512}
								autoComplete="off"
								onChange={e => {
									setExactValue(e.target.value);
									changed();
								}}
							/>
							<p>
								Enter only the detected value, not the surrounding sentence. Matches are
								case-sensitive and include spaces and punctuation. This exception applies to this
								detector across all connected traffic. Other detectors still apply. Presidio
								receives text before its findings can be excluded.
							</p>
							<label htmlFor="exception-label">Exception label</label>
							<input
								id="exception-label"
								value={exceptionLabel}
								maxLength={80}
								placeholder="For example: synthetic test fixture"
								onChange={e => {
									setLabel(e.target.value);
									changed();
								}}
							/>
							<p>
								Use a label without the original value. The original is replaced with a keyed
								fingerprint before storage. Preview the exact spelling you intend to allow.
							</p>
						</>
					)}
					{rule.exceptions.length > 0 && (
						<>
							<h3>Existing exceptions</h3>
							<p>Check an exception to remove it in this draft.</p>
							{rule.exceptions.map(e => (
								<label key={e.id} className="magpie-check">
									<input
										type="checkbox"
										checked={removed.includes(e.id)}
										onChange={event => {
											setRemoved(
												event.target.checked
													? [...removed, e.id]
													: removed.filter(id => id !== e.id)
											);
											changed();
										}}
									/>
									<span>Remove {e.label}</span>
								</label>
							))}
						</>
					)}
				</details>
				<h2>3. Compare before saving</h2>
				<label htmlFor="tuning-sample">Synthetic sample text</label>
				<textarea
					id="tuning-sample"
					rows={4}
					value={text}
					maxLength={16384}
					onChange={e => {
						setText(e.target.value);
						changed();
					}}
				/>
				<p>
					The sample stays in this tab and the test request. It is not saved, audited or sent to a
					model or external analyser. External Presidio results are not simulated.
				</p>
				<button
					type="button"
					className="button"
					onClick={() => void preview()}
					disabled={!text.trim()}
				>
					{busy ? 'Working…' : 'Compare active and draft'}
				</button>
				{result && (
					<div aria-live="polite">
						<h3>Comparison</h3>
						<p>
							Active DLP: <strong>{actionName(result.active.action)}</strong> (
							{result.active.hits.length} findings). Draft DLP:{' '}
							<strong>{actionName(result.draft.action)}</strong> ({result.draft.hits.length}{' '}
							findings).
						</p>
						<p>
							{result.mode === 'log-only'
								? 'The gateway is log-only: traffic currently passes unchanged. These are the actions DLP would choose in enforcement.'
								: 'The gateway is enforcing. These are local DLP decisions, not a test of the complete route.'}
						</p>
						{/* biome-ignore lint/a11y/noNoninteractiveTabindex: Makes a horizontally scrolling results table keyboard accessible. */}
						<section className="table-wrap" tabIndex={0} aria-label="DLP comparison">
							<table>
								<thead>
									<tr>
										<th>Version</th>
										<th>Detector</th>
										<th>Action</th>
										<th>Why it matched</th>
										<th>Position (UTF-8 bytes)</th>
									</tr>
								</thead>
								<tbody>
									{(['active', 'draft'] as const).flatMap(version =>
										result[version].hits.map(hit => (
											<tr key={`${version}-${hit.kind}-${hit.start}-${hit.end}-${hit.note}`}>
												<td>{version}</td>
												<td>{name(hit.kind)}</td>
												<td>{actionName(hit.action)}</td>
												<td>{hit.note}</td>
												<td>
													{hit.start}–{hit.end}
												</td>
											</tr>
										))
									)}
								</tbody>
							</table>
						</section>
						<h3>Sanitised reference</h3>
						<pre>{result.sanitized}</pre>
						<p>
							This reference always hides recognised values, even when the draft would allow them.
						</p>
					</div>
				)}
				<h2>4. Save the change</h2>
				<p>
					{settings.requiresApproval
						? 'A different authorised person must approve the revision. Until then, the active rule stays in use.'
						: 'This deployment activates saved changes immediately.'}
				</p>
				<button
					type="button"
					className="button primary"
					disabled={
						!result ||
						rule.pending ||
						(action === rule.action && enabled === rule.enabled && !exactValue && !removed.length)
					}
					onClick={() => void save()}
				>
					{settings.requiresApproval ? 'Save for approval' : 'Save and activate'}
				</button>
			</fieldset>
			<p>
				<Link to="/governance/$page" params={{ page: 'dlp' }}>
					All detector settings, pending approvals and restore defaults
				</Link>
			</p>
		</section>
	);
}
