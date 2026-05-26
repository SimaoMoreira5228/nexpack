use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PermissionSet {
	pub network: PermissionValue,
	pub filesystem: Vec<String>,
	pub devices: Vec<String>,
	pub ipc: Vec<String>,
	pub display: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(untagged)]
pub enum PermissionValue {
	Bool(bool),
	Patterns(Vec<String>),
}

impl PermissionSet {
	pub fn default_sandboxed() -> Self {
		Self {
			network: PermissionValue::Bool(false),
			filesystem: Vec::new(),
			devices: Vec::new(),
			ipc: vec!["wayland".into(), "dbus-session".into()],
			display: "wayland".into(),
		}
	}

	pub fn default_permissive() -> Self {
		Self {
			network: PermissionValue::Bool(true),
			filesystem: vec!["$HOME".into()],
			devices: vec!["dri".into(), "audio".into(), "input".into()],
			ipc: vec!["dbus-session".into(), "wayland".into(), "x11".into()],
			display: "wayland".into(),
		}
	}
}
