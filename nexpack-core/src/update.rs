use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UpdateFeed {
	pub app_id: String,
	pub latest: String,
	pub releases: Vec<ReleaseInfo>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ReleaseInfo {
	pub version: String,
	pub layers: Vec<LayerInfo>,
	pub signature_bundle: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LayerInfo {
	pub digest: String,
	pub url: String,
	pub size: u64,
}

impl UpdateFeed {
	pub fn find_release(&self, version: &str) -> Option<&ReleaseInfo> {
		self.releases.iter().find(|r| r.version == version)
	}

	pub fn latest_release(&self) -> Option<&ReleaseInfo> {
		self.find_release(&self.latest)
	}
}

impl ReleaseInfo {
	pub fn missing_layers(&self, store: &crate::Store) -> Vec<&LayerInfo> {
		self.layers
			.iter()
			.filter(|l| {
				let hex = l.digest.strip_prefix("blake3:").unwrap_or(&l.digest);
				!store.has_layer(hex)
			})
			.collect()
	}
}
