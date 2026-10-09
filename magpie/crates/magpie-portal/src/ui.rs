//! Server-rendered portal. No external fonts, scripts or images.

use magpie::identity::Role;

pub struct Viewer {
	pub name: String,
	pub email: String,
	pub roles: Vec<Role>,
	pub break_glass: bool,
}

pub struct Nav {
	pub path: &'static str,
	pub label: &'static str,
}

pub const NAV: &[Nav] = &[
	Nav {
		path: "/dashboard",
		label: "Dashboard",
	},
	Nav {
		path: "/agents",
		label: "Agents and clients",
	},
	Nav {
		path: "/catalogue",
		label: "MCP catalogue",
	},
	Nav {
		path: "/providers",
		label: "Providers and routes",
	},
	Nav {
		path: "/policies",
		label: "Policies",
	},
	Nav {
		path: "/dlp",
		label: "DLP",
	},
	Nav {
		path: "/audit",
		label: "Audit log",
	},
	Nav {
		path: "/approvals",
		label: "Approvals",
	},
	Nav {
		path: "/settings",
		label: "Settings",
	},
];

pub fn layout(title: &str, path: &str, viewer: Option<&Viewer>, demo: bool, body: &str) -> String {
	let nav = NAV
		.iter()
		.map(|item| {
			let current = if item.path == path {
				" aria-current=\"page\""
			} else {
				""
			};
			format!("<a href=\"{}\"{current}>{}</a>", item.path, esc(item.label))
		})
		.collect::<Vec<_>>()
		.join("");
	let user = if let Some(viewer) = viewer {
		let roles = viewer
			.roles
			.iter()
			.map(|role| role.as_str())
			.collect::<Vec<_>>()
			.join(", ");
		format!(
			"<p class=\"who\"><span>{}</span> <span class=\"muted\">{}</span> <span class=\"muted\">{}</span></p>",
			esc(&viewer.name),
			esc(&viewer.email),
			esc(&roles)
		)
	} else {
		String::new()
	};
	let banner = if demo {
		"<p class=\"banner\" role=\"status\">Example data. These records are synthetic and are not a client, an agency or a real approval.</p>"
	} else {
		""
	};
	let glass = viewer
		.filter(|viewer| viewer.break_glass)
		.map(
			|_| "<p class=\"banner warn\" role=\"status\">Break-glass session. This login is audited.</p>",
		)
		.unwrap_or("");
	format!(
		r##"<!DOCTYPE html>
<html lang="en-AU">
<head>
<meta charset="utf-8">
<meta name="viewport" content="width=device-width, initial-scale=1">
<meta name="robots" content="noindex">
<title>{title} · Magpie Gateway</title>
<style>{CSS}</style>
</head>
<body>
<a class="skip" href="#content">Skip to content</a>
<div class="shell">
<aside>
<p class="mark">Magpie Gateway</p>
<p class="muted small">Yuma IT</p>
<nav aria-label="Primary">{nav}</nav>
</aside>
<div class="main">
<header>
{user}
<form method="post" action="/api/auth/logout"><button type="submit">Log out</button></form>
</header>
{banner}
{glass}
<main id="content">
<h1>{title}</h1>
{body}
</main>
</div>
</div>
</body>
</html>"##,
		title = esc(title),
		nav = nav,
		user = user,
		banner = banner,
		glass = glass,
		body = body,
	)
}

pub fn login_page(oidc_label: &str, break_glass_enabled: bool, error: Option<&str>) -> String {
	let error = error
		.map(|text| format!("<p class=\"banner warn\" role=\"alert\">{}</p>", esc(text)))
		.unwrap_or_default();
	let glass = if break_glass_enabled {
		r#"<h2>Break-glass</h2>
<p>This account is off unless an operator has enabled it. Use it only when the identity provider is unavailable. The login is audited.</p>
<form method="post" action="/api/auth/break-glass">
<label for="username">Username</label>
<input id="username" name="username" autocomplete="username" required>
<label for="password">Password</label>
<input id="password" name="password" type="password" autocomplete="current-password" required>
<button type="submit">Sign in with break-glass</button>
</form>"#
	} else {
		"<h2>Break-glass</h2><p>Break-glass login is disabled.</p>"
	};
	format!(
		r##"<!DOCTYPE html>
<html lang="en-AU">
<head>
<meta charset="utf-8">
<meta name="viewport" content="width=device-width, initial-scale=1">
<title>Sign in · Magpie Gateway</title>
<style>{CSS}</style>
</head>
<body>
<a class="skip" href="#content">Skip to content</a>
<main id="content" class="login">
<p class="mark">Magpie Gateway</p>
<h1>Sign in</h1>
{error}
<p>Use your organisation's identity provider. The portal supports Microsoft Entra ID, Okta, Google Workspace and any generic OIDC issuer, with PKCE.</p>
<p><a class="button" href="/api/auth/start">{label}</a></p>
{glass}
<p class="muted small">SAML 2.0 is not a supported login method. Unsigned assertions are rejected.</p>
</main>
</body>
</html>"##,
		label = esc(oidc_label),
		error = error,
		glass = glass,
	)
}

pub fn mock_idp_page(state: &str, action: &str) -> String {
	format!(
		r#"<!DOCTYPE html>
<html lang="en-AU">
<head>
<meta charset="utf-8">
<title>Example identity provider · Magpie Gateway</title>
<style>{CSS}</style>
</head>
<body>
<main id="content" class="login">
<h1>Example identity provider</h1>
<p class="banner" role="status">Example data. This is a local mock, not Entra, Okta or Google.</p>
<form method="post" action="{action}">
<input type="hidden" name="state" value="{state}">
<button type="submit" name="who" value="alex">Continue as Alex Nguyen</button>
<button type="submit" name="who" value="sam">Continue as Sam Patel</button>
</form>
<p class="muted small">Example data. Alex is a policy editor and approver. Sam is an administrator. Neither person is real.</p>
</main>
</body>
</html>"#,
		action = esc(action),
		state = esc(state),
	)
}

pub fn esc(value: &str) -> String {
	value
		.replace('&', "&amp;")
		.replace('<', "&lt;")
		.replace('>', "&gt;")
		.replace('"', "&quot;")
}

const CSS: &str = r#"
:root { color-scheme: light; }
* { box-sizing: border-box; }
body { margin: 0; font: 16px/1.5 "Segoe UI", "Helvetica Neue", sans-serif; color: #1c1915; background: #f4f1ea; }
a { color: #0f3d32; }
a:focus, button:focus, input:focus, select:focus, textarea:focus { outline: 3px solid #0b57d0; outline-offset: 2px; }
.skip { position: absolute; left: -999px; top: 0; background: #fff; padding: 8px; }
.skip:focus { left: 8px; z-index: 2; }
.shell { display: grid; grid-template-columns: 240px 1fr; min-height: 100vh; }
aside { background: #10241e; color: #f4f1ea; padding: 20px 16px; }
aside a { color: #f4f1ea; display: block; padding: 8px 10px; text-decoration: none; border-radius: 6px; }
aside a[aria-current="page"] { background: #1f4d3a; }
aside a:hover { background: #1a3a30; }
.mark { font-weight: 700; margin: 0; letter-spacing: 0.01em; }
.main { padding: 20px 28px 48px; }
header { display: flex; justify-content: flex-end; gap: 16px; align-items: center; }
h1 { font-size: 1.6rem; margin: 0 0 16px; }
h2 { font-size: 1.15rem; margin: 24px 0 8px; }
.muted { color: #3f3a33; }
aside .muted { color: #d7e2dc; }
.small { font-size: 0.9rem; }
.banner { background: #fff7d6; border: 1px solid #8a5a00; padding: 10px 12px; border-radius: 6px; }
.banner.warn { background: #fde8e8; border-color: #8a1c1c; }
table { width: 100%; border-collapse: collapse; background: #fff; }
th, td { text-align: left; padding: 8px 10px; border-bottom: 1px solid #d9d3c7; vertical-align: top; }
th { background: #efeae1; }
.cards { display: grid; grid-template-columns: repeat(auto-fit, minmax(160px, 1fr)); gap: 12px; }
.card { background: #fff; border: 1px solid #d9d3c7; border-radius: 8px; padding: 12px; }
.card strong { display: block; font-size: 1.4rem; }
label { display: block; margin-top: 12px; font-weight: 600; }
input, select, textarea { width: 100%; max-width: 36rem; padding: 8px; border: 1px solid #8a8175; border-radius: 6px; background: #fff; color: #1c1915; }
button, .button { display: inline-block; margin-top: 12px; background: #0f3d32; color: #fff; border: 0; border-radius: 6px; padding: 8px 14px; text-decoration: none; cursor: pointer; }
button.secondary { background: #fff; color: #0f3d32; border: 1px solid #0f3d32; }
pre { background: #fff; border: 1px solid #d9d3c7; padding: 12px; overflow: auto; max-width: 52rem; }
.login { max-width: 36rem; margin: 48px auto; background: #fff; padding: 24px; border-radius: 8px; }
.who { margin: 0; }
@media (max-width: 800px) {
  .shell { grid-template-columns: 1fr; }
  aside { padding-bottom: 8px; }
}
"#;
