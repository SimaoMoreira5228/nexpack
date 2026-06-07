use crate::error::Error;
use crate::layer::LayerRef;
use crate::permission::PermissionSet;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BundleHeader {
	pub version: u32,
	pub app_id: String,
	pub app_version: String,
	pub entrypoint: String,
	pub layers: Vec<LayerRef>,
	pub permissions: PermissionSet,
	pub signature: Option<Vec<u8>>,
	pub sbom: Option<Vec<u8>>,
	pub update_url: Option<String>,
	pub bootstrap_size: Option<u64>,

	#[serde(skip)]
	pub offset: u64,
	#[serde(skip)]
	pub encoded_len: u64,
}

impl BundleHeader {
	pub const MAGIC: &'static [u8; 4] = b"NX01";

	pub fn parse(data: &[u8]) -> Result<Self, Error> {
		let magic_pos = data
			.windows(4)
			.position(|w| w == Self::MAGIC)
			.ok_or_else(|| Error::InvalidFormat("NX01 magic not found".into()))?;

		let offset = magic_pos as u64;
		let cbor_data = &data[magic_pos + 4..];

		let mut cursor = std::io::Cursor::new(cbor_data);
		let header: BundleHeader = ciborium::de::from_reader(&mut cursor).map_err(|e| Error::Cbor(e.to_string()))?;

		let consumed = cursor.position() as u64;
		let encoded_len = consumed + 4;

		Ok(BundleHeader {
			offset,
			encoded_len,
			..header
		})
	}

	pub fn encode(&self) -> Result<Vec<u8>, Error> {
		let mut buf = Vec::new();
		buf.extend_from_slice(Self::MAGIC);
		ciborium::ser::into_writer(self, &mut buf).map_err(|e| Error::InvalidFormat(format!("CBOR encode: {}", e)))?;
		Ok(buf)
	}
}
