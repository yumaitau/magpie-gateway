import { useRouterState } from '@tanstack/react-router';
import DOMPurify from 'dompurify';
import type { FormEvent, MouseEvent } from 'react';
import { useCallback, useEffect, useRef, useState } from 'react';

import { PageHeader, StatusBanner } from '@/components/Primitives';

const pages = new Set([
	'alerts',
	'dashboard',
	'agents',
	'catalogue',
	'providers',
	'policies',
	'dlp',
	'australia',
	'audit',
	'approvals',
	'settings',
	'access'
]);
function nativeMarkup(html: string): string {
	const document = new DOMParser().parseFromString(html, 'text/html');
	// Namespace identifiers to prevent DOM clobbering while keeping labels and form fields intact.
	for (const element of document.body.querySelectorAll('[id],[name]')) {
		for (const attribute of ['id', 'name']) {
			const value = element.getAttribute(attribute);
			if (value) element.setAttribute(attribute, `magpie-field-${value}`);
		}
	}
	for (const element of document.body.querySelectorAll(
		'[for],[aria-describedby],[aria-labelledby]'
	)) {
		for (const attribute of ['for', 'aria-describedby', 'aria-labelledby']) {
			const value = element.getAttribute(attribute);
			if (value)
				element.setAttribute(
					attribute,
					value
						.split(/\s+/)
						.map(id => `magpie-field-${id}`)
						.join(' ')
				);
		}
	}
	for (const button of document.body.querySelectorAll('button')) button.classList.add('button');
	return DOMPurify.sanitize(document.body.innerHTML, {
		ADD_ATTR: ['name', 'method', 'action', 'formaction']
	});
}
type View = { title: string; body: string };
export function MagpieGovernancePage() {
	const router = useRouterState();
	const slug = router.location.pathname.split('/').filter(Boolean).at(-1) || 'dashboard';
	const [view, setView] = useState<View>();
	const [error, setError] = useState('');
	const [busy, setBusy] = useState(false);
	const inFlight = useRef(false);
	const load = useCallback(async (url: string, init?: RequestInit) => {
		setError('');
		setBusy(true);
		try {
			const response = await fetch(url, {
				...init,
				headers: { ...init?.headers, 'X-Magpie-View': '1' },
				credentials: 'same-origin'
			});
			if (new URL(response.url).pathname.includes('/login')) {
				location.assign('/ui/login');
				return;
			}
			if (!response.ok) throw new Error(await response.text());
			if (!(response.headers.get('content-type') || '').includes('application/json'))
				throw new Error('Unexpected response. Reload and try again.');
			const result = await response.json();
			setView({
				title: result.title,
				body: nativeMarkup(result.body)
			});
			window.dispatchEvent(new Event('magpie-settings-changed'));
		} catch (e) {
			setError(e instanceof Error ? e.message : 'Request failed');
		} finally {
			setBusy(false);
			inFlight.current = false;
		}
	}, []);
	useEffect(() => {
		setView(undefined);
		if (pages.has(slug)) void load(`/${slug}${window.location.search}`);
		else setError('Page not found.');
	}, [slug, load]);
	function submit(event: FormEvent<HTMLDivElement>) {
		const form = event.target;
		if (!(form instanceof HTMLFormElement)) return;
		event.preventDefault();
		if (inFlight.current) return;
		const submitter = (event.nativeEvent as SubmitEvent).submitter;
		const action = new URL(form.getAttribute('action') || `/${slug}`, location.origin);
		if (action.origin !== location.origin) {
			setError('External form submissions are refused.');
			return;
		}
		const data = new FormData(form, submitter instanceof HTMLButtonElement ? submitter : undefined);
		const params = new URLSearchParams();
		for (const [key, value] of data) {
			if (typeof value === 'string') params.append(key.replace(/^magpie-field-/, ''), value);
		}
		inFlight.current = true;
		if (form.method.toLowerCase() === 'get') {
			action.search = params.toString();
			void load(action.href);
		} else {
			void load(action.href, { method: 'POST', body: params });
		}
	}
	function click(event: MouseEvent<HTMLDivElement>) {
		const target = event.target;
		if (!(target instanceof Element)) return;
		const anchor = target.closest('a');
		if (!anchor || event.ctrlKey || event.metaKey || event.shiftKey || event.altKey) return;
		const url = new URL(anchor.href, location.origin);
		if (url.origin !== location.origin) return;
		if (pages.has(url.pathname.slice(1))) {
			event.preventDefault();
			location.assign(`/ui/governance${url.pathname}${url.search}${url.hash}`);
		}
	}
	return (
		<div className="page-stack">
			<PageHeader title={view?.title || 'Governance'} />
			{error && (
				<StatusBanner state="bad" title="Could not complete the request">
					{error}
				</StatusBanner>
			)}
			{!view && busy && <p role="status">Loading…</p>}
			{/* Server-generated, permission-checked forms are sanitised and use the native shell and tokens. */}
			{/* biome-ignore lint/a11y/useKeyWithClickEvents lint/a11y/noStaticElementInteractions: Delegates existing native anchors, including keyboard activation. */}
			<div
				className="magpie-governance"
				aria-busy={busy}
				onSubmit={submit}
				onClick={click}
				// biome-ignore lint/security/noDangerouslySetInnerHtml: Only permission-checked server markup sanitised with DOMPurify.
				dangerouslySetInnerHTML={{ __html: view?.body || '' }}
			/>
		</div>
	);
}
