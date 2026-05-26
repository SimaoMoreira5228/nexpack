use nexpack_core::Store;

pub fn update(app_id: Option<&str>, all: bool) -> anyhow::Result<()> {
	if all {
		let store = Store::user()?;
		let apps = store.list_apps()?;
		if apps.is_empty() {
			println!("No apps installed");
			return Ok(());
		}
		for app in &apps {
			check_updates(app)?;
		}
	} else if let Some(id) = app_id {
		check_updates(id)?;
	} else {
		println!("Usage: nxpk update <app-id> | nxpk update --all");
	}
	Ok(())
}

fn check_updates(app_id: &str) -> anyhow::Result<()> {
	let store = Store::user()?;
	let app_dir = store.apps_dir().join(app_id);
	let meta_path = app_dir.join("meta.cbor");

	if !meta_path.exists() {
		println!("App '{}' not installed", app_id);
		return Ok(());
	}

	let meta = std::fs::read(&meta_path)?;
	let header: nexpack_core::BundleHeader =
		ciborium::de::from_reader(&meta[..]).map_err(|e| anyhow::anyhow!("CBOR decode: {}", e))?;

	let update_url = match &header.update_url {
		Some(url) => url.clone(),
		None => {
			println!("{}: no update URL configured", app_id);
			return Ok(());
		}
	};

	println!("{}: checking {} for updates...", app_id, update_url);
	// TODO: Fetch update feed, compare digests, download missing layers
	println!("  (update feed fetch not yet implemented)");

	Ok(())
}
