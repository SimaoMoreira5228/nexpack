use crate::Result;
use serde::Serialize;
use std::path::Path;

pub fn generate_sbom(app_name: &str, app_version: &str, layer_sources: &[&Path]) -> Result<Vec<u8>> {
	use std::time::{SystemTime, UNIX_EPOCH};

	let timestamp = {
		let d = SystemTime::now().duration_since(UNIX_EPOCH).unwrap_or_default();
		let secs = d.as_secs();
		format!("{}", secs)
	};

	let mut components: Vec<CycloneDXComponent> = Vec::new();

	for source in layer_sources {
		collect_files(source, source, &mut components)?;
	}

	components.sort_by(|a, b| a.name.cmp(&b.name));

	let bom = CycloneDXBom {
		bom_format: "CycloneDX".to_string(),
		spec_version: "1.5".to_string(),
		version: 1,
		metadata: CycloneDXMetadata {
			timestamp,
			component: CycloneDXComponent {
				r#type: "application".to_string(),
				name: app_name.to_string(),
				version: app_version.to_string(),
				hashes: Vec::new(),
			},
		},
		components,
	};

	let json = serde_json::to_vec(&bom).map_err(|e| crate::Error::Cbor(e.to_string()))?;

	let compressed = compress_gzip(&json)?;
	Ok(compressed)
}

pub fn verify_sbom_data(data: &[u8]) -> Result<()> {
	let decompressed = decompress_gzip(data)?;
	let bom: serde_json::Value = serde_json::from_slice(&decompressed).map_err(|e| crate::Error::Cbor(e.to_string()))?;

	let bom_format = bom.get("bomFormat").and_then(|v| v.as_str()).unwrap_or("");
	if bom_format != "CycloneDX" {
		return Err(crate::Error::Cbor("SBOM: invalid bomFormat".into()));
	}

	let spec = bom.get("specVersion").and_then(|v| v.as_str()).unwrap_or("");
	if spec.is_empty() {
		return Err(crate::Error::Cbor("SBOM: missing specVersion".into()));
	}

	Ok(())
}

fn collect_files(base: &Path, dir: &Path, components: &mut Vec<CycloneDXComponent>) -> Result<()> {
	let entries = match std::fs::read_dir(dir) {
		Ok(e) => e,
		Err(_) => return Ok(()),
	};

	for entry in entries.flatten() {
		let path = entry.path();
		let ft = match entry.file_type() {
			Ok(t) => t,
			Err(_) => continue,
		};

		if ft.is_dir() {
			collect_files(base, &path, components)?;
		} else if ft.is_file() || ft.is_symlink() {
			let data = match std::fs::read(&path) {
				Ok(d) => d,
				Err(_) => continue,
			};
			let hash = blake3::hash(&data).to_hex().to_string();

			let rel = path.strip_prefix(base).unwrap_or(&path).to_string_lossy().to_string();

			components.push(CycloneDXComponent {
				r#type: "file".to_string(),
				name: rel,
				version: String::new(),
				hashes: vec![CycloneDXHash {
					alg: "BLAKE3".to_string(),
					value: hash,
				}],
			});
		}
	}

	Ok(())
}

fn compress_gzip(data: &[u8]) -> Result<Vec<u8>> {
	use std::io::Write;
	let mut encoder = flate2::write::GzEncoder::new(Vec::new(), flate2::Compression::default());
	encoder.write_all(data).map_err(|e| crate::Error::Cbor(e.to_string()))?;
	encoder.finish().map_err(|e| crate::Error::Cbor(e.to_string()))
}

fn decompress_gzip(data: &[u8]) -> Result<Vec<u8>> {
	use std::io::Read;
	let mut decoder = flate2::read::GzDecoder::new(data);
	let mut buf = Vec::new();
	decoder.read_to_end(&mut buf).map_err(|e| crate::Error::Cbor(e.to_string()))?;
	Ok(buf)
}

#[derive(Debug, Serialize)]
struct CycloneDXBom {
	#[serde(rename = "bomFormat")]
	bom_format: String,
	#[serde(rename = "specVersion")]
	spec_version: String,
	version: u32,
	metadata: CycloneDXMetadata,
	#[serde(skip_serializing_if = "Vec::is_empty")]
	components: Vec<CycloneDXComponent>,
}

#[derive(Debug, Serialize)]
struct CycloneDXMetadata {
	timestamp: String,
	component: CycloneDXComponent,
}

#[derive(Debug, Serialize)]
struct CycloneDXComponent {
	r#type: String,
	name: String,
	#[serde(skip_serializing_if = "String::is_empty")]
	version: String,
	#[serde(skip_serializing_if = "Vec::is_empty")]
	hashes: Vec<CycloneDXHash>,
}

#[derive(Debug, Serialize)]
struct CycloneDXHash {
	alg: String,
	value: String,
}
