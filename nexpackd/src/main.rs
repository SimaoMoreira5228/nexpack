use nexpack_core::Bundle;
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

mod mount;
mod sandbox;

fn main() -> anyhow::Result<()> {
	tracing_subscriber::fmt()
		.with_env_filter(tracing_subscriber::EnvFilter::try_from_default_env().unwrap_or_else(|_| "nexpackd=info".into()))
		.init();

	let config = load_config();
	let idle_timeout = Duration::from_secs(config.idle_timeout.unwrap_or(300));
	let update_interval = Duration::from_secs(config.update_interval.unwrap_or(3600));

	let runtime_dir = std::env::var("XDG_RUNTIME_DIR").unwrap_or_else(|_| "/tmp".into());
	let socket_path = PathBuf::from(&runtime_dir).join("nexpack.sock");
	let _ = std::fs::remove_file(&socket_path);

	let state = Arc::new(Mutex::new(DaemonState {
		mounts: HashMap::new(),
		store: nexpack_core::Store::user().ok(),
		idle_timeout,
	}));

	let checker_state = state.clone();
	std::thread::spawn(move || background_update_checker(checker_state, update_interval));

	let idle_state = state.clone();
	std::thread::spawn(move || idle_mount_cleaner(idle_state));

	let listener = std::os::unix::net::UnixListener::bind(&socket_path)?;
	tracing::info!("nexpackd listening on {}", socket_path.display());

	for stream in listener.incoming() {
		let stream = stream?;
		let state = state.clone();
		std::thread::spawn(move || {
			if let Err(e) = handle_client(stream, state) {
				tracing::error!("client error: {}", e);
			}
		});
	}

	Ok(())
}

#[derive(serde::Deserialize)]
struct DaemonConfig {
	idle_timeout: Option<u64>,
	update_interval: Option<u64>,
}

fn load_config() -> DaemonConfig {
	let config_dir = std::env::var("XDG_CONFIG_HOME")
		.map(PathBuf::from)
		.unwrap_or_else(|_| {
			std::env::var("HOME")
				.map(|h| PathBuf::from(h).join(".config"))
				.unwrap_or_default()
		})
		.join("nexpack");
	let config_path = config_dir.join("daemon.toml");

	match std::fs::read_to_string(&config_path) {
		Ok(content) => toml::from_str(&content).unwrap_or_else(|e| {
			tracing::warn!("invalid daemon.toml: {}; using defaults", e);
			DaemonConfig {
				idle_timeout: None,
				update_interval: None,
			}
		}),
		Err(_) => DaemonConfig {
			idle_timeout: None,
			update_interval: None,
		},
	}
}

struct DaemonState {
	mounts: HashMap<String, MountEntry>,
	store: Option<nexpack_core::Store>,
	idle_timeout: Duration,
}

struct MountEntry {
	app_id: String,
	rootfs: PathBuf,
	refcount: u64,
	last_active: Instant,
}

fn background_update_checker(_state: Arc<Mutex<DaemonState>>, interval: Duration) {
	loop {
		std::thread::sleep(interval);

		let store = match nexpack_core::Store::user() {
			Ok(s) => s,
			Err(_) => continue,
		};

		let apps = match store.list_apps() {
			Ok(a) => a,
			Err(_) => continue,
		};

		for app_id in &apps {
			let app_dir = store.apps_dir().join(app_id);
			let meta_path = app_dir.join("meta.capnp");

			let meta = match std::fs::read(&meta_path) {
				Ok(m) => m,
				Err(_) => continue,
			};

			let header: nexpack_core::BundleHeader = match nexpack_core::BundleHeader::parse(&meta) {
				Ok(h) => h,
				Err(_) => continue,
			};

			let update_url = match &header.update_url {
				Some(url) => url.clone(),
				None => continue,
			};

			tracing::debug!("checking updates for {} from {}", app_id, update_url);

			let feed: nexpack_core::update::UpdateFeed = match ureq::get(&update_url).call() {
				Ok(r) if r.status() == 200 => match r.into_json() {
					Ok(f) => f,
					Err(_) => continue,
				},
				_ => continue,
			};

			if feed.latest == header.app_version {
				tracing::debug!("{} is up to date (v{})", app_id, feed.latest);
				continue;
			}

			let release = match feed.latest_release() {
				Some(r) => r,
				None => continue,
			};

			tracing::info!("update available for {}: v{} -> v{}", app_id, header.app_version, feed.latest);

			let missing = release.missing_layers(&store);
			if missing.is_empty() {
				tracing::debug!("{}: all layers already in store", app_id);
			} else {
				for layer in &missing {
					let hex = layer.digest.strip_prefix("blake3:").unwrap_or(&layer.digest);
					tracing::info!("pre-fetching layer {} for {}", &hex[..16], app_id);

					match ureq::get(&layer.url).call() {
						Ok(r) if r.status() == 200 => {
							let mut data = Vec::new();
							if r.into_reader().read_to_end(&mut data).is_ok() {
								if let Err(e) = store.store_layer(&data, hex) {
									tracing::warn!("failed to store layer {}: {}", hex, e);
								}
							}
						}
						_ => tracing::warn!("HTTP error fetching layer {}", hex),
					}
				}
			}

			let new_digest = release
				.layers
				.last()
				.map(|l| l.digest.strip_prefix("blake3:").unwrap_or(&l.digest))
				.unwrap_or("");

			if !new_digest.is_empty() && store.has_layer(new_digest) {
				let current_link = app_dir.join("current");
				let tmp_link = app_dir.join("current.tmp");
				let target = format!("../../../layers/blake3-{}/image.erofs", new_digest);
				let _ = std::fs::remove_file(&tmp_link);
				if let Ok(()) = std::os::unix::fs::symlink(&target, &tmp_link) {
					let _ = std::fs::rename(&tmp_link, &current_link);
				}
				let _ = store.add_gc_root(app_id, new_digest);
			}

			let mut new_header = header.clone();
			new_header.app_version = release.version.clone();
			new_header.layers.clear();
			for l in &release.layers {
				new_header.layers.push(nexpack_core::LayerRef {
					digest: l.digest.clone(),
					size: l.size,
					role: String::new(),
				});
			}
			if let Ok(new_meta) = new_header.encode() {
				let _ = std::fs::write(&meta_path, &new_meta);
			}

			tracing::info!("{} updated to v{} in background", app_id, feed.latest);
		}
	}
}

fn idle_mount_cleaner(state: Arc<Mutex<DaemonState>>) {
	let check_interval = Duration::from_secs(60);
	loop {
		std::thread::sleep(check_interval);

		let to_remove: Vec<String> = {
			let s = state.lock().unwrap();
			s.mounts
				.iter()
				.filter(|(_, e)| e.refcount == 0 && e.last_active.elapsed() >= s.idle_timeout)
				.map(|(id, _)| id.clone())
				.collect()
		};

		for app_id in to_remove {
			let mut s = state.lock().unwrap();
			if let Some(entry) = s.mounts.get(&app_id) {
				if entry.refcount == 0 && entry.last_active.elapsed() >= s.idle_timeout {
					tracing::info!("idle timeout: unmounting {}", app_id);
					if let Err(e) = mount::unmount_overlay(&entry.rootfs) {
						tracing::warn!("failed to unmount idle {}: {}", app_id, e);
						continue;
					}
					s.mounts.remove(&app_id);
				}
			}
		}
	}
}

fn handle_client(stream: std::os::unix::net::UnixStream, state: Arc<Mutex<DaemonState>>) -> anyhow::Result<()> {
	let mut reader = std::io::BufReader::new(&stream);
	let mut writer = &stream;

	let request_reader = capnp::serialize::read_message(&mut reader, capnp::message::ReaderOptions::default())?;
	let request = request_reader.get_root::<nexpack_ipc::ipc_capnp::request::Reader>()?;

	enum RequestKind {
		Mount {
			bundle: String,
			offline: bool,
		},
		Unmount {
			app_id: String,
		},
		List,
		Status,
	}

	let kind = match request.which()? {
		nexpack_ipc::ipc_capnp::request::Mount(m) => {
			let m = m?;
			RequestKind::Mount {
				bundle: m.get_bundle()?.to_str()?.to_string(),
				offline: m.get_offline(),
			}
		}
		nexpack_ipc::ipc_capnp::request::Unmount(u) => {
			let u = u?;
			RequestKind::Unmount {
				app_id: u.get_app_id()?.to_str()?.to_string(),
			}
		}
		nexpack_ipc::ipc_capnp::request::List(_) => RequestKind::List,
		nexpack_ipc::ipc_capnp::request::Status(_) => RequestKind::Status,
	};

	let write_error = |e: &anyhow::Error, w: &mut &std::os::unix::net::UnixStream| -> anyhow::Result<()> {
		let mut msg = capnp::message::Builder::new_default();
		{
			let mut resp = msg.init_root::<nexpack_ipc::ipc_capnp::response::Builder>();
			let mut err = resp.init_error();
			err.set_message(&e.to_string());
		}
		capnp::serialize::write_message(w, &msg)?;
		Ok(())
	};

	match kind {
		RequestKind::Mount { bundle, offline } => match handle_mount(&bundle, offline, &state) {
			Ok(msg) => capnp::serialize::write_message(&mut writer, &msg)?,
			Err(e) => write_error(&e, &mut writer)?,
		},
		RequestKind::Unmount { app_id } => match handle_unmount(&app_id, &state) {
			Ok(msg) => capnp::serialize::write_message(&mut writer, &msg)?,
			Err(e) => write_error(&e, &mut writer)?,
		},
		RequestKind::List => match handle_list(&state) {
			Ok(msg) => capnp::serialize::write_message(&mut writer, &msg)?,
			Err(e) => write_error(&e, &mut writer)?,
		},
		RequestKind::Status => {
			let mut msg = capnp::message::Builder::new_default();
			{
				let mut resp = msg.init_root::<nexpack_ipc::ipc_capnp::response::Builder>();
				let mut status = resp.init_status();
				status.set_version("0.1.0");
				status.set_uptime_seconds(0.0);
			}
			capnp::serialize::write_message(&mut writer, &msg)?;
		}
	}

	Ok(())
}

fn handle_mount(
	bundle_path: &str,
	offline: bool,
	state: &Arc<Mutex<DaemonState>>,
) -> anyhow::Result<capnp::message::Builder<capnp::message::HeapAllocator>> {
	let bundle = Bundle::open(bundle_path)?;
	let app_id = bundle.header.app_id.clone();

	nexpack_core::Verifier::verify_layers(&bundle)?;
	let _ = nexpack_core::Verifier::verify_signature_opt(&bundle, offline);

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

	let bwrap_args: Vec<String> =
		sandbox::build_bwrap_args(&merged.to_string_lossy(), &bundle.header.permissions, &entrypoint, &[])
			.iter()
			.map(|o| o.to_string_lossy().to_string())
			.collect();

	{
		let mut s = state.lock().unwrap();
		let entry = s.mounts.entry(app_id.clone()).or_insert(MountEntry {
			app_id: app_id.clone(),
			rootfs: merged.clone(),
			refcount: 0,
			last_active: Instant::now(),
		});
		entry.refcount += 1;
		entry.last_active = Instant::now();
	}

	let seccomp_filter = sandbox::generate_seccomp_filter(&bundle.header.permissions);

	let mut msg = capnp::message::Builder::new_default();
	{
		let mut resp = msg.init_root::<nexpack_ipc::ipc_capnp::response::Builder>();
		let mut mount_resp = resp.init_mount();
		mount_resp.set_status("mounted");
		mount_resp.set_app_id(&app_id);
		mount_resp.set_rootfs(&merged.to_string_lossy());
		mount_resp.set_entrypoint(&entrypoint);
		mount_resp.set_seccomp_filter(&seccomp_filter);

		let mut args = mount_resp.init_bwrap_args(bwrap_args.len() as u32);
		for (i, arg) in bwrap_args.iter().enumerate() {
			args.set(i as u32, arg);
		}
	}

	Ok(msg)
}

fn handle_unmount(
	app_id: &str,
	state: &Arc<Mutex<DaemonState>>,
) -> anyhow::Result<capnp::message::Builder<capnp::message::HeapAllocator>> {
	let mut s = state.lock().unwrap();
	let mut msg = capnp::message::Builder::new_default();
	{
		let mut resp = msg.init_root::<nexpack_ipc::ipc_capnp::response::Builder>();

		if let Some(entry) = s.mounts.get_mut(app_id) {
			entry.refcount = entry.refcount.saturating_sub(1);
			entry.last_active = Instant::now();
			if entry.refcount == 0 {
				mount::unmount_overlay(&entry.rootfs)?;
				s.mounts.remove(app_id);
				let mut u = resp.init_unmount();
				u.set_status("unmounted");
			} else {
				let mut u = resp.init_unmount();
				u.set_status("refcount_decremented");
			}
		} else {
			let mut err = resp.init_error();
			err.set_message("not mounted");
		}
	}
	Ok(msg)
}

fn handle_list(state: &Arc<Mutex<DaemonState>>) -> anyhow::Result<capnp::message::Builder<capnp::message::HeapAllocator>> {
	let s = state.lock().unwrap();
	let mut msg = capnp::message::Builder::new_default();
	{
		let mut resp = msg.init_root::<nexpack_ipc::ipc_capnp::response::Builder>();
		let mut list_resp = resp.init_list();
		let mut apps = list_resp.init_apps(s.mounts.len() as u32);
		for (i, (id, entry)) in s.mounts.iter().enumerate() {
			let mut app_info = apps.reborrow().get(i as u32);
			app_info.set_app_id(id);
			app_info.set_rootfs(&entry.rootfs.to_string_lossy());
			app_info.set_entrypoint("");
		}
	}
	Ok(msg)
}
