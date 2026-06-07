use nexpack_core::{Store, Verifier};
use std::io::Read;

pub fn update(app_id: Option<&str>, all: bool) -> anyhow::Result<()> {
	if all {
		let store = Store::user()?;
		let apps = store.list_apps()?;
		if apps.is_empty() {
			println!("No apps installed");
			return Ok(());
		}
		for app in &apps {
			if let Err(e) = check_and_apply_updates(app) {
				eprintln!("  error updating {}: {}", app, e);
			}
		}
	} else if let Some(id) = app_id {
		check_and_apply_updates(id)?;
	} else {
		println!("Usage: nxpk update <app-id> | nxpk update --all");
	}
	Ok(())
}

fn check_and_apply_updates(app_id: &str) -> anyhow::Result<()> {
	let store = Store::user()?;
	let app_dir = store.apps_dir().join(app_id);
	let meta_path = app_dir.join("meta.cbor");

	if !meta_path.exists() {
		anyhow::bail!("app '{}' is not installed", app_id);
	}

	let meta = std::fs::read(&meta_path)?;
	let header: nexpack_core::BundleHeader =
		nexpack_core::BundleHeader::parse(&meta).map_err(|e| anyhow::anyhow!("header decode: {}", e))?;

	let update_url = match &header.update_url {
		Some(url) => url.clone(),
		None => {
			println!("{}: no update URL configured", app_id);
			return Ok(());
		}
	};

	println!("{}: checking {}...", app_id, update_url);

	let feed: nexpack_core::update::UpdateFeed = fetch_feed(&update_url)?;

	if feed.app_id != app_id {
		anyhow::bail!("feed app_id '{}' doesn't match installed app '{}'", feed.app_id, app_id);
	}

	println!("  current: v{}  latest: v{}", header.app_version, feed.latest);

	if feed.latest == header.app_version {
		println!("  already up to date");
		return Ok(());
	}

	let release = match feed.latest_release() {
		Some(r) => r,
		None => anyhow::bail!("feed has no release for version '{}'", feed.latest),
	};

	let missing = release.missing_layers(&store);
	if missing.is_empty() {
		println!("  all layers already in store");
	} else {
		println!("  downloading {} missing layers...", missing.len());
		for layer in &missing {
			let hex = layer.digest.strip_prefix("blake3:").unwrap_or(&layer.digest);
			println!("    downloading layer {} ({})", &hex[..16], format_size(layer.size));

			let data = download_layer(&layer.url)?;

			if data.len() as u64 != layer.size {
				anyhow::bail!("size mismatch for {}: expected {}, got {}", hex, layer.size, data.len());
			}

			Verifier::verify_digest(&data, hex)?;
			store.store_layer(&data, hex)?;
			println!("      stored and verified");
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
		std::os::unix::fs::symlink(&target, &tmp_link)?;
		std::fs::rename(&tmp_link, &current_link)?;

		store.add_gc_root(app_id, new_digest)?;
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
	let new_meta = new_header.encode()?;
	std::fs::write(&meta_path, &new_meta)?;

	println!("  updated to v{}", feed.latest);
	Ok(())
}

fn fetch_feed(url: &str) -> anyhow::Result<nexpack_core::update::UpdateFeed> {
	let resp = ureq::get(url)
		.call()
		.map_err(|e| anyhow::anyhow!("fetching update feed: {}", e))?;

	if resp.status() != 200 {
		anyhow::bail!("HTTP {} fetching {}", resp.status(), url);
	}

	let feed: nexpack_core::update::UpdateFeed = resp
		.into_json()
		.map_err(|e| anyhow::anyhow!("parsing feed from {}: {}", url, e))?;

	Ok(feed)
}

fn download_layer(url: &str) -> anyhow::Result<Vec<u8>> {
	let resp = ureq::get(url)
		.call()
		.map_err(|e| anyhow::anyhow!("downloading {}: {}", url, e))?;

	if resp.status() != 200 {
		anyhow::bail!("HTTP {} downloading {}", resp.status(), url);
	}

	let mut data = Vec::new();
	resp.into_reader()
		.read_to_end(&mut data)
		.map_err(|e| anyhow::anyhow!("reading {}: {}", url, e))?;

	Ok(data)
}

fn format_size(size: u64) -> String {
	const UNITS: &[&str] = &["B", "KB", "MB", "GB"];
	let mut s = size as f64;
	let mut unit_idx = 0;
	while s >= 1024.0 && unit_idx < UNITS.len() - 1 {
		s /= 1024.0;
		unit_idx += 1;
	}
	if unit_idx == 0 {
		format!("{} {}", size, UNITS[unit_idx])
	} else {
		format!("{:.1} {}", s, UNITS[unit_idx])
	}
}
