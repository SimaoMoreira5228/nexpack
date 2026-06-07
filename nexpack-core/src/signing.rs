use crate::{Bundle, Error, Result};
use sigstore_bundle::validation::{ValidationOptions, validate_bundle_with_options};
use sigstore_trust_root::trusted_root::{SIGSTORE_PRODUCTION_TRUSTED_ROOT, TrustedRoot};

pub fn artifact_bytes(bundle: &Bundle) -> Result<Vec<u8>> {
	let mut data = Vec::new();
	let mut header = bundle.header.clone();
	header.signature = None;
	let header_cbor = header.encode()?;
	data.extend_from_slice(&header_cbor);
	for layer in &bundle.header.layers {
		data.extend_from_slice(layer.digest.as_bytes());
	}
	Ok(data)
}

fn parse_bundle(sig_bytes: &[u8]) -> Result<sigstore_types::Bundle> {
	let bundle: sigstore_types::Bundle = serde_json::from_slice(sig_bytes)
		.map_err(|e| Error::SignatureVerification(format!("invalid signature JSON: {}", e)))?;
	validate_bundle_with_options(&bundle, &ValidationOptions::default())
		.map_err(|e| Error::SignatureVerification(format!("bundle validation: {}", e)))?;
	Ok(bundle)
}

fn trusted_root() -> Result<TrustedRoot> {
	TrustedRoot::from_json(SIGSTORE_PRODUCTION_TRUSTED_ROOT)
		.map_err(|e| Error::SignatureVerification(format!("trusted root: {}", e)))
}

pub fn verify_signature(bundle: &Bundle) -> Result<()> {
	verify_signature_opt(bundle, false)
}

pub fn verify_signature_opt(bundle: &Bundle, offline: bool) -> Result<()> {
	let sig_bytes = match &bundle.header.signature {
		Some(s) => s,
		None => return Err(Error::SignatureVerification("no signature in bundle".into())),
	};

	let sig_bundle = parse_bundle(sig_bytes)?;
	let artifact = artifact_bytes(bundle)?;
	let root = trusted_root()?;

	let policy = sigstore_verify::VerificationPolicy {
		identity: None,
		issuer: None,
		verify_tlog: !offline,
		verify_timestamp: !offline,
		verify_certificate: true,
		verify_sct: true,
		clock_skew_seconds: 60,
	};

	if offline {
		tracing::info!("offline mode: skipping Rekor tlog and timestamp verification");
	}

	sigstore_verify::verify(&artifact, &sig_bundle, &policy, &root)
		.map_err(|e| Error::SignatureVerification(format!("verification failed: {}", e)))?;

	Ok(())
}

pub fn verify_with_identity(bundle: &Bundle, expected_identity: &str, expected_issuer: &str) -> Result<()> {
	verify_with_identity_opt(bundle, expected_identity, expected_issuer, false)
}

pub fn verify_with_identity_opt(
	bundle: &Bundle,
	expected_identity: &str,
	expected_issuer: &str,
	offline: bool,
) -> Result<()> {
	let sig_bytes = match &bundle.header.signature {
		Some(s) => s,
		None => return Err(Error::SignatureVerification("no signature in bundle".into())),
	};

	let sig_bundle = parse_bundle(sig_bytes)?;
	let artifact = artifact_bytes(bundle)?;
	let root = trusted_root()?;

	let policy = sigstore_verify::VerificationPolicy {
		identity: Some(expected_identity.to_string()),
		issuer: Some(expected_issuer.to_string()),
		verify_tlog: !offline,
		verify_timestamp: !offline,
		verify_certificate: true,
		verify_sct: true,
		clock_skew_seconds: 60,
	};

	if offline {
		tracing::info!("offline mode: skipping Rekor tlog and timestamp verification");
	}

	sigstore_verify::verify(&artifact, &sig_bundle, &policy, &root)
		.map_err(|e| Error::SignatureVerification(format!("identity verification failed: {}", e)))?;

	Ok(())
}

#[derive(Debug, Clone, serde::Deserialize)]
pub struct TrustPolicyEntry {
	pub identities: Vec<String>,
	pub require_rekor: Option<bool>,
}

#[derive(Debug, Clone, serde::Deserialize)]
pub struct TrustConfig {
	#[serde(default)]
	pub policy: std::collections::HashMap<String, TrustPolicyEntry>,
}

impl TrustConfig {
	pub fn load() -> Result<Self> {
		let config_dir = config_dir().ok_or_else(|| Error::Other("cannot determine config directory".into()))?;
		let trust_file = config_dir.join("trust.toml");
		if !trust_file.exists() {
			return Ok(Self {
				policy: std::collections::HashMap::new(),
			});
		}
		let content = std::fs::read_to_string(&trust_file).map_err(|e| Error::Io {
			context: "reading trust.toml".into(),
			source: e,
		})?;
		toml::from_str(&content).map_err(|e| Error::Other(format!("parsing trust.toml: {}", e)))
	}

	pub fn match_policy<'a>(&'a self, app_id: &str) -> Option<(&'a str, &'a TrustPolicyEntry)> {
		if let Some((pattern, entry)) = self.policy.get_key_value(app_id) {
			return Some((pattern, entry));
		}
		let mut best: Option<(&'a str, &'a TrustPolicyEntry)> = None;
		let mut best_len = 0usize;
		for (pattern, entry) in &self.policy {
			if wildmatch(pattern, app_id) {
				let len = pattern.len();
				if len > best_len {
					best = Some((pattern.as_str(), entry));
					best_len = len;
				}
			}
		}
		best
	}
}

fn config_dir() -> Option<std::path::PathBuf> {
	std::env::var("XDG_CONFIG_HOME")
		.ok()
		.map(std::path::PathBuf::from)
		.or_else(|| std::env::var("HOME").ok().map(|h| std::path::Path::new(&h).join(".config")))
		.map(|p| p.join("nexpack"))
}

fn wildmatch(pattern: &str, text: &str) -> bool {
	let pattern_chars: Vec<char> = pattern.chars().collect();
	let text_chars: Vec<char> = text.chars().collect();
	wildmatch_recursive(&pattern_chars, &text_chars)
}

fn wildmatch_recursive(p: &[char], t: &[char]) -> bool {
	match (p.first(), t.first()) {
		(None, None) => true,
		(None, _) => false,
		(Some('*'), _) => {
			let rest = &p[1..];
			if rest.is_empty() {
				return true;
			}
			for i in 0..=t.len() {
				if wildmatch_recursive(rest, &t[i..]) {
					return true;
				}
			}
			false
		}
		(Some(&pc), None) => pc == '*',
		(Some(&pc), Some(&tc)) if pc == tc || pc == '?' => wildmatch_recursive(&p[1..], &t[1..]),
		_ => false,
	}
}
