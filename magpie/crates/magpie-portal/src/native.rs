//! Authenticated bridge for the upstream Agentgateway UI and management API.
//! Only explicit management routes are forwarded; admin/debug endpoints stay private.
use axum::body::{Body, to_bytes};
use axum::extract::{Request, State};
use axum::http::{HeaderMap, Method, StatusCode, header};
use axum::response::{IntoResponse, Redirect, Response};
use axum::routing::{any, get};
use axum::{Json, Router};
use magpie::audit::AuditEvent;
use magpie::identity::Permission;
use serde_json::{Value, json};

use crate::{AppState, access, load_settings, session_cookie, session_from};

pub fn routes() -> Router<AppState> {
	Router::new()
		.route("/ui", get(proxy))
		.route("/ui/", get(proxy))
		.route("/ui/{*path}", get(proxy))
		.route("/api/runtime", get(proxy))
		.route("/api/config", any(proxy))
		.route("/api/config/effective", get(proxy))
		.route("/api/config/resources", any(proxy))
		.route("/api/config/resources/{*path}", any(proxy))
		.route("/api/cel", any(proxy))
		.route("/api/logs/{*path}", any(proxy))
		.route("/api/costs/{*path}", any(proxy))
		.route("/api/budgets/status", get(proxy))
		.route("/config_dump", get(proxy))
		.route("/api/magpie/session", get(session))
		.route("/assets/magpie/{*path}", get(asset))
}

pub async fn home(State(state): State<AppState>, headers: HeaderMap) -> Redirect {
	let native = !state.inner.config.gateway_admin_url.is_empty()
		&& session_from(&state, &headers)
			.is_ok_and(|(s, _)| access::allows(&state, &s, Permission::ReadGatewayConfig));
	Redirect::to(if native { "/ui/" } else { "/dashboard" })
}

async fn asset(axum::extract::Path(path): axum::extract::Path<String>) -> Response {
	let (kind, bytes): (&str, &[u8]) = match path.as_str() {
		"brand.css" => ("text/css", include_bytes!("../assets/brand.css")),
		"brand.js" => ("text/javascript", include_bytes!("../assets/brand.js")),
		"mark.svg" => ("image/svg+xml", include_bytes!("../assets/mark.svg")),
		"magpie-portrait-v1.webp" => (
			"image/webp",
			include_bytes!("../assets/magpie-portrait-v1.webp"),
		),
		_ => return StatusCode::NOT_FOUND.into_response(),
	};
	(
		[
			(header::CONTENT_TYPE, kind),
			(header::CACHE_CONTROL, "public, max-age=300"),
		],
		bytes,
	)
		.into_response()
}

async fn session(State(state): State<AppState>, headers: HeaderMap) -> Response {
	let (session, cookie) = match session_from(&state, &headers) {
		Ok(s) => s,
		Err(_) => return StatusCode::UNAUTHORIZED.into_response(),
	};
	let links: Vec<_> = crate::ui::NAV
		.iter()
		.filter(|n| access::allows(&state, &session, access::page_permission(n.path)))
		.map(|n| json!({"href": n.path, "label": n.label}))
		.collect();
	let mut response = Json(json!({
			"name": session.name,
			"roles": access::role_names(&state, &session),
			"mode": load_settings(&state).guardrail_mode.as_str(),
			"canChangeEnforcement": access::allows(&state, &session, Permission::ChangeEnforcement),
			"links": links,
	}))
	.into_response();
	response
		.headers_mut()
		.insert(header::CACHE_CONTROL, "no-store".parse().unwrap());
	response.headers_mut().insert(
		header::SET_COOKIE,
		session_cookie(&cookie, state.inner.config.cookie_secure)
			.parse()
			.unwrap(),
	);
	response
}

/// Exact allowlist, including read-only POST APIs used by logs and CEL.
fn action(method: &Method, path: &str) -> Option<Permission> {
	use Permission::*;
	if method == Method::GET
		&& (path == "/ui"
			|| path.starts_with("/ui/")
			|| matches!(
				path,
				"/api/runtime"
					| "/api/config"
					| "/api/config/effective"
					| "/api/config/resources"
					| "/config_dump"
					| "/api/costs/models"
					| "/api/budgets/status"
			))
	{
		return Some(ReadGatewayConfig);
	}
	if method == Method::POST {
		return match path {
			"/api/config" => Some(ReadGatewayConfig), // Each changed field is authorised below.
			"/api/cel" => Some(RunGatewayCel),
			"/api/logs/search" | "/api/logs/get" | "/api/logs/tail" | "/api/logs/analytics/summary" => {
				Some(ReadGatewayLogs)
			},
			"/api/costs/refresh-base" => Some(EditGatewaySettings),
			_ => None,
		};
	}
	if let Some(resource) = path.strip_prefix("/api/config/resources/") {
		let kind = resource.split('/').next()?;
		let permission = match kind {
			"llm.provider" => EditGatewayProviders,
			"llm.model" | "llm.virtualModel" => EditGatewayModels,
			"llm.apiKey" => EditGatewayKeys,
			"llm.policy" => EditGatewayPolicies,
			"mcp.target" | "mcp.policy" | "mcp.settings" => EditGatewayMcp,
			"traffic.gateway" | "traffic.route" | "traffic.tcpRoute" | "frontend.policy" => {
				EditGatewayTraffic
			},
			"modelCatalog" | "llm.settings" | "ui.policy" => EditGatewaySettings,
			_ => return None,
		};
		return if method == Method::GET {
			Some(ReadGatewayConfig)
		} else if method == Method::PUT || method == Method::DELETE {
			Some(permission)
		} else {
			None
		};
	}
	None
}

fn changed_actions(before: &Value, after: &Value) -> Vec<Permission> {
	use Permission::*;
	fn walk(before: &Value, after: &Value, path: &str, out: &mut Vec<Permission>) {
		if before == after {
			return;
		}
		// Objects can be added/removed as a whole. Walk them too so a new key
		// cannot bypass a more specific permission by replacing its parent.
		if before.is_object() || after.is_object() {
			let keys: std::collections::BTreeSet<_> = before
				.as_object()
				.into_iter()
				.flat_map(|v| v.keys())
				.chain(after.as_object().into_iter().flat_map(|v| v.keys()))
				.collect();
			for key in keys {
				walk(&before[key], &after[key], &format!("{path}/{key}"), out);
			}
			return;
		}
		let permission = if path.starts_with("/llm/policies/apiKey") {
			EditGatewayKeys
		} else if path.starts_with("/llm/models") || path.starts_with("/llm/virtualModels") {
			EditGatewayModels
		} else if path.starts_with("/llm/providers") {
			EditGatewayProviders
		} else if path.starts_with("/llm/policies") {
			EditGatewayPolicies
		} else if path.starts_with("/mcp/") {
			EditGatewayMcp
		} else if ["/binds", "/gateways", "/routes", "/tcpRoutes", "/policies"]
			.iter()
			.any(|p| path == *p || path.starts_with(&format!("{p}/")))
		{
			EditGatewayTraffic
		} else {
			EditGatewaySettings
		};
		if !out.contains(&permission) {
			out.push(permission);
		}
	}
	let mut out = vec![];
	walk(before, after, "", &mut out);
	out
}

async fn proxy(State(state): State<AppState>, request: Request) -> Response {
	let base = state.inner.config.gateway_admin_url.trim_end_matches('/');
	if base.is_empty() {
		return StatusCode::NOT_FOUND.into_response();
	}
	let path = request.uri().path().to_owned();
	let method = request.method().clone();
	let Some(permission) = action(&method, &path) else {
		return StatusCode::NOT_FOUND.into_response();
	};
	let (session, cookie) = match session_from(&state, request.headers()) {
		Ok(s) => s,
		Err(response) => {
			if path.starts_with("/ui") {
				return response;
			}
			return (
				StatusCode::UNAUTHORIZED,
				[(header::LOCATION, "/login")],
				"Sign in to Magpie.",
			)
				.into_response();
		},
	};
	if !access::allows(&state, &session, permission) {
		return (
			StatusCode::FORBIDDEN,
			"Your role cannot perform this gateway action.",
		)
			.into_response();
	}
	if path.starts_with("/ui") && !state.inner.config.gateway_ui_dir.is_empty() {
		return serve_frontend(&state, &path, &cookie);
	}
	if method != Method::GET {
		if let Some(origin) = request
			.headers()
			.get(header::ORIGIN)
			.and_then(|v| v.to_str().ok())
		{
			let expected = &state.inner.config.public_url;
			let valid = if !expected.is_empty() {
				origin.trim_end_matches('/') == expected.trim_end_matches('/')
			} else {
				reqwest::Url::parse(origin).ok().is_some_and(|u| {
					let authority = match u.port() {
						Some(port) => format!("{}:{port}", u.host_str().unwrap_or_default()),
						None => u.host_str().unwrap_or_default().to_string(),
					};
					request
						.headers()
						.get(header::HOST)
						.and_then(|h| h.to_str().ok())
						== Some(authority.as_str())
				})
			};
			if !valid {
				return (
					StatusCode::FORBIDDEN,
					"Cross-origin management requests are not allowed.",
				)
					.into_response();
			}
		}
		if !request
			.headers()
			.get(header::CONTENT_TYPE)
			.and_then(|h| h.to_str().ok())
			.is_some_and(|s| s.starts_with("application/json"))
			&& method != Method::DELETE
		{
			return (StatusCode::UNSUPPORTED_MEDIA_TYPE, "Use application/json.").into_response();
		}
	}
	let uri = request
		.uri()
		.path_and_query()
		.map(|v| v.as_str())
		.unwrap_or(&path)
		.to_owned();
	let body = match to_bytes(request.into_body(), 4 * 1024 * 1024).await {
		Ok(b) => b,
		Err(_) => return StatusCode::PAYLOAD_TOO_LARGE.into_response(),
	};
	// Serialise the read/check/write sequence to prevent stale privilege checks.
	let _write = if method != Method::GET {
		Some(state.inner.gateway_write.lock().await)
	} else {
		None
	};
	if method == Method::POST && path == "/api/config" {
		let after: Value = match serde_json::from_slice::<Value>(&body) {
			Ok(v) if v.is_object() => v,
			_ => return (StatusCode::BAD_REQUEST, "Expected a configuration object.").into_response(),
		};
		let before = match state
			.inner
			.gateway_client
			.get(format!("{base}/api/config"))
			.send()
			.await
		{
			Ok(r) if r.status().is_success() => r.json::<Value>().await.ok(),
			_ => None,
		};
		let Some(before) = before else {
			return StatusCode::BAD_GATEWAY.into_response();
		};
		// The admin socket must remain private behind this authenticated bridge.
		for pointer in [
			"/config/adminAddr",
			"/config/readinessAddr",
			"/config/statsAddr",
		] {
			if before.pointer(pointer) != after.pointer(pointer) {
				return (
					StatusCode::BAD_REQUEST,
					"Management and health listener addresses are managed by the deployment.",
				)
					.into_response();
			}
		}
		for p in changed_actions(&before, &after) {
			if !access::allows(&state, &session, p) {
				return (
					StatusCode::FORBIDDEN,
					format!("Your role lacks {p:?} for this configuration change."),
				)
					.into_response();
			}
		}
	}
	let upstream = match state
		.inner
		.gateway_client
		.request(method.clone(), format!("{base}{uri}"))
		.header(header::CONTENT_TYPE, "application/json")
		.body(body)
		.send()
		.await
	{
		Ok(r) => r,
		Err(_) => {
			return (
				StatusCode::BAD_GATEWAY,
				"The gateway console is temporarily unavailable.",
			)
				.into_response();
		},
	};
	let status = upstream.status();
	let content_type = upstream.headers().get(header::CONTENT_TYPE).cloned();
	let is_html = content_type
		.as_ref()
		.and_then(|v| v.to_str().ok())
		.is_some_and(|v| v.starts_with("text/html"));
	if method != Method::GET
		&& (path == "/api/config"
			|| path.starts_with("/api/config/resources/")
			|| path == "/api/costs/refresh-base")
	{
		let _ = state
			.inner
			.store
			.lock()
			.expect("db")
			.append_audit(&AuditEvent::new(
				"gateway-config",
				&session.subject,
				"",
				"",
				"",
				if status.is_success() { "allow" } else { "deny" },
				"Native gateway management action.",
				json!({"method": method.as_str(), "path": path, "status": status.as_u16()}),
			));
	}
	let response_body = if path.starts_with("/ui") && is_html && status.is_success() {
		let html = match upstream.text().await {
			Ok(s) => s,
			Err(_) => return StatusCode::BAD_GATEWAY.into_response(),
		};
		Body::from(brand_html(&html))
	} else if path == "/api/runtime" && status.is_success() {
		let mut runtime = match upstream.json::<Value>().await {
			Ok(v) => v,
			Err(_) => return StatusCode::BAD_GATEWAY.into_response(),
		};
		runtime["user"] = json!({"subject": session.subject, "name": session.name, "email": session.email, "canLogout": true});
		let can_edit = magpie::rbac::ACTIONS
			.iter()
			.any(|(p, name, _)| name.starts_with("editGateway") && access::allows(&state, &session, *p));
		if !can_edit {
			runtime["ui"]["configStoreMode"] = "readOnly".into();
		}
		Body::from(serde_json::to_vec(&runtime).unwrap())
	} else {
		Body::from_stream(upstream.bytes_stream())
	};
	let mut response = Response::new(response_body);
	*response.status_mut() = status;
	if let Some(ct) = content_type {
		response.headers_mut().insert(header::CONTENT_TYPE, ct);
	}
	response
		.headers_mut()
		.insert(header::CACHE_CONTROL, "no-store".parse().unwrap());
	response.headers_mut().insert(
		header::SET_COOKIE,
		session_cookie(&cookie, state.inner.config.cookie_secure)
			.parse()
			.unwrap(),
	);
	response
}

fn brand_html(html: &str) -> String {
	html.replace("<title>agentgateway</title>", "<title>Magpie Gateway</title>")
        .replace("href=\"/ui/favicon.svg\"", "href=\"/assets/magpie/mark.svg\"")
        .replace("</head>", "<link rel=\"stylesheet\" href=\"/assets/magpie/brand.css\"><script src=\"/assets/magpie/brand.js\" defer></script></head>")
}

fn serve_frontend(state: &AppState, path: &str, cookie: &str) -> Response {
	let relative = path
		.strip_prefix("/ui")
		.unwrap_or("")
		.trim_start_matches('/');
	let relative = if relative.is_empty() || !relative.rsplit('/').next().unwrap_or("").contains('.')
	{
		"index.html"
	} else {
		relative
	};
	if !std::path::Path::new(relative)
		.components()
		.all(|p| matches!(p, std::path::Component::Normal(_)))
		|| relative.contains('\\')
	{
		return StatusCode::NOT_FOUND.into_response();
	}
	let file = std::path::Path::new(&state.inner.config.gateway_ui_dir).join(relative);
	let bytes = match std::fs::read(&file) {
		Ok(b) => b,
		Err(_) => return StatusCode::NOT_FOUND.into_response(),
	};
	let extension = file.extension().and_then(|x| x.to_str()).unwrap_or("");
	let content_type = match extension {
		"html" => "text/html; charset=utf-8",
		"js" => "text/javascript",
		"css" => "text/css",
		"svg" => "image/svg+xml",
		"woff2" => "font/woff2",
		"woff" => "font/woff",
		"json" => "application/json",
		_ => "application/octet-stream",
	};
	let body = if extension == "html" {
		Body::from(brand_html(&String::from_utf8_lossy(&bytes)))
	} else {
		Body::from(bytes)
	};
	let mut response = Response::new(body);
	response
		.headers_mut()
		.insert(header::CONTENT_TYPE, content_type.parse().unwrap());
	response
		.headers_mut()
		.insert(header::CACHE_CONTROL, "no-store".parse().unwrap());
	response.headers_mut().insert(
		header::SET_COOKIE,
		session_cookie(cookie, state.inner.config.cookie_secure)
			.parse()
			.unwrap(),
	);
	response
}

#[cfg(test)]
mod tests {
	use super::*;
	#[test]
	fn changes_require_each_specific_action_even_when_parent_is_added_or_removed() {
		let config = json!({"llm":{"models":[{"name":"m"}],"policies":{"apiKey":{"keys":["secret"]}}},"mcp":{"servers":[1]}});
		for (a, b) in [(json!({}), config.clone()), (config, json!({}))] {
			let actions = changed_actions(&a, &b);
			assert!(actions.contains(&Permission::EditGatewayModels));
			assert!(actions.contains(&Permission::EditGatewayKeys));
			assert!(actions.contains(&Permission::EditGatewayMcp));
		}
		assert_eq!(
			changed_actions(
				&json!({"llm":{"models":[]}}),
				&json!({"llm":{"models":[1]}})
			),
			vec![Permission::EditGatewayModels]
		);
	}
	#[test]
	fn proxy_never_exposes_arbitrary_admin_endpoints_or_unknown_methods() {
		assert_eq!(action(&Method::GET, "/debug/pprof"), None);
		assert_eq!(action(&Method::POST, "/api/logs/delete"), None);
		assert_eq!(action(&Method::DELETE, "/api/config"), None);
		assert_eq!(
			action(&Method::POST, "/api/logs/tail"),
			Some(Permission::ReadGatewayLogs)
		);
	}
}
