import { useQuery, useQueryClient } from '@tanstack/react-query';
import { Link } from '@tanstack/react-router';
import type { FormEvent, ReactNode } from 'react';
import { useState } from 'react';

import { PageHeader, Panel, StatusBanner } from '@/components/Primitives';
import { magpieApi } from '@/magpie';

function ErrorMessage({ error }: { error: string }) {
	return error ? (
		<div role="alert">
			<StatusBanner state="bad" title="Please check">
				{error}
			</StatusBanner>
		</div>
	) : null;
}
function PasswordFields() {
	return (
		<>
			<label htmlFor="new-password">New password</label>
			<input
				id="new-password"
				name="password"
				type="password"
				autoComplete="new-password"
				minLength={16}
				required
			/>
			<p className="muted-copy">
				At least 16 characters. A long passphrase works well. Maximum 72 UTF-8 bytes.
			</p>
			<label htmlFor="confirm-password">Confirm new password</label>
			<input
				id="confirm-password"
				name="confirmPassword"
				type="password"
				autoComplete="new-password"
				minLength={16}
				required
			/>
		</>
	);
}
const message = (e: unknown) => (e instanceof Error ? e.message : 'The request failed.');
export function MagpieAccountPage() {
	const account = useQuery({
		queryKey: ['magpie-account'],
		queryFn: () =>
			magpieApi<{ name: string; email: string; local: boolean; canManagePeople: boolean }>(
				'/api/magpie/account'
			)
	});
	const [error, setError] = useState('');
	const [busy, setBusy] = useState(false);
	async function submit(e: FormEvent<HTMLFormElement>) {
		e.preventDefault();
		setBusy(true);
		setError('');
		const f = new FormData(e.currentTarget);
		try {
			await magpieApi('/api/magpie/account/password', Object.fromEntries(f));
			location.assign('/ui/login?passwordChanged=1');
		} catch (e) {
			setError(message(e));
		} finally {
			setBusy(false);
		}
	}
	return (
		<div className="page-stack">
			<PageHeader title="My account" description="Manage your sign-in and account security." />
			<ErrorMessage error={error || account.error?.message || ''} />
			<Panel>
				<h3>{account.data?.name || 'Your account'}</h3>
				<p>{account.data?.email}</p>
				{account.data?.canManagePeople && (
					<Link to="/people">Invite a colleague or manage people</Link>
				)}
			</Panel>
			{account.data?.local ? (
				<Panel>
					<h3>Change password</h3>
					<p>
						Changing your password signs out all your sessions. Sign in again with the new password.
					</p>
					<form className="magpie-account-form" onSubmit={submit}>
						<label htmlFor="current-password">Current password</label>
						<input
							id="current-password"
							name="currentPassword"
							type="password"
							autoComplete="current-password"
							required
						/>
						<PasswordFields />
						<button type="submit" className="button primary" disabled={busy}>
							{busy ? 'Changing password…' : 'Change password'}
						</button>
					</form>
				</Panel>
			) : account.data ? (
				<Panel>
					<h3>Password managed by your organisation</h3>
					<p>
						You sign in with single sign-on. Change or reset your password through your
						organisation’s identity provider.
					</p>
				</Panel>
			) : (
				<p role="status">Loading account…</p>
			)}
		</div>
	);
}
type Group = { id: string; name: string; roles: string[]; members: string[] };
type Person = {
	subject: string;
	name: string;
	email: string;
	disabled: boolean;
	local: boolean;
	canReset: boolean;
	lastLogin: string;
};
type AccountLink = { id: string; kind: string; name: string; email: string; expiresAt: string };
type People = { users: Person[]; groups: Group[]; links: AccountLink[]; currentSubject: string };
type CreatedLink = { link: string; email: string; expiresAt: string };
export function MagpiePeoplePage() {
	const client = useQueryClient();
	const people = useQuery({
		queryKey: ['magpie-people'],
		queryFn: () => magpieApi<People>('/api/magpie/people')
	});
	const [error, setError] = useState('');
	const [busy, setBusy] = useState(false);
	const [created, setCreated] = useState<CreatedLink>();
	const [copied, setCopied] = useState(false);
	const refresh = () => client.invalidateQueries({ queryKey: ['magpie-people'] });
	async function invite(e: FormEvent<HTMLFormElement>) {
		e.preventDefault();
		setError('');
		setBusy(true);
		const form = e.currentTarget;
		const f = new FormData(form);
		try {
			setCreated(
				await magpieApi('/api/magpie/invitations', {
					email: f.get('email'),
					name: f.get('name'),
					groups: f.getAll('group')
				})
			);
			setCopied(false);
			form.reset();
			await refresh();
		} catch (e) {
			setError(message(e));
		} finally {
			setBusy(false);
		}
	}
	async function reset(subject: string) {
		setError('');
		setBusy(true);
		try {
			setCreated(await magpieApi('/api/magpie/password-reset', { subject }));
			setCopied(false);
			await refresh();
		} catch (e) {
			setError(message(e));
		} finally {
			setBusy(false);
		}
	}
	async function revoke(id: string) {
		setError('');
		setBusy(true);
		try {
			await magpieApi(`/api/magpie/links/${encodeURIComponent(id)}/revoke`, {});
			setCreated(undefined);
			await refresh();
		} catch (e) {
			setError(message(e));
		} finally {
			setBusy(false);
		}
	}
	const url = created ? new URL(created.link, location.origin).href : '';
	return (
		<div className="page-stack">
			<PageHeader
				title="People"
				description="Invite colleagues and manage their sign-in access."
				actions={
					<Link className="button small" to="/governance/$page" params={{ page: 'access' }}>
						Manage roles and groups
					</Link>
				}
			/>
			<ErrorMessage error={error || people.error?.message || ''} />
			{people.data && (
				<>
					<Panel>
						<h3>Invite colleague</h3>
						<p>
							Create a single-use invitation valid for 24 hours. Copy the link and share it directly
							with your colleague. Email delivery is not configured.
						</p>
						<form onSubmit={invite} className="magpie-account-form">
							<label htmlFor="invite-name">Colleague’s name</label>
							<input id="invite-name" name="name" required maxLength={100} autoComplete="off" />
							<label htmlFor="invite-email">Email address</label>
							<input
								id="invite-email"
								name="email"
								type="email"
								required
								maxLength={254}
								autoComplete="off"
							/>
							<fieldset>
								<legend>User groups</legend>
								<p>Choose the access they need. Owner groups provide full administration.</p>
								{people.data.groups.map(g => (
									<label className="magpie-check" key={g.id}>
										<input name="group" value={g.id} type="checkbox" />
										{g.name}{' '}
										<span className="muted-copy">({g.roles.join(', ') || 'no roles'})</span>
									</label>
								))}
							</fieldset>
							<button className="button primary" disabled={busy} type="submit">
								Create invitation
							</button>
						</form>
					</Panel>
					{created && (
						<Panel>
							<h3>Share this link with {created.email}</h3>
							<p>Shown only now. Expires {new Date(created.expiresAt).toLocaleString()}.</p>
							<label htmlFor="account-link">Private account link</label>
							<input id="account-link" value={url} readOnly autoComplete="off" />
							<div className="page-actions">
								<button
									className="button"
									type="button"
									onClick={() => {
										void navigator.clipboard
											.writeText(url)
											.then(() => setCopied(true))
											.catch(() => setError('Copy the link from the field above.'));
									}}
								>
									{copied ? 'Copied' : 'Copy link'}
								</button>
								<a
									href={`mailto:${encodeURIComponent(created.email)}?subject=${encodeURIComponent('Your Magpie Gateway account')}&body=${encodeURIComponent(`Use this private link to set your password before ${new Date(created.expiresAt).toLocaleString()}:\n${url}`)}`}
								>
									Open in email app
								</a>
								<button className="button" type="button" onClick={() => setCreated(undefined)}>
									Hide link
								</button>
							</div>
							<p>
								Anyone holding this link can set the account password. Share it only with the named
								person.
							</p>
						</Panel>
					)}
					<Panel>
						<h3>People with access</h3>
						<div className="table-wrap">
							<table>
								<thead>
									<tr>
										<th>Name</th>
										<th>Email</th>
										<th>Sign-in</th>
										<th>Status</th>
										<th>Actions</th>
									</tr>
								</thead>
								<tbody>
									{people.data.users.map(u => (
										<tr key={u.subject}>
											<td>{u.name}</td>
											<td>{u.email}</td>
											<td>{u.local ? 'Password' : 'Single sign-on'}</td>
											<td>{u.disabled ? 'Disabled' : 'Enabled'}</td>
											<td>
												{u.canReset && (
													<button
														className="button"
														type="button"
														disabled={busy || u.disabled}
														onClick={() => void reset(u.subject)}
													>
														Create password reset link
													</button>
												)}{' '}
												<Link
													className="button small"
													to="/governance/$page"
													params={{ page: 'access' }}
												>
													Manage access
												</Link>
											</td>
										</tr>
									))}
								</tbody>
							</table>
						</div>
					</Panel>
					<Panel>
						<h3>Outstanding invitations and resets</h3>
						{people.data.links.length ? (
							<div className="table-wrap">
								<table>
									<thead>
										<tr>
											<th>Person</th>
											<th>Type</th>
											<th>Expires</th>
											<th>Action</th>
										</tr>
									</thead>
									<tbody>
										{people.data.links.map(l => (
											<tr key={l.id}>
												<td>
													{l.name}
													<br />
													{l.email}
												</td>
												<td>{l.kind === 'invite' ? 'Invitation' : 'Password reset'}</td>
												<td>
													{new Date(l.expiresAt).toLocaleString()}
													{Date.parse(l.expiresAt) < Date.now() ? ' (expired)' : ''}
												</td>
												<td>
													<button
														className="button"
														type="button"
														disabled={busy}
														onClick={() => void revoke(l.id)}
													>
														Revoke link
													</button>
												</td>
											</tr>
										))}
									</tbody>
								</table>
							</div>
						) : (
							<p>No outstanding links.</p>
						)}
					</Panel>
				</>
			)}
		</div>
	);
}
function AuthFrame({ children }: { children: ReactNode }) {
	return (
		<main className="magpie-auth">
			<img
				className="magpie-auth-banner"
				src="/assets/magpie/magpie-banner-v1.webp"
				alt="Magpie Gateway. Australian ready."
			/>
			<section className="panel">
				<img
					className="magpie-auth-mark"
					src="/assets/magpie/magpie-emblem-v1.webp"
					alt=""
					width="48"
					height="48"
				/>
				{children}
			</section>
		</main>
	);
}
export function MagpieLoginPage() {
	const [error, setError] = useState('');
	const [busy, setBusy] = useState(false);
	const options = useQuery({
		queryKey: ['magpie-login-options'],
		queryFn: () => magpieApi<{ sso: boolean }>('/api/magpie/login-options')
	});
	async function submit(e: FormEvent<HTMLFormElement>) {
		e.preventDefault();
		setError('');
		setBusy(true);
		try {
			await magpieApi('/api/magpie/login', Object.fromEntries(new FormData(e.currentTarget)));
			location.assign('/');
		} catch (e) {
			setError(message(e));
		} finally {
			setBusy(false);
		}
	}
	return (
		<AuthFrame>
			<h1>Sign in to Magpie Gateway</h1>
			{new URLSearchParams(location.search).has('passwordChanged') && (
				<StatusBanner state="ok" title="Password updated">
					Sign in with your new password.
				</StatusBanner>
			)}
			<ErrorMessage error={error} />
			{options.data?.sso && (
				<p>
					<a className="button" href="/api/auth/start">
						Sign in with your organisation
					</a>
				</p>
			)}
			<form className="magpie-account-form" onSubmit={submit}>
				<label htmlFor="username">Email or username</label>
				<input id="username" name="username" autoComplete="username" required />
				<label htmlFor="password">Password</label>
				<input
					id="password"
					name="password"
					type="password"
					autoComplete="current-password"
					required
				/>
				<button className="button primary" type="submit" disabled={busy}>
					{busy ? 'Signing in…' : 'Sign in'}
				</button>
			</form>
			<p>
				<Link to="/reset-password">Forgot your password?</Link>
			</p>
			<p className="muted-copy">New here? Ask your administrator for an invitation.</p>
		</AuthFrame>
	);
}
export function MagpieAcceptPage() {
	const [token] = useState(() => location.hash.slice(1));
	const [error, setError] = useState('');
	const [busy, setBusy] = useState(false);
	const [done, setDone] = useState(false);
	const invited = location.pathname.endsWith('accept-invitation');
	async function submit(e: FormEvent<HTMLFormElement>) {
		e.preventDefault();
		setError('');
		setBusy(true);
		try {
			await magpieApi('/api/magpie/accept-link', {
				...Object.fromEntries(new FormData(e.currentTarget)),
				token
			});
			history.replaceState(null, '', location.pathname);
			setDone(true);
		} catch (e) {
			setError(message(e));
		} finally {
			setBusy(false);
		}
	}
	return (
		<AuthFrame>
			<h1>{invited ? 'Accept your invitation' : 'Reset your password'}</h1>
			<ErrorMessage error={error} />
			{done ? (
				<>
					<StatusBanner state="ok" title="Password set">
						Your password has been saved. Sign in to continue.
					</StatusBanner>
					<Link to="/login">Go to sign in</Link>
				</>
			) : token ? (
				<>
					<p>Choose your own password. This private link can be used once.</p>
					<form onSubmit={submit} className="magpie-account-form">
						<PasswordFields />
						<button type="submit" className="button primary" disabled={busy}>
							{busy ? 'Saving…' : 'Set password'}
						</button>
					</form>
				</>
			) : (
				<>
					<p>
						Ask an administrator to open <strong>People</strong> and create a password reset link
						for your account. Links expire after one hour.
					</p>
					<p>
						If you use single sign-on, reset your password with your organisation’s identity
						provider.
					</p>
					<p>
						Already signed in? Change your password from <strong>My account</strong>.
					</p>
					<Link to="/login">Back to sign in</Link>
				</>
			)}
		</AuthFrame>
	);
}
