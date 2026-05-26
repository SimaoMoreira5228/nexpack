use nexpack_core::Store;

pub fn gc() -> anyhow::Result<()> {
	let store = Store::user()?;
	println!("Running garbage collection...");
	let removed = store.gc()?;
	println!("Removed {} unreferenced layers", removed);
	Ok(())
}
