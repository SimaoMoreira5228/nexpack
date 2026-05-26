use nexpack_core::Store;

pub fn remove(app_id: &str) -> anyhow::Result<()> {
	let store = Store::user()?;

	if !store.apps_dir().join(app_id).exists() {
		anyhow::bail!("app '{}' is not installed", app_id);
	}

	store.remove_app(app_id)?;
	println!("Removed: {}", app_id);
	Ok(())
}
