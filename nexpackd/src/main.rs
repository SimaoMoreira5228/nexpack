use nexpack_core::Bundle;
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::net::{UnixListener, UnixStream};
use tokio::sync::RwLock;

mod mount;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
	tracing_subscriber::fmt()
		.with_env_filter(tracing_subscriber::EnvFilter::try_from_default_env().unwrap_or_else(|_| "nexpackd=info".into()))
		.init();

	let runtime_dir = std::env::var("XDG_RUNTIME_DIR").unwrap_or_else(|_| "/tmp".into());
	let socket_path = PathBuf::from(&runtime_dir).join("nexpack.sock");

	let _ = std::fs::remove_file(&socket_path);

	let state = Arc::new(RwLock::new(DaemonState {
		mounts: HashMap::new(),
		store: nexpack_core::Store::user().ok(),
	}));

	let listener = UnixListener::bind(&socket_path)?;
	tracing::info!("nexpackd listening on {}", socket_path.display());

	loop {
		let (stream, _addr) = listener.accept().await?;
		let state = state.clone();
		tokio::spawn(async move {
			if let Err(e) = handle_client(stream, state).await {
				tracing::error!("client error: {}", e);
			}
		});
	}
}

struct DaemonState {
	mounts: HashMap<String, MountEntry>,
	store: Option<nexpack_core::Store>,
}

struct MountEntry {
	app_id: String,
	rootfs: PathBuf,
	refcount: u64,
}

async fn handle_client(mut stream: UnixStream, state: Arc<RwLock<DaemonState>>) -> anyhow::Result<()> {
	let (reader, mut writer) = stream.split();
	let mut buf_reader = BufReader::new(reader);
	let mut line = String::new();
	buf_reader.read_line(&mut line).await?;
	let line = line.trim();

	let request: serde_json::Value = serde_json::from_str(line)?;
	let method = request.get("method").and_then(|v| v.as_str()).unwrap_or("");

	let response = match method {
		"mount" => handle_mount(request, &state).await,
		"unmount" => handle_unmount(request, &state).await,
		"list" => handle_list(&state).await,
		"status" => Ok(serde_json::json!({"status": "ok", "version": "0.1.0"})),
		_ => Ok(serde_json::json!({"error": format!("unknown method: {}", method)})),
	};

	let mut resp = serde_json::to_string(&response?)?;
	resp.push('\n');
	writer.write_all(resp.as_bytes()).await?;

	Ok(())
}

async fn handle_mount(request: serde_json::Value, state: &Arc<RwLock<DaemonState>>) -> anyhow::Result<serde_json::Value> {
	let bundle_path = request
		.get("bundle")
		.and_then(|v| v.as_str())
		.ok_or_else(|| anyhow::anyhow!("missing 'bundle' field"))?;

	let bundle = Bundle::open(bundle_path)?;
	let app_id = bundle.header.app_id.clone();

	nexpack_core::Verifier::verify_layers(&bundle)?;

	let store_dir = nexpack_core::Store::user()?;
	for (i, layer) in bundle.header.layers.iter().enumerate() {
		let digest_hex = layer.digest_hex();
		if !store_dir.has_layer(digest_hex) {
			let data = bundle.extract_layer(i)?;
			store_dir.store_layer(data, digest_hex)?;
		}
	}

	let merged = mount::build_overlay(&app_id, &bundle, &store_dir)?;
	let entrypoint = bundle.header.entrypoint.clone();

	let mut state = state.write().await;
	let entry = state.mounts.entry(app_id.clone()).or_insert(MountEntry {
		app_id: app_id.clone(),
		rootfs: merged.clone(),
		refcount: 0,
	});
	entry.refcount += 1;

	Ok(serde_json::json!({
		"status": "mounted",
		"app_id": app_id,
		"rootfs": merged.to_string_lossy(),
		"entrypoint": entrypoint,
	}))
}

async fn handle_unmount(request: serde_json::Value, state: &Arc<RwLock<DaemonState>>) -> anyhow::Result<serde_json::Value> {
	let app_id = request
		.get("app_id")
		.and_then(|v| v.as_str())
		.ok_or_else(|| anyhow::anyhow!("missing 'app_id' field"))?;

	let mut state = state.write().await;
	if let Some(entry) = state.mounts.get_mut(app_id) {
		entry.refcount = entry.refcount.saturating_sub(1);
		if entry.refcount == 0 {
			mount::unmount_overlay(&entry.rootfs)?;
			state.mounts.remove(app_id);
		}
		Ok(serde_json::json!({"status": "unmounted", "app_id": app_id}))
	} else {
		Ok(serde_json::json!({"error": "not mounted", "app_id": app_id}))
	}
}

async fn handle_list(state: &Arc<RwLock<DaemonState>>) -> anyhow::Result<serde_json::Value> {
	let state = state.read().await;
	let mounts: Vec<serde_json::Value> = state
		.mounts
		.iter()
		.map(|(id, entry)| {
			serde_json::json!({
				"app_id": id,
				"rootfs": entry.rootfs.to_string_lossy(),
				"refcount": entry.refcount,
			})
		})
		.collect();
	Ok(serde_json::json!({"mounts": mounts}))
}
