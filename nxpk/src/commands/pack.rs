use nexpack_core::permission::{PermissionSet, PermissionValue};
use nexpack_core::{
	BootstrapEntry, BundleHeader, LayerRef, Verifier, generate_sbom, make_bootstrap_data, make_bootstrap_trailer,
};
use std::path::{Path, PathBuf};

#[derive(serde::Deserialize)]
struct PackSpec {
	app: PackApp,
	permissions: Option<PackPermissions>,
	layer: Vec<PackLayer>,
	bootstrap: Option<PackBootstrap>,
}

#[derive(serde::Deserialize)]
struct PackApp {
	id: String,
	version: String,
	entrypoint: String,
}

#[derive(serde::Deserialize)]
struct PackPermissions {
	network: Option<serde_json::Value>,
	filesystem: Option<Vec<String>>,
	devices: Option<Vec<String>>,
	ipc: Option<Vec<String>>,
	display: Option<String>,
}

#[derive(serde::Deserialize)]
struct PackLayer {
	role: String,
	source: String,
}

#[derive(serde::Deserialize)]
struct PackBootstrap {
	nexpackd: Option<String>,
	nxpk: Option<String>,
}

pub fn pack(spec_path: &str) -> anyhow::Result<()> {
	let spec_data = std::fs::read_to_string(spec_path).map_err(|e| anyhow::anyhow!("reading spec {}: {}", spec_path, e))?;
	let spec: PackSpec = toml::from_str(&spec_data).map_err(|e| anyhow::anyhow!("parsing spec {}: {}", spec_path, e))?;

	let spec_dir = Path::new(spec_path).parent().unwrap_or(Path::new("."));

	let output_name = format!("{}.nxpk", spec.app.id.split('.').last().unwrap_or("app"));
	let output_path = Path::new(&output_name);
	let stub_data = find_stub()?;
	let mut layers: Vec<LayerRef> = Vec::new();
	let mut layer_data: Vec<Vec<u8>> = Vec::new();

	for layer_spec in &spec.layer {
		let source_path = spec_dir.join(&layer_spec.source);
		if !source_path.exists() {
			anyhow::bail!("layer source '{}' not found", source_path.display());
		}

		eprintln!("Building layer: {} <- {}", layer_spec.role, source_path.display());

		let erofs_data = build_erofs(&source_path)?;

		let hex = Verifier::blake3_hex(&erofs_data);
		let digest = format!("blake3:{}", hex);

		let size = erofs_data.len() as u64;
		let role = layer_spec.role.clone();

		layers.push(LayerRef { digest, size, role });
		layer_data.push(erofs_data);
	}

	if layers.is_empty() {
		anyhow::bail!("at least one layer is required");
	}

	let permissions = spec_to_permissions(&spec.permissions);
	let sbom = {
		let source_dirs: Vec<PathBuf> = spec.layer.iter().map(|ls| spec_dir.join(&ls.source)).collect();
		let refs: Vec<&Path> = source_dirs.iter().map(|p| p.as_path()).collect();
		let sbom_data = generate_sbom(
			spec.app.id.split('.').last().unwrap_or(&spec.app.id),
			&spec.app.version,
			&refs,
		)
		.map_err(|e| anyhow::anyhow!("SBOM generation failed: {}", e))?;
		Some(sbom_data)
	};
	let bootstrap_data = build_bootstrap_data(&spec, spec_dir)?;
	let header = BundleHeader {
		version: 1,
		app_id: spec.app.id,
		app_version: spec.app.version,
		entrypoint: spec.app.entrypoint,
		layers,
		permissions,
		signature: None,
		sbom,
		update_url: None,
		bootstrap_size: bootstrap_data.as_ref().map(|d| d.len() as u64),
		offset: 0,
		encoded_len: 0,
	};

	let header_bytes = header.encode()?;
	eprintln!("Writing {}", output_path.display());
	let mut out = std::fs::File::create(output_path)?;
	use std::io::Write;

	out.write_all(&stub_data)?;
	out.write_all(&header_bytes)?;

	for data in &layer_data {
		out.write_all(data)?;
	}

	if let Some(ref boot) = bootstrap_data {
		let boot_offset = out.metadata()?.len();
		out.write_all(boot)?;
		let trailer = make_bootstrap_trailer(boot_offset, boot.len() as u64);
		out.write_all(&trailer)?;
		eprintln!("  Bootstrap: {} B (nexpackd + nxpk)", boot.len());
	}

	let file_size = output_path.metadata()?.len();
	eprintln!("Done: {} ({})", output_path.display(), format_size(file_size));

	let b = BundleHeader::parse(&std::fs::read(output_path)?)?;
	eprintln!("  App:      {} v{}", b.app_id, b.app_version);
	eprintln!("  Entry:    {}", b.entrypoint);
	eprintln!("  Layers:   {}", b.layers.len());
	for l in &b.layers {
		eprintln!("    [{:>3} B] {}", l.size, l.role);
	}

	Ok(())
}

fn build_erofs(source: &Path) -> anyhow::Result<Vec<u8>> {
	use std::process::Command;

	if let Some(path) = find_tool("mkfs.erofs") {
		let tmp = std::env::temp_dir().join(format!("nexpack-pack-{}.erofs", std::process::id()));
		let status = Command::new(&path)
			.args([
				"-z",
				"lz4",
				"--ignore-mtime",
				"--force-uid=0",
				"--force-gid=0",
				&tmp.to_string_lossy(),
				&source.to_string_lossy(),
			])
			.status()
			.map_err(|e| anyhow::anyhow!("mkfs.erofs failed: {}", e))?;

		if !status.success() {
			anyhow::bail!("mkfs.erofs exited with {}", status.code().unwrap_or(-1));
		}

		let data = std::fs::read(&tmp)?;
		let _ = std::fs::remove_file(&tmp);
		return Ok(data);
	}

	anyhow::bail!("mkfs.erofs not found. Install erofs-utils or run in nix dev shell");
}

fn find_stub() -> anyhow::Result<Vec<u8>> {
	for loc in &["./stub/stub", "../stub/stub", "/nix/store/*/stub"] {
		if loc.contains('*') {
			if let Ok(entries) = std::fs::read_dir("/nix/store") {
				for entry in entries.flatten() {
					let candidate = entry.path().join("stub");
					if candidate.is_file() {
						return std::fs::read(&candidate).map_err(|e| anyhow::anyhow!("reading stub: {}", e));
					}
				}
			}
			continue;
		}
		let path = Path::new(loc);
		if path.is_file() {
			return std::fs::read(path).map_err(|e| anyhow::anyhow!("reading stub {}: {}", path.display(), e));
		}
	}

	eprintln!("Stub not found, building it...");
	let status = std::process::Command::new("make")
		.arg("-C")
		.arg(if Path::new("stub/Makefile").exists() {
			"stub"
		} else {
			"../stub"
		})
		.status()
		.map_err(|e| anyhow::anyhow!("make failed: {}", e))?;

	if !status.success() {
		anyhow::bail!("stub build failed");
	}

	let stub_path = if Path::new("stub/stub").exists() {
		"stub/stub"
	} else {
		"../stub/stub"
	};

	std::fs::read(stub_path).map_err(|e| anyhow::anyhow!("reading stub: {}", e))
}

fn find_tool(name: &str) -> Option<String> {
	for dir in std::env::split_paths(&std::env::var_os("PATH").unwrap_or_default()) {
		let candidate = dir.join(name);
		if candidate.is_file() {
			return Some(candidate.to_string_lossy().to_string());
		}
	}

	if let Ok(entries) = std::fs::read_dir("/nix/store") {
		for entry in entries.flatten() {
			let p = entry.path().join("bin").join(name);
			if p.is_file() {
				return Some(p.to_string_lossy().to_string());
			}
		}
	}
	None
}

fn build_bootstrap_data(spec: &PackSpec, spec_dir: &Path) -> anyhow::Result<Option<Vec<u8>>> {
	let bootstrap_spec = match &spec.bootstrap {
		Some(b) => b,
		None => return Ok(None),
	};

	let mut entries = Vec::new();

	if let Some(ref path_str) = bootstrap_spec.nexpackd {
		let path = spec_dir.join(path_str);
		let data = std::fs::read(&path)
			.map_err(|e| anyhow::anyhow!("reading nexpackd bootstrap binary '{}': {}", path.display(), e))?;
		entries.push(BootstrapEntry {
			name: "nexpackd".into(),
			data,
		});
	}

	if let Some(ref path_str) = bootstrap_spec.nxpk {
		let path = spec_dir.join(path_str);
		let data = std::fs::read(&path)
			.map_err(|e| anyhow::anyhow!("reading nxpk bootstrap binary '{}': {}", path.display(), e))?;
		entries.push(BootstrapEntry {
			name: "nxpk".into(),
			data,
		});
	}

	if entries.is_empty() {
		return Ok(None);
	}

	eprintln!("Embedding bootstrap binaries:");
	for e in &entries {
		eprintln!("  {} ({} B)", e.name, e.data.len());
	}

	Ok(Some(make_bootstrap_data(&entries)))
}

fn spec_to_permissions(spec: &Option<PackPermissions>) -> PermissionSet {
	let default = PermissionSet::default_sandboxed();
	let spec = match spec {
		Some(s) => s,
		None => return default,
	};

	let network = spec
		.network
		.as_ref()
		.map(|v| match v {
			serde_json::Value::Bool(b) => PermissionValue::Bool(*b),
			serde_json::Value::Array(a) => {
				PermissionValue::Patterns(a.iter().filter_map(|v| v.as_str().map(String::from)).collect())
			}
			_ => PermissionValue::Bool(false),
		})
		.unwrap_or(PermissionValue::Bool(false));

	let filesystem = spec.filesystem.clone().unwrap_or_default();
	let devices = spec.devices.clone().unwrap_or_default();
	let ipc = spec
		.ipc
		.clone()
		.unwrap_or_else(|| vec!["wayland".into(), "dbus-session".into()]);
	let display = spec.display.clone().unwrap_or_else(|| "wayland".into());

	PermissionSet {
		network,
		filesystem,
		devices,
		ipc,
		display,
	}
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
