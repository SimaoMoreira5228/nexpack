use crate::{Bundle, Error, Result};
use blake3::Hash;

pub struct Verifier;

impl Verifier {
	pub fn verify_layers(bundle: &Bundle) -> Result<()> {
		for (i, layer) in bundle.header.layers.iter().enumerate() {
			let data = bundle.extract_layer(i)?;
			let actual = blake3::hash(data);
			let expected_hex = layer.digest_hex();
			let actual_hex = actual.to_hex();

			if actual_hex.as_str() != expected_hex {
				return Err(Error::DigestMismatch {
					expected: expected_hex.to_string(),
					actual: actual_hex.to_string(),
				});
			}
		}
		Ok(())
	}

	pub fn blake3_digest(data: &[u8]) -> Hash {
		blake3::hash(data)
	}

	pub fn blake3_hex(data: &[u8]) -> String {
		Self::blake3_digest(data).to_hex().to_string()
	}

	pub fn verify_digest(data: &[u8], expected_hex: &str) -> Result<()> {
		let actual = Self::blake3_hex(data);
		if actual != expected_hex {
			return Err(Error::DigestMismatch {
				expected: expected_hex.to_string(),
				actual,
			});
		}
		Ok(())
	}

	pub fn verify_signature(_bundle: &Bundle) -> Result<()> {
		// TODO: Implement Sigstore verification

		Ok(())
	}

	pub fn verify_sbom(_bundle: &Bundle) -> Result<()> {
		// TODO: Validate CycloneDX SBOM structure
		Ok(())
	}
}
