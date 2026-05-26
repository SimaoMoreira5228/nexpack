use nexpack_core::Bundle;

pub fn inspect(bundle_path: &str, as_json: bool) -> anyhow::Result<()> {
	let bundle = Bundle::open(bundle_path)?;

	if as_json {
		let json = serde_json::to_string_pretty(&serde_json::json!({
			"version": bundle.header.version,
			"app_id": bundle.header.app_id,
			"app_version": bundle.header.app_version,
			"entrypoint": bundle.header.entrypoint,
			"layers": bundle.header.layers,
			"permissions": bundle.header.permissions,
			"has_signature": bundle.header.signature.is_some(),
			"has_sbom": bundle.header.sbom.is_some(),
			"update_url": bundle.header.update_url,
			"stub_digest": bundle.stub_digest().to_hex().to_string(),
		}))?;
		println!("{}", json);
		return Ok(());
	}

	println!("Nexpack Bundle Inspector");
	println!("========================");
	println!(" File:        {}", bundle_path);
	println!(" App ID:      {}", bundle.header.app_id);
	println!(" Version:     {}", bundle.header.app_version);
	println!(" Entrypoint:  {}", bundle.header.entrypoint);
	println!(" Format ver:  {}", bundle.header.version);
	println!();
	println!("Layers:");
	for (i, layer) in bundle.header.layers.iter().enumerate() {
		println!(
			"  [{:02}] {:32} {:>10} bytes  role={}",
			i,
			layer.digest_hex().chars().take(32).collect::<String>(),
			layer.size,
			layer.role,
		);
	}
	println!();
	println!("Permissions:");
	println!("  network:    {:?}", bundle.header.permissions.network);
	println!("  filesystem: {:?}", bundle.header.permissions.filesystem);
	println!("  devices:    {:?}", bundle.header.permissions.devices);
	println!("  ipc:        {:?}", bundle.header.permissions.ipc);
	println!("  display:    {:?}", bundle.header.permissions.display);
	println!();
	println!(
		"Signature:  {}",
		if bundle.header.signature.is_some() {
			"present"
		} else {
			"none"
		}
	);
	println!(
		"SBOM:       {}",
		if bundle.header.sbom.is_some() { "present" } else { "none" }
	);
	println!("Update URL: {}", bundle.header.update_url.as_deref().unwrap_or("none"));
	println!("Stub hash:  {}", bundle.stub_digest().to_hex());

	Ok(())
}
