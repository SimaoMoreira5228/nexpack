use crate::Error;
use std::path::{Path, PathBuf};

pub struct Store {
	root: PathBuf,
}

impl Store {
	pub fn user() -> Result<Self, Error> {
		let home = std::env::var("HOME").map_err(|_| Error::StorePath("$HOME not set".into()))?;
		let root = PathBuf::from(home).join(".local/share/nexpack/store");
		Ok(Self { root })
	}

	pub fn nexpack_home() -> Result<PathBuf, Error> {
		crate::nexpack_home()
	}

	pub fn bin_dir() -> Result<PathBuf, Error> {
		crate::nexpack_bin_dir()
	}

	pub fn open(root: impl Into<PathBuf>) -> Self {
		Self { root: root.into() }
	}

	pub fn root(&self) -> &Path {
		&self.root
	}

	pub fn layers_dir(&self) -> PathBuf {
		self.root.join("layers")
	}

	pub fn layer_dir(&self, digest_hex: &str) -> PathBuf {
		self.layers_dir().join(format!("blake3-{}", digest_hex))
	}

	pub fn layer_image(&self, digest_hex: &str) -> PathBuf {
		self.layer_dir(digest_hex).join("image.erofs")
	}

	pub fn layer_mount(&self, digest_hex: &str) -> PathBuf {
		self.layer_dir(digest_hex).join("mnt")
	}

	pub fn apps_dir(&self) -> PathBuf {
		self.root.join("apps")
	}

	pub fn gc_roots_dir(&self) -> PathBuf {
		self.root.join("gc-roots")
	}

	pub fn has_layer(&self, digest_hex: &str) -> bool {
		self.layer_image(digest_hex).exists()
	}

	pub fn store_layer(&self, data: &[u8], digest_hex: &str) -> Result<PathBuf, Error> {
		let dir = self.layer_dir(digest_hex);
		std::fs::create_dir_all(&dir).map_err(|e| Error::Io {
			context: format!("creating layer dir {}", dir.display()),
			source: e,
		})?;

		let image_path = dir.join("image.erofs");
		std::fs::write(&image_path, data).map_err(|e| Error::Io {
			context: format!("writing layer image {}", image_path.display()),
			source: e,
		})?;

		Ok(image_path)
	}

	pub fn register_app(&self, app_id: &str, current_digest: &str, meta: &[u8]) -> Result<PathBuf, Error> {
		let app_dir = self.apps_dir().join(app_id);
		std::fs::create_dir_all(&app_dir).map_err(|e| Error::Io {
			context: format!("creating app dir {}", app_dir.display()),
			source: e,
		})?;

		std::fs::write(app_dir.join("meta.capnp"), meta).map_err(|e| Error::Io {
			context: "writing meta.capnp".into(),
			source: e,
		})?;

		let target = format!("../../../layers/blake3-{}/image.erofs", current_digest);
		let link = app_dir.join("current");
		let _ = std::fs::remove_file(&link);
		std::os::unix::fs::symlink(&target, &link).map_err(|e| Error::Io {
			context: format!("creating symlink {}", link.display()),
			source: e,
		})?;

		Ok(app_dir)
	}

	pub fn remove_app(&self, app_id: &str) -> Result<(), Error> {
		let app_dir = self.apps_dir().join(app_id);
		if app_dir.exists() {
			std::fs::remove_dir_all(&app_dir).map_err(|e| Error::Io {
				context: format!("removing app dir {}", app_dir.display()),
				source: e,
			})?;
		}
		Ok(())
	}

	pub fn list_apps(&self) -> Result<Vec<String>, Error> {
		let apps_dir = self.apps_dir();
		if !apps_dir.exists() {
			return Ok(Vec::new());
		}
		let mut apps = Vec::new();
		for entry in std::fs::read_dir(&apps_dir).map_err(|e| Error::Io {
			context: format!("reading apps dir {}", apps_dir.display()),
			source: e,
		})? {
			let entry = entry.map_err(|e| Error::Io {
				context: "reading app entry".into(),
				source: e,
			})?;
			if entry.file_type().map(|t| t.is_dir()).unwrap_or(false) {
				if let Ok(name) = entry.file_name().into_string() {
					apps.push(name);
				}
			}
		}
		apps.sort();
		Ok(apps)
	}

	pub fn add_gc_root(&self, name: &str, digest_hex: &str) -> Result<(), Error> {
		let roots_dir = self.gc_roots_dir();
		std::fs::create_dir_all(&roots_dir).map_err(|e| Error::Io {
			context: format!("creating gc-roots dir {}", roots_dir.display()),
			source: e,
		})?;

		let target = format!("../layers/blake3-{}", digest_hex);
		let link = roots_dir.join(name);
		let _ = std::fs::remove_file(&link);
		std::os::unix::fs::symlink(&target, &link).map_err(|e| Error::Io {
			context: format!("creating gc root symlink {}", link.display()),
			source: e,
		})?;
		Ok(())
	}

	pub fn gc(&self) -> Result<u64, Error> {
		let layers_dir = self.layers_dir();
		if !layers_dir.exists() {
			return Ok(0);
		}

		let roots_dir = self.gc_roots_dir();
		let mut reachable = std::collections::HashSet::new();
		if roots_dir.exists() {
			for entry in std::fs::read_dir(&roots_dir).map_err(|e| Error::Io {
				context: "reading gc-roots".into(),
				source: e,
			})? {
				let entry = entry.map_err(|e| Error::Io {
					context: "reading gc-root entry".into(),
					source: e,
				})?;
				let path = std::fs::read_link(entry.path()).ok();
				if let Some(target) = path {
					if let Some(name) = target.file_name() {
						if let Some(s) = name.to_str() {
							reachable.insert(s.to_string());
						}
					}
				}
			}
		}

		for app in self.list_apps()? {
			let app_dir = self.apps_dir().join(&app);
			let cur = app_dir.join("current");
			if let Ok(target) = std::fs::read_link(&cur) {
				if let Some(name) = target.file_name() {
					if let Some(s) = name.to_str() {
						reachable.insert(s.to_string());
					}
				}
			}
		}

		let mut removed = 0u64;
		for entry in std::fs::read_dir(&layers_dir).map_err(|e| Error::Io {
			context: "reading layers dir".into(),
			source: e,
		})? {
			let entry = entry.map_err(|e| Error::Io {
				context: "reading layer entry".into(),
				source: e,
			})?;
			let name = entry.file_name();
			let name_str = name.to_string_lossy().to_string();
			if !reachable.contains(&name_str) {
				let path = entry.path();

				let mnt = path.join("mnt");
				if mnt.exists() {
					let _ = std::process::Command::new("umount").arg(&mnt).output();
				}
				std::fs::remove_dir_all(&path).ok();
				removed += 1;
			}
		}

		Ok(removed)
	}
}
