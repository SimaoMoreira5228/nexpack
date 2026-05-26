use nexpack_core::Bundle;

pub fn verify(bundle_path: &str) -> anyhow::Result<()> {
	let bundle = Bundle::open(bundle_path)?;

	println!("Verifying: {}", bundle_path);
	println!("App:       {} v{}", bundle.header.app_id, bundle.header.app_version);

	println!("\nLayer digests (BLAKE3):");
	for (i, layer) in bundle.header.layers.iter().enumerate() {
		let data = bundle.extract_layer(i)?;
		let actual = nexpack_core::Verifier::blake3_hex(data);
		let expected = layer.digest_hex();
		let status = if actual == expected { "OK" } else { "MISMATCH" };
		println!("  [{:02}] {}  {}", i, actual.chars().take(48).collect::<String>(), status);
		if actual != expected {
			anyhow::bail!("Layer {} digest mismatch", i);
		}
	}

	println!("\nAll layer digests verified successfully");
	Ok(())
}
