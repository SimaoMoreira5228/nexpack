pub mod error;
pub mod header;
pub mod layer;
pub mod permission;
pub mod sbom;
pub mod signing;
pub mod store;
pub mod update;
pub mod verify;

pub use error::Error;
pub use header::BundleHeader;
pub use layer::LayerRef;
pub use permission::PermissionSet;
pub use sbom::{generate_sbom, verify_sbom_data};
pub use signing::{TrustConfig, verify_signature_opt, verify_with_identity_opt};
pub use store::Store;
pub use verify::Verifier;

use std::path::Path;

pub type Result<T> = std::result::Result<T, Error>;

pub const BOOTSTRAP_TRAILER_MAGIC: &[u8; 4] = b"NXBT";

pub struct Bundle {
	pub header: BundleHeader,
	pub data: Vec<u8>,
}

#[derive(Debug)]
pub struct BootstrapEntry {
	pub name: String,
	pub data: Vec<u8>,
}

impl Bundle {
	pub fn parse(data: Vec<u8>) -> Result<Self> {
		let header = BundleHeader::parse(&data)?;
		Ok(Self { header, data })
	}

	pub fn open(path: impl AsRef<Path>) -> Result<Self> {
		let data = std::fs::read(path.as_ref()).map_err(|e| Error::Io {
			context: format!("reading {}", path.as_ref().display()),
			source: e,
		})?;
		Self::parse(data)
	}

	pub fn header_offset(&self) -> u64 {
		self.header.offset
	}

	pub fn layers_offset(&self) -> u64 {
		self.header.offset + self.header.encoded_len as u64
	}

	pub fn layers_end(&self) -> usize {
		let mut off = self.layers_offset() as usize;
		for layer in &self.header.layers {
			off += layer.size as usize;
		}
		off
	}

	pub fn extract_layer(&self, index: usize) -> Result<&[u8]> {
		let layers = &self.header.layers;
		if index >= layers.len() {
			return Err(Error::IndexOutOfRange {
				what: "layer index".into(),
				index,
				max: layers.len(),
			});
		}

		let mut offset = self.layers_offset() as usize;
		for i in 0..index {
			offset += layers[i].size as usize;
		}

		let end = offset + layers[index].size as usize;
		if end > self.data.len() {
			return Err(Error::InvalidFormat(format!(
				"layer {} exceeds bundle bounds (offset {}, size {})",
				index, offset, layers[index].size
			)));
		}

		Ok(&self.data[offset..end])
	}

	pub fn stub_digest(&self) -> blake3::Hash {
		let end = self.header.offset as usize;
		blake3::hash(&self.data[..end])
	}

	pub fn has_bootstrap(&self) -> bool {
		self.header.bootstrap_size.is_some_and(|s| s > 0)
	}

	pub fn bootstrap_offset(&self) -> Option<usize> {
		self.header.bootstrap_size.filter(|s| *s > 0).map(|_| self.layers_end())
	}

	pub fn bootstrap_data(&self) -> Option<&[u8]> {
		let size = self.header.bootstrap_size?;
		if size == 0 {
			return None;
		}
		let offset = self.layers_end();
		if offset + size as usize > self.data.len() {
			return None;
		}
		Some(&self.data[offset..offset + size as usize])
	}

	pub fn extract_bootstrap(&self) -> Result<Vec<BootstrapEntry>> {
		let data = self
			.bootstrap_data()
			.ok_or_else(|| Error::InvalidFormat("no bootstrap data in bundle".into()))?;
		parse_bootstrap_entries(data)
	}

	pub fn read_trailer(data: &[u8]) -> Option<(u64, u64)> {
		if data.len() < 20 {
			return None;
		}
		let pos = data.len() - 20;
		if &data[pos..pos + 4] != BOOTSTRAP_TRAILER_MAGIC {
			return None;
		}
		let offset = u64::from_le_bytes(data[pos + 4..pos + 12].try_into().ok()?);
		let size = u64::from_le_bytes(data[pos + 12..pos + 20].try_into().ok()?);
		Some((offset, size))
	}
}

fn parse_bootstrap_entries(data: &[u8]) -> Result<Vec<BootstrapEntry>> {
	if data.len() < 4 {
		return Err(Error::InvalidFormat("bootstrap data too short".into()));
	}
	let count = u32::from_le_bytes([data[0], data[1], data[2], data[3]]);
	let mut offset = 4;
	let mut entries = Vec::with_capacity(count as usize);

	for _ in 0..count {
		if offset + 2 > data.len() {
			return Err(Error::InvalidFormat("bootstrap entry name_len truncated".into()));
		}
		let name_len = u16::from_le_bytes([data[offset], data[offset + 1]]) as usize;
		offset += 2;
		if offset + name_len > data.len() {
			return Err(Error::InvalidFormat("bootstrap entry name truncated".into()));
		}
		let name = String::from_utf8_lossy(&data[offset..offset + name_len]).to_string();
		offset += name_len;
		if offset + 4 > data.len() {
			return Err(Error::InvalidFormat("bootstrap entry data_len truncated".into()));
		}
		let data_len = u32::from_le_bytes([data[offset], data[offset + 1], data[offset + 2], data[offset + 3]]) as usize;
		offset += 4;
		if offset + data_len > data.len() {
			return Err(Error::InvalidFormat("bootstrap entry data truncated".into()));
		}
		let binary = data[offset..offset + data_len].to_vec();
		offset += data_len;
		entries.push(BootstrapEntry { name, data: binary });
	}

	Ok(entries)
}

pub fn make_bootstrap_data(entries: &[BootstrapEntry]) -> Vec<u8> {
	let mut buf = Vec::new();
	buf.extend_from_slice(&(entries.len() as u32).to_le_bytes());
	for entry in entries {
		let name_bytes = entry.name.as_bytes();
		buf.extend_from_slice(&(name_bytes.len() as u16).to_le_bytes());
		buf.extend_from_slice(name_bytes);
		buf.extend_from_slice(&(entry.data.len() as u32).to_le_bytes());
		buf.extend_from_slice(&entry.data);
	}
	buf
}

pub fn make_bootstrap_trailer(bootstrap_offset: u64, bootstrap_size: u64) -> Vec<u8> {
	let mut buf = Vec::with_capacity(20);
	buf.extend_from_slice(BOOTSTRAP_TRAILER_MAGIC);
	buf.extend_from_slice(&bootstrap_offset.to_le_bytes());
	buf.extend_from_slice(&bootstrap_size.to_le_bytes());
	buf
}

pub fn nexpack_home() -> Result<std::path::PathBuf> {
	let home = std::env::var("HOME").map_err(|_| Error::StorePath("$HOME not set".into()))?;
	Ok(std::path::PathBuf::from(home).join(".local/share/nexpack"))
}

pub fn nexpack_bin_dir() -> Result<std::path::PathBuf> {
	Ok(nexpack_home()?.join("bin"))
}
