use nexpack_core::{Bundle, Store, Verifier};

pub fn install(bundle_path: &str) -> anyhow::Result<()> {
	let bundle = Bundle::open(bundle_path)?;
	let store = Store::user()?;

	println!("Installing: {} v{}", bundle.header.app_id, bundle.header.app_version);

	for (i, layer) in bundle.header.layers.iter().enumerate() {
		let digest_hex = layer.digest_hex();
		if store.has_layer(digest_hex) {
			println!(
				"  layer [{:02}] {} — already present, skipping",
				i,
				digest_hex.chars().take(16).collect::<String>()
			);
			continue;
		}

		let data = bundle.extract_layer(i)?;
		Verifier::verify_digest(data, digest_hex)?;
		store.store_layer(data, digest_hex)?;
		println!(
			"  layer [{:02}] {} — stored",
			i,
			digest_hex.chars().take(16).collect::<String>()
		);
	}

	let current_digest = bundle.header.layers.last().map(|l| l.digest_hex()).unwrap_or("");

	let meta = bundle.header.encode()?;
	store.register_app(&bundle.header.app_id, current_digest, &meta)?;
	store.add_gc_root(&bundle.header.app_id, current_digest)?;

	println!("\nInstalled: {}", bundle.header.app_id);
	println!("  Layers in store: {}", bundle.header.layers.len());
	println!("  Run with: nxpk run {}", bundle_path);

	Ok(())
}
