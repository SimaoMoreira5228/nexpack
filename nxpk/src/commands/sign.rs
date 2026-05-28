use nexpack_core::Bundle;
use std::io::Write;
use std::path::Path;
use std::process::Command;

pub fn sign_bundle(bundle_path: &str, output: Option<&str>) -> anyhow::Result<()> {
	let bundle = Bundle::open(bundle_path)?;

	if bundle.header.signature.is_some() {
		eprintln!("Bundle already has a signature. Use --force to re-sign.");
		return Ok(());
	}

	let artifact = nexpack_core::signing::artifact_bytes(&bundle)?;
	let tmp_sig = std::env::temp_dir().join(format!("nexpack-sig-{}.json", std::process::id()));

	if let Some(cosign) = find_tool("cosign") {
		eprintln!("Signing with cosign (keyless)...");
		let status = Command::new(&cosign)
			.args(["sign-blob", "--bundle", &tmp_sig.to_string_lossy(), "--yes", "-"])
			.stdin(std::process::Stdio::piped())
			.stdout(std::process::Stdio::null())
			.stderr(std::process::Stdio::inherit())
			.spawn()
			.and_then(|mut child| {
				if let Some(ref mut stdin) = child.stdin {
					stdin.write_all(&artifact)?;
				}
				child.wait()
			})?;

		if !status.success() {
			anyhow::bail!("cosign sign-blob failed (exit {})", status.code().unwrap_or(-1));
		}
	} else {
		anyhow::bail!(
			"cosign not found in PATH. Install cosign or use:\n  \
			 cosign sign-blob --bundle sig.json {}",
			bundle_path
		);
	}

	let sig_data = std::fs::read(&tmp_sig)?;
	let _ = std::fs::remove_file(&tmp_sig);

	let _: serde_json::Value =
		serde_json::from_slice(&sig_data).map_err(|e| anyhow::anyhow!("invalid cosign output: {}", e))?;

	let header = nexpack_core::BundleHeader {
		signature: Some(sig_data),
		..bundle.header.clone()
	};

	let header_bytes = header.encode()?;
	let output_path: std::path::PathBuf = match output {
		Some(p) => Path::new(p).to_path_buf(),
		None => {
			let p = Path::new(bundle_path);
			let stem = p.file_stem().unwrap_or_default();
			Path::new(&format!("{}_signed.nxpk", stem.to_string_lossy())).to_path_buf()
		}
	};

	let mut out = std::fs::File::create(&output_path)?;

	let stub_end = bundle.header.offset as usize;
	out.write_all(&bundle.data[..stub_end])?;
	out.write_all(&header_bytes)?;

	for i in 0..bundle.header.layers.len() {
		out.write_all(bundle.extract_layer(i)?)?;
	}

	let file_size = std::fs::metadata(&output_path)?.len();
	eprintln!(
		"Signed bundle written: {} ({})",
		output_path.display(),
		format_size(file_size)
	);

	Ok(())
}

fn find_tool(name: &str) -> Option<String> {
	for dir in std::env::split_paths(&std::env::var_os("PATH").unwrap_or_default()) {
		let candidate = dir.join(name);
		if candidate.is_file() {
			return Some(candidate.to_string_lossy().to_string());
		}
	}
	None
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
