use nexpack_core::{BundleHeader, Store, Verifier};
use std::io::Write;
use std::path::Path;

include!(concat!(env!("OUT_DIR"), "/stub_bytes.rs"));

pub fn export_app(app_id: &str) -> anyhow::Result<()> {
	let store = Store::user()?;
	let app_dir = store.apps_dir().join(app_id);
	let meta_path = app_dir.join("meta.capnp");

	if !meta_path.exists() {
		anyhow::bail!("app '{}' is not installed", app_id);
	}

	let meta = std::fs::read(&meta_path)?;
	let header: BundleHeader = BundleHeader::parse(&meta).map_err(|e| anyhow::anyhow!("header decode: {}", e))?;

	let output_name = format!("{}.nxpk", app_id.rsplit('.').next().unwrap_or(app_id));
	let output_path = Path::new(&output_name);

	eprintln!("Exporting: {} v{}", app_id, header.app_version);
	eprintln!("  Output:  {}", output_path.display());

	let stub_data = STUB_BYTES.to_vec();
	let header_bytes = header.encode()?;

	let mut out = std::fs::File::create(output_path)?;
	out.write_all(&stub_data)?;
	out.write_all(&header_bytes)?;

	for layer in &header.layers {
		let digest_hex = layer.digest_hex();
		let image_path = store.layer_image(digest_hex);

		if !image_path.exists() {
			anyhow::bail!("layer {} not found in store (digest: {})", layer.role, digest_hex);
		}

		let data = std::fs::read(&image_path)?;
		Verifier::verify_digest(&data, digest_hex)?;
		out.write_all(&data)?;
		eprintln!("  Layer [{}] {} — {} B", layer.role, &digest_hex[..16], data.len());
	}

	let file_size = output_path.metadata()?.len();
	eprintln!("Done: {} ({})", output_path.display(), format_size(file_size));

	Ok(())
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
