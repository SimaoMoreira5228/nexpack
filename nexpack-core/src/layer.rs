use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LayerRef {
	pub digest: String,
	pub size: u64,
	pub role: String,
}

impl LayerRef {
	pub fn digest_bytes(&self) -> Result<[u8; 32], String> {
		let hex = self
			.digest
			.strip_prefix("blake3:")
			.ok_or_else(|| format!("digest missing blake3: prefix: {}", self.digest))?;
		let mut out = [0u8; 32];
		hex::decode_to_slice(hex, &mut out).map_err(|e| format!("hex decode: {}", e))?;
		Ok(out)
	}

	pub fn digest_hex(&self) -> &str {
		self.digest.strip_prefix("blake3:").unwrap_or(&self.digest)
	}
}
