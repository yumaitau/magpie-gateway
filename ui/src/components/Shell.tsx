import { useQueryClient } from '@tanstack/react-query';
import { Link, Outlet, useNavigate, useRouterState } from '@tanstack/react-router';
import {
	BarChart3,
	Bolt,
	Bot,
	Boxes,
	Braces,
	Cable,
	ChevronDown,
	ChevronRight,
	Coins,
	FileCode2,
	GitFork,
	Globe,
	Home,
	KeyRound,
	LogOut,
	Menu,
	MessageSquarePlus,
	Moon,
	Network,
	Play,
	Route,
	ScrollText,
	Server,
	Shield,
	ShieldCheck,
	SlidersHorizontal,
	Sun,
	UserRound
} from 'lucide-react';
import { useEffect, useRef, useState } from 'react';

import { apiBase } from '@/api/base';
import type { RuntimeUser } from '@/api/runtimeApi';
import logoDark from '@/assets/agw-dark.svg';
import logoLight from '@/assets/agw-light.svg';
import { StatusBanner, Tooltip, useDismissiblePopover } from '@/components/Primitives';
import {
	useConfigDumpMode,
	useEffectiveGatewayConfig,
	useMcpConfigData,
	useRuntimeInfo,
	useTrafficConfigData
} from '@/hooks';
import { isMagpie, useMagpieSession } from '@/magpie';

type NavItemConfig = {
	to: string;
	label: string;
	icon: React.ComponentType<{ size?: number }>;
	placeholder?: boolean;
	groupStart?: boolean;
	exact?: boolean;
};

const projectLinks = [
	{
		label: 'GitHub',
		href: 'https://github.com/agentgateway/agentgateway',
		icon: GitFork
	},
	{
		label: 'Documentation',
		href: 'https://agentgateway.dev/docs/standalone/latest/',
		icon: Globe
	},
	{
		label: 'Feedback',
		href: 'https://github.com/agentgateway/agentgateway/issues/new?title=UI%20feedback%3A%20&body=Thanks%20for%20trying%20the%20agentgateway%20UI.%0A%0AWhat%20happened%3F%0A%0AWhat%20did%20you%20expect%20instead%3F%0A%0AAny%20screenshots%2C%20logs%2C%20or%20config%20that%20would%20help%3F',
		icon: MessageSquarePlus
	}
] as const;

export function Shell() {
	const router = useRouterState();
	const magpie = useMagpieSession();
	const canReadGateway = !isMagpie || Boolean(magpie.data?.canReadGateway);
	const runtime = useRuntimeInfo({ enabled: canReadGateway });
	const mode = useConfigDumpMode({ enabled: canReadGateway });
	const queryClient = useQueryClient();
	useEffect(() => {
		const refresh = () => {
			void queryClient.invalidateQueries({ queryKey: ['magpie-session'] });
		};
		window.addEventListener('magpie-settings-changed', refresh);
		return () => window.removeEventListener('magpie-settings-changed', refresh);
	}, [queryClient]);
	const dumpMode = mode.data?.mode === 'dump';
	const config = useEffectiveGatewayConfig({
		enabled: canReadGateway && Boolean(mode.data && mode.data.mode !== 'dump')
	});
	const mcpData = useMcpConfigData({
		enabled: canReadGateway && Boolean(mode.data && mode.data.mode !== 'dump')
	});
	const trafficData = useTrafficConfigData({
		enabled: canReadGateway && Boolean(mode.data && mode.data.mode !== 'dump')
	});
	const [theme, setTheme] = useState(
		() =>
			localStorage.getItem('theme') ??
			(window.matchMedia('(prefers-color-scheme: dark)').matches ? 'dark' : 'light')
	);
	const [mobileNavOpen, setMobileNavOpen] = useState(false);
	const mobileNavRef = useDismissiblePopover<HTMLDivElement>(mobileNavOpen, () =>
		setMobileNavOpen(false)
	);
	const hasLlm = dumpMode ? false : config.data ? Boolean(config.data.llm) : true;
	const hasMcp = dumpMode ? false : mcpData.data ? Boolean(mcpData.data.mcp) : true;
	const hasTraffic = dumpMode
		? true
		: trafficData.data
			? Boolean(trafficData.data.binds?.length) ||
				'gateways' in trafficData.data ||
				'routes' in trafficData.data ||
				'tcpRoutes' in trafficData.data
			: true;
	const hasBinds = dumpMode ? true : config.data ? Boolean(config.data.binds?.length) : false;
	const nativeGroups = navigationGroups({
		hasLlm,
		hasMcp,
		hasTraffic,
		hasBinds,
		dumpMode
	});
	const navGroups: ReadonlyArray<{ title: string; items: readonly NavItemConfig[] }> = isMagpie
		? [
				{
					title: 'Workspace',
					items: [
						{ to: '/account', label: 'My account', icon: UserRound },
						...(magpie.data?.canManagePeople
							? [{ to: '/people', label: 'People', icon: UserRound }]
							: [])
					]
				},
				...(canReadGateway ? nativeGroups : []),
				{
					title: 'Governance',
					items: (magpie.data?.links || []).map(item => ({
						to: `/governance${item.href}`,
						label:
							item.href === '/access'
								? 'Roles and groups'
								: item.href === '/settings'
									? 'Enforcement and settings'
									: item.label,
						icon: ShieldCheck
					}))
				}
			]
		: nativeGroups;
	const currentUser = magpie.data || runtime.data?.user;
	const nav = navGroups.flatMap(group => group.items);
	const matchedNav = nav
		.filter(item => navItemActive(item, router.location.pathname))
		.sort((left, right) => right.to.length - left.to.length)[0];
	const currentNav = matchedNav ?? nav[0];
	const currentGroup = navGroups.find(group => group.items.includes(currentNav));
	const CurrentIcon = currentNav.icon;

	useEffect(() => {
		document.documentElement.dataset.theme = theme;
	}, [theme]);

	// biome-ignore lint/correctness/useExhaustiveDependencies: Existing lint violation; remove this suppression when the underlying issue is fixed.
	useEffect(() => {
		setMobileNavOpen(false);
	}, [router.location.pathname]);

	return (
		<div className="app-shell">
			<aside className="sidebar">
				<Link
					to={canReadGateway ? '/' : '/account'}
					className="brand"
					aria-label="agentgateway home"
				>
					<img className="brand-logo brand-logo-light" src={logoLight} alt="agentgateway" />
					<img className="brand-logo brand-logo-dark" src={logoDark} alt="agentgateway" />
					{runtime.data?.build.version && (
						<small title={runtime.data.build.gitRevision}>{runtime.data.build.version}</small>
					)}
				</Link>
				<nav className="nav-list" aria-label="Primary">
					<Navigation groups={navGroups} currentPath={router.location.pathname} />
				</nav>
				<div className="sidebar-links">
					{projectLinks.map(link => {
						const Icon = link.icon;
						return (
							<Tooltip content={link.label} key={link.href} side="top">
								<a
									className="sidebar-link"
									href={link.href}
									target="_blank"
									rel="noreferrer"
									aria-label={link.label}
								>
									<Icon size={17} />
								</a>
							</Tooltip>
						);
					})}
				</div>
			</aside>
			<div className="main-area">
				<header className="topbar">
					<div className="topbar-left">
						<div className="mobile-nav" ref={mobileNavRef}>
							<button
								className="mobile-nav-trigger"
								type="button"
								aria-expanded={mobileNavOpen}
								onClick={() => setMobileNavOpen(open => !open)}
							>
								<Menu size={17} />
								<CurrentIcon size={16} />
								<span>{currentNav.label}</span>
							</button>
							{mobileNavOpen ? (
								<nav className="mobile-nav-menu" aria-label="Primary">
									<Navigation groups={navGroups} currentPath={router.location.pathname} mobile />
								</nav>
							) : null}
						</div>
						{matchedNav && currentGroup && (
							<nav className="breadcrumb" aria-label="Breadcrumb">
								<span>{currentGroup.title}</span>
								<ChevronRight size={14} />
								<span aria-current="page">{matchedNav.label}</span>
							</nav>
						)}
					</div>
					<div className="topbar-controls">
						{isMagpie &&
							magpie.data &&
							(magpie.data.links.some(item => item.href === '/settings') ? (
								<Link className="magpie-mode" to="/governance/$page" params={{ page: 'settings' }}>
									{magpie.data.mode === 'log-only' ? 'Magpie log-only' : 'Magpie enforcement'}
								</Link>
							) : (
								<span className="magpie-mode">
									{magpie.data.mode === 'log-only' ? 'Magpie log-only' : 'Magpie enforcement'}
								</span>
							))}
						{currentUser && (
							<UserMenu user={currentUser} canManagePeople={magpie.data?.canManagePeople} />
						)}
						<Tooltip content="Toggle theme">
							<button
								className="icon-button"
								type="button"
								aria-label="Toggle theme"
								onClick={() => {
									const next = theme === 'dark' ? 'light' : 'dark';
									localStorage.setItem('theme', next);
									setTheme(next);
								}}
							>
								{theme === 'dark' ? <Sun size={18} /> : <Moon size={18} />}
							</button>
						</Tooltip>
					</div>
				</header>
				<main className="content">
					{runtime.data?.configReload && !runtime.data.configReload.synchronized && (
						<StatusBanner state="warn" title="Configuration reload failed">
							The last configuration reload was rejected, so the gateway is still running the
							previous configuration. Fix the configuration and save it to retry.
						</StatusBanner>
					)}
					{runtime.data?.ui.configStoreMode === 'readOnly' &&
						!router.location.pathname.includes('/governance/') &&
						!['/account', '/people'].includes(router.location.pathname) && (
							<StatusBanner state="info" title="Read-only mode">
								The UI is configured as read-only. Editing is disabled.
							</StatusBanner>
						)}
					<Outlet />
				</main>
			</div>
		</div>
	);
}

function UserMenu({ user, canManagePeople }: { user: RuntimeUser; canManagePeople?: boolean }) {
	const [open, setOpen] = useState(false);
	const trigger = useRef<HTMLButtonElement>(null);
	const ref = useDismissiblePopover<HTMLDivElement>(open, () => {
		setOpen(false);
		trigger.current?.focus();
	});
	const label = user.name || user.email || user.subject || 'Signed in';
	const initials = user.name
		? user.name
				.split(/\s+/)
				.slice(0, 2)
				.map(part => Array.from(part)[0])
				.join('')
				.toLocaleUpperCase()
		: Array.from(user.email || user.subject || '')
				.slice(0, 1)
				.join('')
				.toLocaleUpperCase();

	return (
		<div className="user-menu" ref={ref}>
			<button
				ref={trigger}
				className="user-menu-trigger"
				type="button"
				aria-label={`Account: ${label}`}
				aria-expanded={open}
				aria-controls="user-menu-panel"
				onClick={() => setOpen(!open)}
			>
				<span className="user-avatar" aria-hidden="true">
					{initials || <UserRound size={16} />}
				</span>
				<span className="user-menu-name">{label}</span>
				<ChevronDown size={14} aria-hidden="true" />
			</button>
			{open && (
				<section id="user-menu-panel" className="user-menu-panel" aria-label="Your account">
					<div className="user-menu-identity">
						<span className="user-menu-caption">Signed in as</span>
						<strong>{label}</strong>
						{user.email && user.email !== label && <span>{user.email}</span>}
					</div>
					{isMagpie && (
						<Link className="user-menu-signout" to="/account" onClick={() => setOpen(false)}>
							My account · Change password
						</Link>
					)}
					{isMagpie && canManagePeople && (
						<Link className="user-menu-signout" to="/people" onClick={() => setOpen(false)}>
							Invite colleague
						</Link>
					)}
					{user.canLogout && (
						<form action={`${apiBase}/api/auth/logout`} method="post">
							<button className="user-menu-signout" type="submit">
								<LogOut size={16} aria-hidden="true" />
								Sign out
							</button>
						</form>
					)}
				</section>
			)}
		</div>
	);
}

function navigationGroups(options: {
	hasBinds: boolean;
	hasLlm: boolean;
	hasMcp: boolean;
	hasTraffic: boolean;
	dumpMode: boolean;
}): ReadonlyArray<{ title: string; items: readonly NavItemConfig[] }> {
	const groups: Array<{ title: string; items: readonly NavItemConfig[] }> = [
		{
			title: 'Gateway',
			items: [{ to: '/', label: 'Home', icon: Home }]
		}
	];
	if (!options.dumpMode) {
		groups.push({
			title: 'LLM',
			items: options.hasLlm
				? [
						{ to: '/llm/models', label: 'Models', icon: Bot },
						{ to: '/llm/providers', label: 'Providers', icon: Boxes },

						{
							to: '/llm/policies',
							label: 'Policies',
							icon: Bolt,
							groupStart: true
						},
						{ to: '/llm/guardrails', label: 'Guardrails', icon: Shield },
						{ to: '/llm/keys', label: 'Virtual API Keys', icon: KeyRound },
						{ to: '/llm/costs', label: 'Costs', icon: Coins },

						{
							to: '/llm/analytics',
							label: 'Analytics',
							icon: BarChart3,
							groupStart: true
						},
						{ to: '/llm/logs', label: 'Logs', icon: ScrollText },

						{
							to: '/llm/client-setup',
							label: 'Client Setup',
							icon: Cable,
							groupStart: true
						},
						{ to: '/llm/playground', label: 'Chat Playground', icon: Play }
					]
				: [
						{
							to: '/llm/get-started',
							label: 'Get started',
							icon: Bot,
							placeholder: true
						}
					]
		});
		groups.push({
			title: 'MCP',
			items: options.hasMcp
				? [
						{ to: '/mcp/servers', label: 'Servers', icon: Server },
						{ to: '/mcp/policies', label: 'Policies', icon: ShieldCheck },
						{ to: '/mcp/playground', label: 'Tool Playground', icon: Play }
					]
				: [
						{
							to: '/mcp/get-started',
							label: 'Get started',
							icon: Server,
							placeholder: true
						}
					]
		});
	} else {
		groups.push({
			title: 'LLM',
			items: [
				{ to: '/llm/models', label: 'Models', icon: Bot },
				{ to: '/llm/client-setup', label: 'Client Setup', icon: Cable }
			]
		});
	}
	groups.push({
		title: 'Traffic',
		items: options.dumpMode
			? [
					{ to: '/traffic/listeners', label: 'Listeners', icon: Network },
					{ to: '/traffic/routes', label: 'Routes', icon: Route },
					{ to: '/traffic/policies', label: 'Policies', icon: ShieldCheck }
				]
			: options.hasTraffic
				? [
						{ to: '/traffic/gateways', label: 'Gateways', icon: Network },
						...(options.hasBinds
							? [
									{
										to: '/traffic/listeners',
										label: 'Listeners',
										icon: Network
									}
								]
							: []),
						{ to: '/traffic/routes', label: 'Routes', icon: Route }
					]
				: [
						{
							to: '/traffic/get-started',
							label: 'Get started',
							icon: Network,
							placeholder: true
						}
					]
	});
	groups.push({
		title: 'Tools',
		items: options.dumpMode
			? [{ to: '/cel', label: 'CEL Playground', icon: Braces }]
			: [
					{ to: '/cel', label: 'CEL Playground', icon: Braces },
					{
						to: '/raw-config',
						label: 'Raw Configuration',
						icon: FileCode2,
						exact: true
					},
					{
						to: '/settings',
						label: 'Settings',
						icon: SlidersHorizontal
					}
				]
	});
	return groups;
}

const essentialPaths = new Set([
	'/',
	'/account',
	'/people',
	'/llm/get-started',
	'/llm/models',
	'/llm/keys',
	'/llm/playground',
	'/llm/client-setup',
	'/llm/logs',
	'/settings',
	'/governance/dashboard',
	'/governance/approvals',
	'/governance/audit',
	'/governance/alerts',
	'/governance/controls'
]);

function Navigation(props: {
	groups: ReadonlyArray<{ title: string; items: readonly NavItemConfig[] }>;
	currentPath: string;
	mobile?: boolean;
}) {
	const Section = props.mobile ? MobileNavSection : NavSection;
	const primary = props.groups
		.map(group => ({
			...group,
			items: group.items.filter(item => essentialPaths.has(item.to))
		}))
		.filter(group => group.items.length);
	const advanced = props.groups
		.map(group => ({
			...group,
			items: group.items.filter(item => !essentialPaths.has(item.to))
		}))
		.filter(group => group.items.length);
	const activeAdvanced = advanced.some(group =>
		group.items.some(item => navItemActive(item, props.currentPath))
	);
	return (
		<>
			{primary.map(group => (
				<Section key={group.title} {...group} currentPath={props.currentPath} />
			))}
			{advanced.length > 0 && (
				<details className="advanced-navigation" key={props.currentPath} open={activeAdvanced}>
					<summary>
						<SlidersHorizontal size={16} /> Advanced <ChevronDown size={14} />
					</summary>
					{advanced.map(group => (
						<Section key={group.title} {...group} currentPath={props.currentPath} />
					))}
				</details>
			)}
		</>
	);
}

function NavSection(props: {
	title: string;
	items: readonly NavItemConfig[];
	currentPath: string;
}) {
	return (
		<>
			<div className="nav-section">{props.title}</div>
			{props.items.map(item => (
				<NavItem key={item.to} {...item} currentPath={props.currentPath} />
			))}
		</>
	);
}

function MobileNavSection(props: {
	title: string;
	items: readonly NavItemConfig[];
	currentPath: string;
}) {
	return (
		<>
			<div className="mobile-nav-section">{props.title}</div>
			{props.items.map(item => (
				<MobileNavItem key={item.to} {...item} currentPath={props.currentPath} />
			))}
		</>
	);
}

function MobileNavItem(props: {
	to: string;
	label: string;
	icon: React.ComponentType<{ size?: number }>;
	currentPath: string;
	placeholder?: boolean;
	groupStart?: boolean;
	exact?: boolean;
}) {
	const Icon = props.icon;
	const navigate = useNavigate();
	const active = props.placeholder ? false : navItemActive(props, props.currentPath);
	if (props.placeholder) {
		return (
			<button
				type="button"
				className={props.groupStart ? 'mobile-nav-item nav-group-start' : 'mobile-nav-item'}
				onClick={() => void navigate({ to: props.to })}
			>
				<Icon size={16} />
				<span>{props.label}</span>
			</button>
		);
	}
	return (
		<Link
			to={props.to}
			className={`${active ? 'mobile-nav-item active' : 'mobile-nav-item'}${props.groupStart ? ' nav-group-start' : ''}`}
		>
			<Icon size={16} />
			<span>{props.label}</span>
		</Link>
	);
}

function NavItem(props: {
	to: string;
	label: string;
	icon: React.ComponentType<{ size?: number }>;
	currentPath: string;
	placeholder?: boolean;
	groupStart?: boolean;
	exact?: boolean;
}) {
	const Icon = props.icon;
	const navigate = useNavigate();
	const active = props.placeholder ? false : navItemActive(props, props.currentPath);
	if (props.placeholder) {
		return (
			<button
				type="button"
				className={props.groupStart ? 'nav-item nav-group-start' : 'nav-item'}
				onClick={() => void navigate({ to: props.to })}
			>
				<Icon size={17} />
				<span>{props.label}</span>
			</button>
		);
	}
	return (
		<Link
			to={props.to}
			className={`${active ? 'nav-item active' : 'nav-item'}${props.groupStart ? ' nav-group-start' : ''}`}
		>
			<Icon size={17} />
			<span>{props.label}</span>
		</Link>
	);
}

function navItemActive(item: { to: string; exact?: boolean }, path: string) {
	if (item.to === '/') return path === '/';
	if (item.exact) return path === item.to;
	return path === item.to || path.startsWith(`${item.to}/`);
}
