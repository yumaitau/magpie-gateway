import { useQuery, useQueryClient } from '@tanstack/react-query';
import { Link } from '@tanstack/react-router';
import { useState } from 'react';

import { PageHeader, Panel, StatusBanner } from '@/components/Primitives';
import { magpieApi } from '@/magpie';

type IdentityData = {
	oidcEnabled: boolean;
	oidcKind: string;
	issuer: string;
	clientId: string;
	publicUrl: string;
	redirectUri: string;
	scimUrl: string;
	scimConfigured: boolean;
	scimTokenSource: 'database' | 'environment' | 'none';
	scimUsers: {
		id: string;
		userName: string;
		displayName?: string;
		externalId?: string;
		active: boolean;
	}[];
	scimGroups: {
		id: string;
		displayName: string;
		externalId?: string;
		memberCount: number;
	}[];
	groups: {
		id: string;
		name: string;
		roles: string[];
		identityGroups: string[];
		enforcement: string;
	}[];
	roles: { id: string; name: string }[];
	appRoles: { value: string; name: string; description: string }[];
	canManageAccess: boolean;
};

type OidcTestResult = {
	ok: boolean;
	url: string;
	issuer?: string;
	jwksUri?: string;
	error?: string;
};

export function IdentityEntraPage() {
	const queryClient = useQueryClient();
	const query = useQuery<IdentityData>({
		queryKey: ['magpie-identity'],
		queryFn: () => magpieApi<IdentityData>('/api/magpie/identity')
	});

	const [busy, setBusy] = useState('');
	const [error, setError] = useState('');
	const [notice, setNotice] = useState('');
	const [newScimToken, setNewScimToken] = useState('');
	const [oidcResult, setOidcResult] = useState<OidcTestResult | null>(null);
	const [mapRole, setMapRole] = useState<Record<string, string>>({});
	const [mapMode, setMapMode] = useState<Record<string, string>>({});

	async function handleRotateToken() {
		setError('');
		setNotice('');
		setBusy('rotate');
		try {
			const res = await magpieApi<{ token: string; note: string }>(
				'/api/magpie/identity/scim-token',
				{}
			);
			setNewScimToken(res.token);
			setNotice(res.note);
			await queryClient.invalidateQueries({ queryKey: ['magpie-identity'] });
		} catch (err) {
			setError(err instanceof Error ? err.message : 'Could not rotate SCIM token.');
		} finally {
			setBusy('');
		}
	}

	async function handleTestOidc() {
		setError('');
		setOidcResult(null);
		setBusy('oidc');
		try {
			const res = await magpieApi<OidcTestResult>('/api/magpie/identity/test-oidc', {});
			setOidcResult(res);
		} catch (err) {
			setError(err instanceof Error ? err.message : 'Could not test OIDC discovery.');
		} finally {
			setBusy('');
		}
	}

	async function handleMapGroup(scimGroupId: string, displayName: string) {
		setError('');
		setNotice('');
		setBusy(`map:${scimGroupId}`);
		try {
			await magpieApi('/api/magpie/identity/map-scim-group', {
				scimGroupId,
				roleId: mapRole[scimGroupId] || 'viewer',
				enforcement: mapMode[scimGroupId] || 'follow'
			});
			await queryClient.invalidateQueries({ queryKey: ['magpie-identity'] });
			await queryClient.invalidateQueries({ queryKey: ['magpie-control-centre'] });
			setNotice(`Mapped Entra group "${displayName}" to Magpie RBAC & enforcement cohorts.`);
		} catch (err) {
			setError(err instanceof Error ? err.message : 'Could not map SCIM group.');
		} finally {
			setBusy('');
		}
	}

	if (query.isLoading) {
		return (
			<div className="page-stack control-centre-page">
				<PageHeader title="Identity & Microsoft Entra" />
				<StatusBanner state="loading" title="Loading identity configuration" />
			</div>
		);
	}

	if (query.isError || !query.data) {
		return (
			<div className="page-stack control-centre-page">
				<PageHeader title="Identity & Microsoft Entra" />
				<StatusBanner state="bad" title="Could not load identity settings">
					{query.error instanceof Error ? query.error.message : 'Access denied or unavailable.'}
				</StatusBanner>
			</div>
		);
	}

	const data = query.data;
	const entraScript = `# Register Magpie Gateway in Microsoft Entra ID (PowerShell / Azure CLI)
$DisplayName = "Magpie Gateway"
$RedirectUri = "${data.redirectUri}"
$App = az ad app create --display-name $DisplayName --web-redirect-uris $RedirectUri --sign-in-audience AzureADMyOrg | ConvertFrom-Json
az ad sp create --id $App.appId | Out-Null
Write-Host "MAGPIE_ENTRA_TENANT_ID=$((az account show --query tenantId -o tsv))"
Write-Host "MAGPIE_OIDC_CLIENT_ID=$($App.appId)"
Write-Host "MAGPIE_PUBLIC_URL=${data.publicUrl}"
# Configure SCIM Provisioning in Entra Enterprise Applications:
# Tenant URL:   ${data.scimUrl}
# Secret Token: (generate below in Magpie Identity & Microsoft Entra)`;

	return (
		<div className="page-stack control-centre-page">
			<PageHeader
				title="Identity & Microsoft Entra"
				description="Connect Microsoft Entra ID single sign-on (OIDC + PKCE) and persistent SCIM 2.0 provisioning so IT manages AI access, cohorts, and instant offboarding directly from Entra."
				actions={
					data.canManageAccess ? (
						<Link className="button" to="/governance/$page" params={{ page: 'access' }}>
							Roles & groups
						</Link>
					) : undefined
				}
			/>

			{error ? (
				<StatusBanner state="bad" title="Action failed">
					{error}
				</StatusBanner>
			) : null}
			{notice ? (
				<StatusBanner state="ok" title="Updated">
					{notice}
				</StatusBanner>
			) : null}

			<section className="cc-kpi-grid" aria-label="Identity summary">
				<div className="cc-kpi-card">
					<span className="cc-kpi-label">Single sign-on (OIDC)</span>
					<strong className="cc-kpi-value">
						{data.oidcEnabled ? `Connected (${data.oidcKind.toUpperCase()})` : 'Not configured'}
					</strong>
					<span className="cc-kpi-sub">
						{data.oidcEnabled
							? 'Phishing-resistant MFA enforced by Entra Conditional Access'
							: 'Using local administrator accounts only'}
					</span>
				</div>
				<div className="cc-kpi-card">
					<span className="cc-kpi-label">SCIM 2.0 provisioning</span>
					<strong className="cc-kpi-value">
						{data.scimConfigured ? 'Active' : 'Token not set'}
					</strong>
					<span className="cc-kpi-sub">
						Source: {data.scimTokenSource} · Deprovisioning revokes sessions immediately
					</span>
				</div>
				<div className="cc-kpi-card">
					<span className="cc-kpi-label">Synced SCIM users</span>
					<strong className="cc-kpi-value">{data.scimUsers.length}</strong>
					<span className="cc-kpi-sub">Persisted in SQLite across restarts</span>
				</div>
				<div className="cc-kpi-card">
					<span className="cc-kpi-label">Synced Entra groups</span>
					<strong className="cc-kpi-value">{data.scimGroups.length}</strong>
					<span className="cc-kpi-sub">Ready to bind to RBAC & rollout cohorts</span>
				</div>
			</section>

			<Panel>
				<div className="cc-panel-head">
					<h2>Microsoft Entra ID single sign-on</h2>
					<p className="muted-copy">
						Verify your OIDC discovery endpoint and copy exact Redirect URI and App Role values for
						your Entra Enterprise Application.
					</p>
				</div>

				<dl className="cc-meta-list">
					<div>
						<dt>OIDC Redirect URI</dt>
						<dd>
							<code>{data.redirectUri}</code>
						</dd>
					</div>
					<div>
						<dt>SCIM 2.0 Tenant URL</dt>
						<dd>
							<code>{data.scimUrl}</code>
						</dd>
					</div>
					<div>
						<dt>Configured Issuer</dt>
						<dd>
							<code>{data.issuer || 'Not configured'}</code>
						</dd>
					</div>
					<div>
						<dt>Application (Client) ID</dt>
						<dd>
							<code>{data.clientId || 'Not configured'}</code>
						</dd>
					</div>
				</dl>

				<div className="cc-preview-actions" style={{ marginTop: 14 }}>
					<button
						type="button"
						className="button primary"
						disabled={!data.oidcEnabled || busy !== ''}
						onClick={() => void handleTestOidc()}
					>
						{busy === 'oidc' ? 'Testing…' : 'Test OIDC discovery'}
					</button>
				</div>

				{oidcResult ? (
					<div style={{ marginTop: 12 }}>
						<StatusBanner
							state={oidcResult.ok ? 'ok' : 'bad'}
							title={oidcResult.ok ? 'OIDC discovery reachable' : 'OIDC discovery check failed'}
						>
							{oidcResult.ok
								? `Issuer: ${oidcResult.issuer} · JWKS: ${oidcResult.jwksUri}`
								: oidcResult.error}
						</StatusBanner>
					</div>
				) : null}

				<details className="cc-disclosure" style={{ marginTop: 14 }}>
					<summary>Entra ID App Roles & PowerShell registration snippet</summary>
					{/* biome-ignore lint/a11y/noNoninteractiveTabindex: Scrollable table region requires keyboard access (WCAG 2.1.1). */}
					<section className="table-wrap" tabIndex={0} aria-label="Entra App Roles">
						<table className="cc-table">
							<thead>
								<tr>
									<th scope="col">App Role value</th>
									<th scope="col">Magpie role</th>
									<th scope="col">Scope</th>
								</tr>
							</thead>
							<tbody>
								{data.appRoles.map(role => (
									<tr key={role.value}>
										<th scope="row">
											<code>{role.value}</code>
										</th>
										<td>{role.name}</td>
										<td>{role.description}</td>
									</tr>
								))}
							</tbody>
						</table>
					</section>
					<pre className="cc-code-block">{entraScript}</pre>
				</details>
			</Panel>

			<Panel>
				<div className="cc-panel-head">
					<h2>SCIM 2.0 token & synced Entra groups</h2>
					<p className="muted-copy">
						When Entra ID deactivates or removes a user over SCIM, Magpie disables their portal
						account, revokes active sessions immediately, and records an audit event.
					</p>
				</div>

				{data.canManageAccess ? (
					<div className="cc-preview-actions" style={{ marginBottom: 14 }}>
						<button
							type="button"
							className="button primary"
							disabled={busy !== ''}
							onClick={() => void handleRotateToken()}
						>
							{busy === 'rotate'
								? 'Generating…'
								: data.scimConfigured
									? 'Rotate SCIM bearer token'
									: 'Generate SCIM bearer token'}
						</button>
					</div>
				) : null}

				{newScimToken ? (
					<div className="cc-field" style={{ marginBottom: 16 }}>
						<label htmlFor="scim-new-token">New SCIM secret token (shown once)</label>
						<input id="scim-new-token" readOnly value={newScimToken} />
					</div>
				) : null}

				<h3>Synced Entra groups ({data.scimGroups.length})</h3>
				{data.scimGroups.length === 0 ? (
					<p className="muted-copy">
						No SCIM groups synced yet. Assign security groups to your Magpie Gateway Enterprise
						Application in Microsoft Entra ID and start provisioning.
					</p>
				) : (
					// biome-ignore lint/a11y/noNoninteractiveTabindex: Scrollable table region requires keyboard access (WCAG 2.1.1).
					<section className="table-wrap" tabIndex={0} aria-label="Synced SCIM groups">
						<table className="cc-table">
							<thead>
								<tr>
									<th scope="col">Group name</th>
									<th scope="col">External ID</th>
									<th scope="col">Members</th>
									<th scope="col">Map role</th>
									<th scope="col">Cohort mode</th>
									<th scope="col">Action</th>
								</tr>
							</thead>
							<tbody>
								{data.scimGroups.map(group => (
									<tr key={group.id}>
										<th scope="row">{group.displayName}</th>
										<td>
											<code>{group.externalId || group.id}</code>
										</td>
										<td>{group.memberCount}</td>
										<td>
											<select
												aria-label={`Role for ${group.displayName}`}
												disabled={!data.canManageAccess}
												value={mapRole[group.id] || 'viewer'}
												onChange={e =>
													setMapRole(prev => ({ ...prev, [group.id]: e.target.value }))
												}
											>
												{data.roles.map(r => (
													<option key={r.id} value={r.id}>
														{r.name}
													</option>
												))}
											</select>
										</td>
										<td>
											<select
												aria-label={`Enforcement mode for ${group.displayName}`}
												disabled={!data.canManageAccess}
												value={mapMode[group.id] || 'follow'}
												onChange={e =>
													setMapMode(prev => ({ ...prev, [group.id]: e.target.value }))
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
												className="button primary"
												disabled={!data.canManageAccess || busy !== ''}
												onClick={() => void handleMapGroup(group.id, group.displayName)}
											>
												{busy === `map:${group.id}` ? 'Mapping…' : 'Bind group'}
											</button>
										</td>
									</tr>
								))}
							</tbody>
						</table>
					</section>
				)}

				<h3 style={{ marginTop: 20 }}>Synced SCIM users ({data.scimUsers.length})</h3>
				{data.scimUsers.length === 0 ? (
					<p className="muted-copy">No SCIM users provisioned yet.</p>
				) : (
					// biome-ignore lint/a11y/noNoninteractiveTabindex: Scrollable table region requires keyboard access (WCAG 2.1.1).
					<section className="table-wrap" tabIndex={0} aria-label="Synced SCIM users">
						<table className="cc-table">
							<thead>
								<tr>
									<th scope="col">User name</th>
									<th scope="col">Display name</th>
									<th scope="col">External ID</th>
									<th scope="col">Status</th>
								</tr>
							</thead>
							<tbody>
								{data.scimUsers.map(user => (
									<tr key={user.id}>
										<th scope="row">{user.userName}</th>
										<td>{user.displayName || '—'}</td>
										<td>
											<code>{user.externalId || '—'}</code>
										</td>
										<td>
											<span className={`cc-pill ${user.active ? 'cc-pill-ok' : 'cc-pill-bad'}`}>
												{user.active ? 'Active' : 'Disabled'}
											</span>
										</td>
									</tr>
								))}
							</tbody>
						</table>
					</section>
				)}
			</Panel>
		</div>
	);
}
