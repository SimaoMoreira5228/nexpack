use nexpack_core::Store;

pub fn permissions(app_id: &str, _edit: bool) -> anyhow::Result<()> {
	let store = Store::user()?;
	let app_dir = store.apps_dir().join(app_id);
	let meta_path = app_dir.join("meta.cbor");

	if !meta_path.exists() {
		anyhow::bail!("app '{}' is not installed", app_id);
	}

	let meta = std::fs::read(&meta_path)?;
	let header: nexpack_core::BundleHeader =
		ciborium::de::from_reader(&meta[..]).map_err(|e| anyhow::anyhow!("CBOR decode: {}", e))?;

	println!("Permissions for: {}", app_id);
	println!("  network:    {:?}", header.permissions.network);
	println!("  filesystem: {:?}", header.permissions.filesystem);
	println!("  devices:    {:?}", header.permissions.devices);
	println!("  ipc:        {:?}", header.permissions.ipc);
	println!("  display:    {:?}", header.permissions.display);

	if _edit {
		println!("\nInteractive edit not yet implemented");
	}

	Ok(())
}
