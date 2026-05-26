pub mod error;
pub mod header;
pub mod layer;
pub mod permission;
pub mod store;
pub mod verify;

pub use error::Error;
pub use header::BundleHeader;
pub use layer::LayerRef;
pub use permission::PermissionSet;
pub use store::Store;
pub use verify::Verifier;

use std::path::Path;

pub type Result<T> = std::result::Result<T, Error>;

pub struct Bundle {
	pub header: BundleHeader,
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
}
