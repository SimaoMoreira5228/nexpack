use crate::error::Error;
use crate::layer::LayerRef;
use crate::permission::{PermissionSet, PermissionValue};
use capnp::serialize;
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
	pub const MAGIC: &'static [u8; 4] = b"NX02";

	pub fn parse(data: &[u8]) -> Result<Self, Error> {
		let magic_pos = data
			.windows(4)
			.position(|w| w == Self::MAGIC)
			.ok_or_else(|| Error::InvalidFormat("NX02 magic not found".into()))?;

		let offset = magic_pos as u64;
		let after_magic = &data[magic_pos + 4..];

		let mut cursor = std::io::Cursor::new(after_magic);
		let message_reader = serialize::read_message(&mut cursor, capnp::message::ReaderOptions::default())
			.map_err(|e| Error::InvalidFormat(format!("Cap'n Proto parse: {}", e)))?;

		let header_reader = message_reader
			.get_root::<nexpack_ipc::header_capnp::bundle_header::Reader>()
			.map_err(|e| Error::InvalidFormat(format!("Cap'n Proto root: {}", e)))?;

		let encoded_len = (magic_pos + 4 + cursor.position() as usize) as u64;

		let version = header_reader.get_version();
		let app_id = header_reader.get_app_id().map_err(|e| Error::InvalidFormat(e.to_string()))?;
		let app_id = app_id.to_str().map_err(|e| Error::InvalidFormat(e.to_string()))?.to_string();
		let app_version = header_reader
			.get_app_version()
			.map_err(|e| Error::InvalidFormat(e.to_string()))?;
		let app_version = app_version
			.to_str()
			.map_err(|e| Error::InvalidFormat(e.to_string()))?
			.to_string();
		let entrypoint = header_reader
			.get_entrypoint()
			.map_err(|e| Error::InvalidFormat(e.to_string()))?;
		let entrypoint = entrypoint
			.to_str()
			.map_err(|e| Error::InvalidFormat(e.to_string()))?
			.to_string();

		let layers_reader = header_reader.get_layers().map_err(|e| Error::InvalidFormat(e.to_string()))?;
		let mut layers = Vec::with_capacity(layers_reader.len() as usize);
		for lr in layers_reader.iter() {
			let digest = lr.get_digest().map_err(|e| Error::InvalidFormat(e.to_string()))?;
			let role = lr.get_role().map_err(|e| Error::InvalidFormat(e.to_string()))?;
			layers.push(LayerRef {
				digest: digest.to_str().map_err(|e| Error::InvalidFormat(e.to_string()))?.to_string(),
				size: lr.get_size(),
				role: role.to_str().map_err(|e| Error::InvalidFormat(e.to_string()))?.to_string(),
			});
		}

		let perm_reader = header_reader
			.get_permissions()
			.map_err(|e| Error::InvalidFormat(e.to_string()))?;
		let network = {
			let nw = perm_reader.get_network().map_err(|e| Error::InvalidFormat(e.to_string()))?;
			match nw.which().map_err(|e| Error::InvalidFormat(e.to_string()))? {
				nexpack_ipc::header_capnp::permission_value::Bool(b) => PermissionValue::Bool(b),
				nexpack_ipc::header_capnp::permission_value::Patterns(p) => {
					let p = p.map_err(|e| Error::InvalidFormat(e.to_string()))?;
					PermissionValue::Patterns(
						p.iter()
							.filter_map(|s| s.ok().and_then(|s| s.to_str().ok().map(|s| s.to_string())))
							.collect(),
					)
				}
			}
		};
		let filesystem: Vec<String> = perm_reader
			.get_filesystem()
			.map_err(|e| Error::InvalidFormat(e.to_string()))?
			.iter()
			.filter_map(|s| s.ok().and_then(|s| s.to_str().ok().map(|s| s.to_string())))
			.collect();
		let devices: Vec<String> = perm_reader
			.get_devices()
			.map_err(|e| Error::InvalidFormat(e.to_string()))?
			.iter()
			.filter_map(|s| s.ok().and_then(|s| s.to_str().ok().map(|s| s.to_string())))
			.collect();
		let ipc: Vec<String> = perm_reader
			.get_ipc()
			.map_err(|e| Error::InvalidFormat(e.to_string()))?
			.iter()
			.filter_map(|s| s.ok().and_then(|s| s.to_str().ok().map(|s| s.to_string())))
			.collect();
		let display = perm_reader.get_display().map_err(|e| Error::InvalidFormat(e.to_string()))?;
		let display = display.to_str().map_err(|e| Error::InvalidFormat(e.to_string()))?.to_string();
		let permissions = PermissionSet {
			network,
			filesystem,
			devices,
			ipc,
			display,
		};

		let sig_data = header_reader
			.get_signature()
			.map_err(|e| Error::InvalidFormat(e.to_string()))?;
		let signature = if sig_data.is_empty() { None } else { Some(sig_data.to_vec()) };

		let sbom_data = header_reader.get_sbom().map_err(|e| Error::InvalidFormat(e.to_string()))?;
		let sbom = if sbom_data.is_empty() {
			None
		} else {
			Some(sbom_data.to_vec())
		};

		let url_text = header_reader
			.get_update_url()
			.map_err(|e| Error::InvalidFormat(e.to_string()))?;
		let url_str = url_text.to_str().unwrap_or("");
		let update_url = if url_str.is_empty() { None } else { Some(url_str.to_string()) };

		let bs = header_reader.get_bootstrap_size();
		let bootstrap_size = if bs == 0 { None } else { Some(bs) };

		Ok(BundleHeader {
			version,
			app_id,
			app_version,
			entrypoint,
			layers,
			permissions,
			signature,
			sbom,
			update_url,
			bootstrap_size,
			offset,
			encoded_len,
		})
	}

	pub fn encode(&self) -> Result<Vec<u8>, Error> {
		use capnp::message::Builder;

		let mut msg = Builder::new_default();
		{
			let mut header = msg.init_root::<nexpack_ipc::header_capnp::bundle_header::Builder>();
			header.set_version(self.version);
			header.set_app_id(&self.app_id);
			header.set_app_version(&self.app_version);
			header.set_entrypoint(&self.entrypoint);

			{
				let mut layers = header.reborrow().init_layers(self.layers.len() as u32);
				for (i, lr) in self.layers.iter().enumerate() {
					let mut l = layers.reborrow().get(i as u32);
					l.set_digest(&lr.digest);
					l.set_size(lr.size);
					l.set_role(&lr.role);
				}
			}

			{
				let mut perm = header.reborrow().init_permissions();
				match &self.permissions.network {
					PermissionValue::Bool(b) => {
						let mut nw = perm.reborrow().init_network();
						nw.set_bool(*b);
					}
					PermissionValue::Patterns(pats) => {
						let mut nw = perm.reborrow().init_network();
						let mut list = nw.reborrow().init_patterns(pats.len() as u32);
						for (i, p) in pats.iter().enumerate() {
							list.set(i as u32, p);
						}
					}
				}
				{
					let mut list = perm.reborrow().init_filesystem(self.permissions.filesystem.len() as u32);
					for (i, v) in self.permissions.filesystem.iter().enumerate() {
						list.set(i as u32, v);
					}
				}
				{
					let mut list = perm.reborrow().init_devices(self.permissions.devices.len() as u32);
					for (i, v) in self.permissions.devices.iter().enumerate() {
						list.set(i as u32, v);
					}
				}
				{
					let mut list = perm.reborrow().init_ipc(self.permissions.ipc.len() as u32);
					for (i, v) in self.permissions.ipc.iter().enumerate() {
						list.set(i as u32, v);
					}
				}
				perm.set_display(&self.permissions.display);
			}

			header.set_signature(self.signature.as_deref().unwrap_or(&[]));
			header.set_sbom(self.sbom.as_deref().unwrap_or(&[]));
			header.set_update_url(self.update_url.as_deref().unwrap_or(""));
			header.set_bootstrap_size(self.bootstrap_size.unwrap_or(0));
		}

		let mut buf = Vec::new();
		buf.extend_from_slice(Self::MAGIC);
		serialize::write_message(&mut buf, &msg).map_err(|e| Error::InvalidFormat(format!("Cap'n Proto encode: {}", e)))?;

		Ok(buf)
	}
}
