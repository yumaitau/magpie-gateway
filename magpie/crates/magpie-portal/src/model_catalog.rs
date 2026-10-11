//! Public model metadata only. Discovery never changes routes, credentials or approvals.
use super::*;
use serde::Serialize;
use std::collections::BTreeMap;
use std::time::Instant;

const SOURCE: &str = "https://models.dev/api.json";
const INTERVAL: u64 = 3600;
const MAX_BYTES: usize = 64 * 1024 * 1024;

#[derive(Clone, Default, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
struct Snapshot {
	providers: Vec<CatalogProvider>,
	synced_at: Option<String>,
}
#[derive(Clone, Deserialize, Serialize)]
struct CatalogProvider {
	provider: String,
	name: String,
	models: Vec<String>,
}
#[derive(Default)]
pub(super) struct Catalog {
	snapshot: Mutex<Snapshot>,
	error: Mutex<Option<String>>,
	refresh: tokio::sync::Mutex<Option<Instant>>,
}
impl Catalog {
	pub(super) fn load(store: &Store) -> magpie::Result<Self> {
		let snapshot = store
			.model_catalog_cache()?
			.and_then(|value| serde_json::from_value(value).ok())
			.unwrap_or_default();
		Ok(Self {
			snapshot: Mutex::new(snapshot),
			..Default::default()
		})
	}
}

/// Start once from the executable, so router construction and tests do no external I/O.
pub fn start(state: AppState) {
	if state.inner.config.gateway_admin_url.is_empty() {
		return;
	}
	tokio::spawn(async move {
		loop {
			let ok = match refresh(&state, SOURCE).await {
				Ok(()) => {
					let snapshot = state.inner.model_catalog.snapshot.lock().expect("catalog");
					eprintln!(
						"Model catalog synced: {} providers, {} models.",
						snapshot.providers.len(),
						snapshot
							.providers
							.iter()
							.map(|p| p.models.len())
							.sum::<usize>()
					);
					true
				},
				Err(error) => {
					eprintln!("Model catalog sync: {error}");
					false
				},
			};
			tokio::time::sleep(Duration::from_secs(if ok { INTERVAL } else { 300 })).await;
		}
	});
}

fn response(state: &AppState, session: &SessionRecord) -> Response {
	let catalog = &state.inner.model_catalog;
	let snapshot = catalog.snapshot.lock().expect("catalog").clone();
	let error = catalog.error.lock().expect("catalog error").clone();
	(
		[(header::CACHE_CONTROL, "no-store")],
		Json(json!({
			"providers": snapshot.providers, "syncedAt": snapshot.synced_at,
			"source": SOURCE, "intervalSeconds": INTERVAL, "error": error,
			"canSync": access::allows(state, session, Permission::EditGatewaySettings)
		})),
	)
		.into_response()
}
pub(super) async fn get(State(state): State<AppState>, headers: HeaderMap) -> Response {
	match require(&state, &headers, Permission::ReadGatewayConfig) {
		Ok(session) => response(&state, &session),
		Err(r) => r,
	}
}
pub(super) async fn sync(
	State(state): State<AppState>,
	headers: HeaderMap,
	Json(_): Json<Value>,
) -> Response {
	let session = match require(&state, &headers, Permission::EditGatewaySettings) {
		Ok(s) => s,
		Err(r) => return r,
	};
	if !access::allows(&state, &session, Permission::ReadGatewayConfig) {
		return StatusCode::FORBIDDEN.into_response();
	}
	let _ = refresh(&state, SOURCE).await;
	response(&state, &session)
}

async fn refresh(state: &AppState, url: &str) -> Result<(), String> {
	sync_catalog(
		&state.inner.model_catalog,
		&state.inner.store,
		&state.inner.gateway_client,
		url,
	)
	.await
}
async fn sync_catalog(
	catalog: &Catalog,
	store: &Mutex<Store>,
	client: &reqwest::Client,
	url: &str,
) -> Result<(), String> {
	// Coalesce concurrent syncs and bound manual retries to once per minute.
	let mut last = catalog.refresh.lock().await;
	if last.is_some_and(|at| at.elapsed() < Duration::from_secs(60)) {
		return match catalog.error.lock().expect("catalog error").clone() {
			Some(error) => Err(error),
			None => Ok(()),
		};
	}
	*last = Some(Instant::now());
	let result = async {
		let snapshot = fetch(client, url).await?;
		let value = serde_json::to_value(&snapshot).map_err(|_| "Could not encode catalog.")?;
		store
			.lock()
			.expect("db")
			.set_model_catalog_cache(&value)
			.map_err(|_| "Could not save catalog; retaining the previous snapshot.")?;
		*catalog.snapshot.lock().expect("catalog") = snapshot;
		Ok(())
	}
	.await;
	*catalog.error.lock().expect("catalog error") = result.as_ref().err().cloned();
	result
}
async fn fetch(client: &reqwest::Client, url: &str) -> Result<Snapshot, String> {
	let mut response = client
		.get(url)
		.timeout(Duration::from_secs(30))
		.send()
		.await
		.map_err(|_| "Model source unavailable. The last successful catalog is still in use.")?
		.error_for_status()
		.map_err(|_| "Model source rejected the sync. The last successful catalog is still in use.")?;
	let mut bytes = Vec::new();
	while let Some(chunk) = response
		.chunk()
		.await
		.map_err(|_| "Could not read model source.")?
	{
		if bytes.len() + chunk.len() > MAX_BYTES {
			return Err("Model source exceeded the size limit.".into());
		}
		bytes.extend_from_slice(&chunk);
	}
	parse(&bytes)
}
#[derive(Deserialize)]
struct SourceProvider {
	name: String,
	models: BTreeMap<String, SourceModel>,
}
#[derive(Deserialize)]
struct SourceModel {
	#[serde(default)]
	status: String,
}
fn parse(bytes: &[u8]) -> Result<Snapshot, String> {
	let source: BTreeMap<String, SourceProvider> = serde_json::from_slice(bytes)
		.map_err(|_| "Invalid model source; retaining the previous catalog.")?;
	let providers: Vec<_> = source
		.into_iter()
		.filter_map(|(id, source)| {
			let models: Vec<_> = source
				.models
				.into_iter()
				.filter(|(id, model)| valid_id(id) && model.status != "deprecated")
				.map(|(id, _)| id)
				.collect();
			if !valid_id(&id) || models.is_empty() {
				return None;
			}
			let provider = match id.as_str() {
				"amazon-bedrock" => "aws.bedrock",
				"google" => "gcp.gemini",
				"google-vertex" => "gcp.vertex_ai",
				"github-copilot" => "copilot",
				_ => &id,
			}
			.to_string();
			Some(CatalogProvider {
				provider,
				name: source.name,
				models,
			})
		})
		.collect();
	// Reject empty/error-shaped/partial payloads rather than replacing a good cache.
	if !["anthropic", "openai", "xai", "opencode"]
		.iter()
		.all(|id| providers.iter().any(|p| p.provider == *id))
	{
		return Err("Model source is incomplete; retaining the previous catalog.".into());
	}
	Ok(Snapshot {
		providers,
		synced_at: Some(Utc::now().to_rfc3339()),
	})
}
fn valid_id(id: &str) -> bool {
	!id.is_empty()
		&& id.len() <= 256
		&& id
			.bytes()
			.all(|c| c.is_ascii_alphanumeric() || b"-_.:/@+".contains(&c))
}

#[cfg(test)]
mod tests {
	use super::*;
	fn fixture() -> Value {
		json!({"anthropic":{"name":"Anthropic","models":{"claude-new":{},"old":{"status":"deprecated"}}},
		"openai":{"name":"OpenAI","models":{"codex-new":{}}},"xai":{"name":"xAI","models":{"grok-new":{}}},
		"opencode":{"name":"OpenCode","models":{"model-new":{}}},
		"github-copilot":{"name":"Copilot","models":{"claude.new":{}}}})
	}
	#[test]
	fn preserves_provider_specific_ids_and_filters_deprecated() {
		let snapshot = parse(&serde_json::to_vec(&fixture()).unwrap()).unwrap();
		assert_eq!(
			snapshot
				.providers
				.iter()
				.find(|p| p.provider == "anthropic")
				.unwrap()
				.models,
			["claude-new"]
		);
		assert_eq!(
			snapshot
				.providers
				.iter()
				.find(|p| p.provider == "copilot")
				.unwrap()
				.models,
			["claude.new"]
		);
		assert!(snapshot.synced_at.is_some());
	}
	#[test]
	fn rejects_invalid_and_partial_sources() {
		for bytes in [b"{}".as_slice(), b"null", b"not json"] {
			assert!(parse(bytes).is_err());
		}
		let mut source = fixture();
		source["anthropic"]["models"] = json!({"bad\nmodel":{}});
		assert!(parse(&serde_json::to_vec(&source).unwrap()).is_err());
	}
	#[test]
	fn cached_snapshot_survives_reload() {
		let store = Store::open_memory().unwrap();
		let snapshot = parse(&serde_json::to_vec(&fixture()).unwrap()).unwrap();
		store
			.set_model_catalog_cache(&serde_json::to_value(snapshot).unwrap())
			.unwrap();
		let catalog = Catalog::load(&store).unwrap();
		assert_eq!(catalog.snapshot.lock().unwrap().providers.len(), 5);
	}
	#[tokio::test]
	async fn refresh_persists_coalesces_and_keeps_last_good_on_failure() {
		let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
		let url = format!("http://{}", listener.local_addr().unwrap());
		let app = Router::new()
			.route("/", axum::routing::get(|| async { Json(fixture()) }))
			.route(
				"/bad",
				axum::routing::get(|| async { Json(json!({"error":"temporary failure"})) }),
			);
		let server = tokio::spawn(async move { axum::serve(listener, app).await.unwrap() });
		let store = Mutex::new(Store::open_memory().unwrap());
		let catalog = Catalog::default();
		let client = reqwest::Client::new();
		sync_catalog(&catalog, &store, &client, &url).await.unwrap();
		let original = serde_json::to_value(catalog.snapshot.lock().unwrap().clone()).unwrap();
		assert_eq!(
			original,
			store
				.lock()
				.unwrap()
				.model_catalog_cache()
				.unwrap()
				.unwrap()
		);
		// A concurrent/repeated request reuses the successful result, even if the source is now broken.
		sync_catalog(&catalog, &store, &client, &format!("{url}/bad"))
			.await
			.unwrap();
		*catalog.refresh.lock().await = None;
		assert!(
			sync_catalog(&catalog, &store, &client, &format!("{url}/bad"))
				.await
				.is_err()
		);
		assert!(catalog.error.lock().unwrap().is_some());
		assert_eq!(
			original,
			serde_json::to_value(catalog.snapshot.lock().unwrap().clone()).unwrap()
		);
		assert_eq!(
			original,
			store
				.lock()
				.unwrap()
				.model_catalog_cache()
				.unwrap()
				.unwrap()
		);
		*catalog.refresh.lock().await = None;
		sync_catalog(&catalog, &store, &client, &url).await.unwrap();
		assert!(catalog.error.lock().unwrap().is_none());
		server.abort();
	}
}
