//! Server-rendered portal. No external fonts, scripts or images.

use magpie::identity::Permission;

pub struct Viewer {
	pub name: String,
	pub email: String,
	pub roles: Vec<String>,
	pub permissions: Vec<Permission>,
	pub break_glass: bool,
}

pub struct Nav {
	pub path: &'static str,
	pub label: &'static str,
}

pub const NAV: &[Nav] = &[
	Nav {
		path: "/australia",
		label: "Australian assurance",
	},
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
		label: "Australian privacy",
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
		path: "/access",
		label: "Access control",
	},
	Nav {
		path: "/settings",
		label: "Settings",
	},
];

pub fn layout(title: &str, path: &str, viewer: Option<&Viewer>, demo: bool, body: &str) -> String {
	let console = if viewer.is_some_and(|v| v.permissions.contains(&Permission::ReadGatewayConfig)) {
		r#"<a class="console-link" href="/ui/">← Gateway console</a>"#
	} else {
		""
	};
	let nav = NAV
		.iter()
		.filter(|item| {
			viewer.is_some_and(|v| {
				v.permissions
					.contains(&crate::access::page_permission(item.path))
			})
		})
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
			.map(String::as_str)
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
<link rel="icon" href="/assets/magpie/magpie-emblem-v1.webp">
<style>{CSS}</style>
</head>
<body>
<a class="skip" href="#content">Skip to content</a>
<div class="shell">
<aside>
<p class="mark"><img src="/assets/magpie/magpie-emblem-v1.webp" width="38" height="38" alt=""> <span>Magpie<small>GATEWAY</small></span></p>
<p class="muted small">Yuma IT</p>
{console}<nav aria-label="Primary">{nav}</nav>
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

pub fn login_page(oidc_enabled: bool, break_glass_enabled: bool, error: Option<&str>) -> String {
	let error = error
		.map(|text| format!("<p class=\"banner warn\" role=\"alert\">{}</p>", esc(text)))
		.unwrap_or_default();
	let sso = if oidc_enabled {
		r#"<p>Use your organisation's identity provider.</p>
<p><a class="button" href="/api/auth/start">Sign in with single sign-on</a></p>"#
	} else {
		""
	};
	let glass = match (break_glass_enabled, oidc_enabled) {
		(true, sso) => format!(
			r#"<h2>{heading}</h2>
<p>{text} The login is audited.</p>
<form method="post" action="/api/auth/break-glass">
<label for="username">Username</label>
<input id="username" name="username" autocomplete="username" required>
<label for="password">Password</label>
<input id="password" name="password" type="password" autocomplete="current-password" required>
<button type="submit">Sign in as administrator</button>
</form>"#,
			heading = if sso { "Break-glass" } else { "Administrator" },
			text = if sso {
				"Use this only when the identity provider is unavailable."
			} else {
				"Use the administrator password from setup."
			},
		),
		(false, _) => "<h2>Break-glass</h2><p>Break-glass login is disabled.</p>".to_string(),
	};
	format!(
		r##"<!DOCTYPE html>
<html lang="en-AU">
<head>
<meta charset="utf-8">
<meta name="viewport" content="width=device-width, initial-scale=1">
<title>Sign in · Magpie Gateway</title>
<link rel="icon" href="/assets/magpie/magpie-emblem-v1.webp">
<style>{CSS}</style>
</head>
<body class="login-body">
<a class="skip" href="#content">Skip to content</a>
<div class="login-scene">
<section class="login-art" aria-label="Australian magpie">
<img src="/assets/magpie/magpie-portrait-v1.webp" alt="Australian magpie, its black plumage and white nape framed by eucalyptus leaves.">
<div class="login-caption"><span>AUSTRALIAN AI GATEWAY</span><h2>A watchful eye.<br>On every connection.</h2><p>Models, agents and tools. Connected with confidence.</p></div>
</section>
<main id="content" class="login">
<p class="mark"><img src="/assets/magpie/magpie-emblem-v1.webp" width="38" height="38" alt=""> <span>Magpie<small>GATEWAY</small></span></p>
<h1>Welcome back.</h1><p class="login-intro">Sign in to your Magpie Gateway.</p>
{error}
{sso}
{glass}
<p class="login-footer">Powered by Agentgateway · Made yours by Yuma IT</p>
</main>
</div>
</body>
</html>"##,
		error = error,
		sso = sso,
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
<link rel="icon" href="/assets/magpie/magpie-emblem-v1.webp">
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
body { margin: 0; font: 16px/1.5 "Segoe UI", "Helvetica Neue", sans-serif; color: #252a2c; background: #f7f8f7; }
a { color: #844332; }
a:focus, button:focus, input:focus, select:focus, textarea:focus { outline: 3px solid #0b57d0; outline-offset: 2px; }
.skip { position: absolute; left: -999px; top: 0; background: #fff; padding: 8px; }
.skip:focus { left: 8px; z-index: 2; }
.shell { display: grid; grid-template-columns: 240px 1fr; min-height: 100vh; }
aside { background: #22282a; color: #f7f8f7; padding: 20px 16px; }
aside a { color: #f7f8f7; display: block; padding: 8px 10px; text-decoration: none; border-radius: 6px; }
aside a[aria-current="page"] { background: #42494b; }
aside a:hover { background: #343c3e; }
.mark { font-weight: 700; margin: 0; letter-spacing: 0.01em; }
.main { padding: 20px 28px 48px; }
header { display: flex; justify-content: flex-end; gap: 16px; align-items: center; }
h1 { font-size: 1.6rem; margin: 0 0 16px; }
h2 { font-size: 1.15rem; margin: 24px 0 8px; }
.muted { color: #515b5e; }
aside .muted { color: #cdd5d5; }
.small { font-size: 0.9rem; }
.banner { background: #fff7d6; border: 1px solid #8a5a00; padding: 10px 12px; border-radius: 6px; }
.banner.warn { background: #fde8e8; border-color: #8a1c1c; }
table { width: 100%; border-collapse: collapse; background: #fff; }
th, td { text-align: left; padding: 8px 10px; border-bottom: 1px solid #dce1e0; vertical-align: top; }
th { background: #f0f3f2; }
.cards { display: grid; grid-template-columns: repeat(auto-fit, minmax(160px, 1fr)); gap: 12px; }
.card { background: #fff; border: 1px solid #dce1e0; border-radius: 8px; padding: 12px; }
.card strong { display: block; font-size: 1.4rem; }
label { display: block; margin-top: 12px; font-weight: 600; }
input, select, textarea { width: 100%; max-width: 36rem; padding: 8px; border: 1px solid #879391; border-radius: 6px; background: #fff; color: #252a2c; }
button, .button { display: inline-block; margin-top: 12px; background: #844332; color: #fff; border: 0; border-radius: 6px; padding: 8px 14px; text-decoration: none; cursor: pointer; }
button.secondary { background: #fff; color: #844332; border: 1px solid #844332; }
pre { background: #fff; border: 1px solid #dce1e0; padding: 12px; white-space: pre-wrap; max-width: 52rem; }
.login { max-width: 36rem; margin: 48px auto; background: #fff; padding: 24px; border-radius: 8px; }
.who { margin: 0; }

.choices { display: grid; grid-template-columns: repeat(auto-fit, minmax(250px, 1fr)); gap: .5rem; } .choice { display: flex; align-items: center; gap: .5rem; } .choice input { width: auto; } details { border-bottom: 1px solid #d1d5db; padding: 1rem 0; } summary { cursor: pointer; font-weight: 600; } fieldset { margin: 1rem 0; }
button:disabled { cursor: not-allowed; opacity: .6; }
@media (max-width: 800px) {
  .shell { grid-template-columns: 1fr; }
  aside { padding-bottom: 8px; }
}
.mark { display: flex; align-items: center; gap: 11px; font-size: 24px; letter-spacing: -.8px; line-height: 1.15; }
.mark small { display: block; font-size: 9px; letter-spacing: 2.4px; margin-top: 4px; }
.mark img { border-radius: 8px; }
.magpie-dashboard-banner { display: block; width: 100%; height: auto; border-radius: 10px; margin-bottom: 24px; }
.console-link { border: 1px solid #60696b; margin: 23px 0 16px; font-size: 14px; }
.main { min-width: 0; }
header { padding-bottom: 24px; }
.who { font-size: 13px; }
.who span { display: inline-block; margin-right: 7px; }
main h1 { letter-spacing: -.6px; font-size: 28px; }
table { font-size: 14px; }
th { font-size: 12px; font-weight: 650; color: #515b5e; }
.table-scroll { max-width: 100%; overflow: auto; }
.banner.warn { background: #fff4e5; border-color: #a57636; color: #644513; }
.login-body { background: #22282a; }
.login-scene { display: grid; grid-template-columns: 1.05fr 1fr; min-height: 100svh; max-width: 1800px; margin: auto; }
.login-art { position: relative; min-height: 720px; overflow: hidden; background: #22282a; }
.login-art > img { width: 100%; height: 100%; position: absolute; object-fit: cover; object-position: center 58%; }
.login-caption { position: absolute; top: 8%; left: 10%; right: 8%; color: #f1f3ed; }
.login-caption > span { font-size: 11px; letter-spacing: 2px; font-weight: 600; color: #cad5cf; }
.login-caption h2 { font-size: clamp(32px, 3.2vw, 49px); font-weight: 500; letter-spacing: -1.5px; line-height: 1.12; margin: 20px 0 14px; }
.login-caption p { font-size: 14px; max-width: 290px; color: #cad5cf; }
.login { max-width: none; width: 100%; margin: 0; padding: clamp(32px, 7vw, 115px); border-radius: 0; background: #f8f9f7; display: flex; flex-direction: column; justify-content: center; }
.login .mark { margin-bottom: 54px; }
.login h1 { font-size: 38px; font-weight: 600; margin: 0 0 9px; }
.login-intro { color: #515b5e; margin: 0 0 26px; }
.login h2 { font-size: 15px; margin: 8px 0 0; }
.login h2 + p { font-size: 13px; color: #515b5e; margin: 5px 0 8px; }
.login form { display: grid; gap: 5px; }
.login label { font-size: 13px; margin-top: 13px; }
.login input { max-width: none; padding: 12px; background: #fdfefc; }
.login button { min-height: 45px; margin-top: 20px; font-weight: 600; }
.login-footer { margin-top: 45px; color: #596362; font-size: 11px; }
@media(max-width: 800px) {
 .login-scene { grid-template-columns: 1fr; }
 .login-art { min-height: 260px; }
 .login-art > img { object-position: 75% 40%; }
 .login-caption { top: 25px; left: 24px; max-width: 240px; }
 .login-caption h2 { font-size: 30px; }
 .login-caption p { display: none; }
 .login { padding: 32px 24px; }
 .login .mark { margin-bottom: 30px; }
 .login h1 { font-size: 30px; }
 .choices { grid-template-columns: 1fr; }
 .main { padding: 20px 16px; }
 header { flex-wrap: wrap; }
}
"#;
