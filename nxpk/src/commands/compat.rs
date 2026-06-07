use nexpack_core::{BundleHeader, LayerRef, Verifier};
use std::path::{Path, PathBuf};

pub fn run_compat(appimage: &str, app_args: &[String]) -> anyhow::Result<()> {
	let path = Path::new(appimage);
	if !path.is_file() {
		anyhow::bail!("not a file: {}", appimage);
	}

	let data = std::fs::read(path)?;
	if data.len() < 4 || data[..4] != [0x7f, b'E', b'L', b'F'] {
		anyhow::bail!("not an ELF file (not a valid AppImage)");
	}

	let (squashfs_offset, _squashfs_magic) = find_squashfs_offset(&data)?;
	eprintln!("AppImage detected: squashfs at offset {}", squashfs_offset);

	let tmpdir = std::env::temp_dir().join(format!("nexpack-compat-{}", std::process::id()));
	let extract_dir = tmpdir.join("rootfs");
	let _ = std::fs::remove_dir_all(&tmpdir);
	std::fs::create_dir_all(&extract_dir)?;

	eprintln!("Extracting AppImage (this may take a moment)...");
	let status = std::process::Command::new("unsquashfs")
		.arg("-d")
		.arg(&extract_dir)
		.arg("-o")
		.arg(squashfs_offset.to_string())
		.arg(appimage)
		.status()
		.map_err(|e| anyhow::anyhow!("unsquashfs failed: {}", e))?;

	if !status.success() {
		anyhow::bail!("unsquashfs extraction failed (exit code: {:?})", status.code());
	}

	let apprun = find_apprun(&extract_dir);
	let entrypoint = apprun.unwrap_or_else(|| {
		let guess = PathBuf::from("AppRun");
		eprintln!("AppRun not found, trying {}", guess.display());
		guess
	});

	eprintln!("Entrypoint: {}", entrypoint.display());

	let permissions = heuristic_permissions(&extract_dir);
	eprintln!("Permissions: network={}", if permissions.network { "yes" } else { "no" });

	let display = std::env::var("WAYLAND_DISPLAY").ok().map(|_| "wayland").unwrap_or("x11");
	let ipc = if display == "wayland" {
		vec!["wayland".to_string(), "dbus-session".to_string()]
	} else {
		vec!["x11".to_string(), "dbus-session".to_string()]
	};

	let perm_set = nexpack_core::PermissionSet {
		network: nexpack_core::permission::PermissionValue::Bool(permissions.network),
		filesystem: vec!["$HOME".to_string()],
		devices: vec!["dri".to_string(), "audio".to_string()],
		ipc,
		display: display.to_string(),
	};

	let bwrap_args = build_bwrap_args_generic(&extract_dir, &perm_set, entrypoint.to_str().unwrap_or("AppRun"), app_args);

	eprintln!("Launching with bubblewrap...");
	let status = std::process::Command::new("bwrap")
		.args(&bwrap_args)
		.status()
		.map_err(|e| anyhow::anyhow!("bwrap execution failed: {}", e))?;

	if !status.success() {
		let code = status.code().unwrap_or(-1);
		eprintln!("bwrap exited with code {}", code);
	}

	Ok(())
}

struct HeuristicPerms {
	network: bool,
}

fn heuristic_permissions(root: &Path) -> HeuristicPerms {
	let mut network_hints = false;

	if let Ok(entries) = walk_files(root) {
		for entry in entries {
			let name = entry.to_string_lossy();
			let lower = name.to_lowercase();

			if lower.contains("libcurl")
				|| lower.contains("libssl")
				|| lower.contains("libcrypto")
				|| lower.contains("libsoup")
				|| lower.contains("libfetch")
				|| lower.contains("libwslay")
				|| lower.contains("libnghttp2")
			{
				network_hints = true;
			}

			if lower.ends_with("/curl") || lower.ends_with("/wget") || lower.ends_with("/ssh") {
				network_hints = true;
			}
		}
	}

	HeuristicPerms { network: network_hints }
}

fn walk_files(dir: &Path) -> std::io::Result<Vec<PathBuf>> {
	let mut files = Vec::new();
	let mut stack = vec![dir.to_path_buf()];
	while let Some(current) = stack.pop() {
		if let Ok(entries) = std::fs::read_dir(&current) {
			for entry in entries.flatten() {
				let path = entry.path();
				if path.is_dir() {
					stack.push(path);
				} else {
					files.push(path);
				}
			}
		}
	}
	Ok(files)
}

fn find_apprun(root: &Path) -> Option<PathBuf> {
	let candidates = ["AppRun", "apprun", "appimage.sh", "AppImageLauncher"];
	for name in &candidates {
		let p = root.join(name);
		if p.is_file() {
			return Some(p);
		}
	}

	if let Ok(entries) = std::fs::read_dir(root) {
		for entry in entries.flatten() {
			let path = entry.path();
			let name = path.file_name()?.to_string_lossy().to_lowercase();
			if name == "apprun" {
				return Some(path);
			}
		}
	}

	Some(root.join("AppRun"))
}

fn find_squashfs_offset(data: &[u8]) -> anyhow::Result<(u64, [u8; 4])> {
	let magics: &[(&[u8; 4], &str)] = &[
		(b"hsqs", "squashfs (little-endian)"),
		(b"sqsh", "squashfs (big-endian)"),
		(b"qshs", "squashfs (little-endian, swapped)"),
	];

	if data.len() > 8 {
		let possible_iso = u32::from_le_bytes([data[8], data[9], data[10], data[11]]);
		if possible_iso == 0xFFFFFFFF || possible_iso == 0 {
			for i in (0..data.len().saturating_sub(4)).step_by(4) {
				let chunk: [u8; 4] = [data[i], data[i + 1], data[i + 2], data[i + 3]];
				for &magic in &[b"hsqs", b"sqsh", b"qshs"] {
					if chunk == *magic {
						return Ok((i as u64 - 4, chunk));
					}
				}
			}
			for (magic, _) in magics {
				for i in 0..data.len().saturating_sub(4) {
					if data[i..i + 4] == **magic {
						let guess = if i >= 4 { i as u64 - 4 } else { i as u64 };
						return Ok((guess, **magic));
					}
				}
			}
		}
	}

	anyhow::bail!("could not locate squashfs image in AppImage (tried looking for hsqs/sqsh/qshs magic)");
}

fn build_bwrap_args_generic(
	rootfs: &Path,
	perms: &nexpack_core::PermissionSet,
	entrypoint: &str,
	app_args: &[String],
) -> Vec<String> {
	let entrypoint = if entrypoint.starts_with('/') {
		entrypoint.to_string()
	} else {
		format!("/{}", entrypoint)
	};

	let runtime_dir = std::env::var("XDG_RUNTIME_DIR").ok();

	let mut args: Vec<String> = Vec::new();

	args.push("--ro-bind".to_string());
	args.push(format!("{}", rootfs.display()));
	args.push("/".to_string());

	args.push("--proc".to_string());
	args.push("/proc".to_string());
	args.push("--dev".to_string());
	args.push("/dev".to_string());
	args.push("--tmpfs".to_string());
	args.push("/tmp".to_string());

	for path in &perms.filesystem {
		let expanded = expand_path(path);
		if expanded.exists() {
			args.push("--bind".to_string());
			args.push(expanded.to_string_lossy().to_string());
			args.push(expanded.to_string_lossy().to_string());
		}
	}

	if let nexpack_core::permission::PermissionValue::Bool(net) = &perms.network {
		if !net {
			args.push("--unshare-net".to_string());
		}
	}

	args.push("--unshare-pid".to_string());
	args.push("--unshare-uts".to_string());
	args.push("--unshare-ipc".to_string());
	args.push("--die-with-parent".to_string());

	for dev in &perms.devices {
		let dev_path = PathBuf::from("/dev").join(dev);
		if dev_path.exists() {
			args.push("--ro-bind".to_string());
			args.push(dev_path.to_string_lossy().to_string());
			args.push(dev_path.to_string_lossy().to_string());
		}
	}

	if let Some(ref runtime) = runtime_dir {
		let rt = PathBuf::from(runtime);
		if rt.exists() {
			args.push("--ro-bind".to_string());
			args.push(rt.to_string_lossy().to_string());
			args.push(rt.to_string_lossy().to_string());
		}
	}

	if let Some(display) = std::env::var("WAYLAND_DISPLAY").ok() {
		let socket = runtime_dir
			.as_ref()
			.map(|r| PathBuf::from(r).join(&display))
			.unwrap_or_else(|| PathBuf::from(&display));
		if socket.exists() {
			args.push("--ro-bind".to_string());
			args.push(socket.to_string_lossy().to_string());
			args.push(socket.to_string_lossy().to_string());
		}
		args.push("--set-env".to_string());
		args.push("WAYLAND_DISPLAY".to_string());
		args.push(display);
	}

	if let Some(xauth) = std::env::var("XAUTHORITY").ok() {
		let xa = PathBuf::from(&xauth);
		if xa.exists() {
			args.push("--ro-bind".to_string());
			args.push(xa.to_string_lossy().to_string());
			args.push(xa.to_string_lossy().to_string());
		}
	}
	if let Some(host) = std::env::var("DISPLAY").ok() {
		args.push("--set-env".to_string());
		args.push("DISPLAY".to_string());
		args.push(host);
	}

	if let Some(ref runtime) = runtime_dir {
		let dbus_socket = PathBuf::from(runtime).join("bus");
		if dbus_socket.exists() {
			args.push("--ro-bind".to_string());
			args.push(dbus_socket.to_string_lossy().to_string());
			args.push(dbus_socket.to_string_lossy().to_string());
		}
	}

	args.push("--set-env".to_string());
	args.push("HOME".to_string());
	args.push(std::env::var("HOME").unwrap_or_else(|_| "/tmp".into()));

	args.push("--set-env".to_string());
	args.push("PATH".to_string());
	args.push("/usr/bin:/bin:/usr/local/bin".to_string());

	args.push("--".to_string());
	args.push(entrypoint);
	for a in app_args {
		args.push(a.clone());
	}

	args
}

fn expand_path(path: &str) -> PathBuf {
	if path.starts_with("$HOME") {
		let home = std::env::var("HOME").unwrap_or_else(|_| "/tmp".into());
		PathBuf::from(path.replace("$HOME", &home))
	} else if path.starts_with("$XDG_") {
		let parts: Vec<&str> = path.splitn(3, '/').collect();
		if parts.len() >= 2 {
			let var = parts[0];
			let rest = parts.get(1).unwrap_or(&"");
			let val = std::env::var(&var[1..]).unwrap_or_else(|_| format!("/tmp/{}", var));
			PathBuf::from(val).join(rest)
		} else {
			PathBuf::from(path)
		}
	} else {
		PathBuf::from(path)
	}
}

pub fn convert_appimage(appimage: &str, output: Option<&str>) -> anyhow::Result<()> {
	let path = Path::new(appimage);
	if !path.is_file() {
		anyhow::bail!("not a file: {}", appimage);
	}

	let data = std::fs::read(path)?;
	if data.len() < 4 || data[..4] != [0x7f, b'E', b'L', b'F'] {
		anyhow::bail!("not an ELF file (not a valid AppImage)");
	}

	let (squashfs_offset, _) = find_squashfs_offset(&data)?;
	eprintln!("AppImage detected: squashfs at offset {}", squashfs_offset);

	let app_name = path.file_stem().and_then(|s| s.to_str()).unwrap_or("app").to_string();

	let tmpdir = std::env::temp_dir().join(format!("nexpack-convert-{}", std::process::id()));
	let extract_dir = tmpdir.join("rootfs");
	let _ = std::fs::remove_dir_all(&tmpdir);
	std::fs::create_dir_all(&extract_dir)?;

	eprintln!("Extracting AppImage...");
	let status = std::process::Command::new("unsquashfs")
		.arg("-d")
		.arg(&extract_dir)
		.arg("-o")
		.arg(squashfs_offset.to_string())
		.arg(appimage)
		.status()
		.map_err(|e| anyhow::anyhow!("unsquashfs failed: {}", e))?;

	if !status.success() {
		anyhow::bail!("unsquashfs extraction failed (exit code: {:?})", status.code());
	}

	let apprun = find_apprun(&extract_dir)
		.and_then(|p| {
			let rel = p.strip_prefix(&extract_dir).ok()?;
			Some(rel.to_string_lossy().to_string())
		})
		.unwrap_or_else(|| "AppRun".to_string());

	eprintln!("Building erofs layer...");
	let erofs_data = build_erofs_from_dir(&extract_dir)?;
	let hex = Verifier::blake3_hex(&erofs_data);
	let digest = format!("blake3:{}", hex);

	let layer = LayerRef {
		digest,
		size: erofs_data.len() as u64,
		role: "app".to_string(),
	};

	let permissions = heuristic_permissions(&extract_dir);
	let display = std::env::var("WAYLAND_DISPLAY").ok().map(|_| "wayland").unwrap_or("x11");
	let ipc = if display == "wayland" {
		vec!["wayland".to_string(), "dbus-session".to_string()]
	} else {
		vec!["x11".to_string(), "dbus-session".to_string()]
	};

	let perm_set = nexpack_core::PermissionSet {
		network: nexpack_core::permission::PermissionValue::Bool(permissions.network),
		filesystem: vec!["$HOME/.config/$appname".to_string()],
		devices: vec!["dri".to_string(), "audio".to_string()],
		ipc,
		display: display.to_string(),
	};

	let output_name = output
		.map(|o| {
			if o.ends_with(".nxpk") {
				o.to_string()
			} else {
				format!("{}.nxpk", o)
			}
		})
		.unwrap_or_else(|| format!("{}.nxpk", app_name));

	let header = BundleHeader {
		version: 1,
		app_id: format!("compat.{}", app_name),
		app_version: "0.1.0".to_string(),
		entrypoint: apprun,
		layers: vec![layer],
		permissions: perm_set,
		signature: None,
		sbom: None,
		update_url: None,
		offset: 0,
		encoded_len: 0,
	};

	let stub_data = find_stub_binary()?;
	let header_bytes = header.encode()?;

	eprintln!("Writing {}", output_name);
	let mut out = std::fs::File::create(&output_name)?;
	use std::io::Write;
	out.write_all(&stub_data)?;
	out.write_all(&header_bytes)?;
	out.write_all(&erofs_data)?;

	let file_size = std::fs::metadata(&output_name)?.len();
	eprintln!("Done: {} ({})", output_name, format_size(file_size));

	let _ = std::fs::remove_dir_all(&tmpdir);
	Ok(())
}

fn build_erofs_from_dir(source: &Path) -> anyhow::Result<Vec<u8>> {
	use std::process::Command;

	if let Some(path) = find_tool("mkfs.erofs") {
		let tmp = std::env::temp_dir().join(format!("nexpack-erofs-{}.erofs", std::process::id()));
		let status = Command::new(&path)
			.args([
				"-z",
				"lz4",
				"--ignore-mtime",
				"--force-uid=0",
				"--force-gid=0",
				&tmp.to_string_lossy(),
				&source.to_string_lossy(),
			])
			.status()
			.map_err(|e| anyhow::anyhow!("mkfs.erofs failed: {}", e))?;

		if !status.success() {
			anyhow::bail!("mkfs.erofs exited with {}", status.code().unwrap_or(-1));
		}

		let data = std::fs::read(&tmp)?;
		let _ = std::fs::remove_file(&tmp);
		return Ok(data);
	}

	anyhow::bail!("mkfs.erofs not found. Install erofs-utils or run in nix dev shell");
}

fn find_tool(name: &str) -> Option<String> {
	for dir in std::env::split_paths(&std::env::var_os("PATH").unwrap_or_default()) {
		let candidate = dir.join(name);
		if candidate.is_file() {
			return Some(candidate.to_string_lossy().to_string());
		}
	}
	if let Ok(entries) = std::fs::read_dir("/nix/store") {
		for entry in entries.flatten() {
			let p = entry.path().join("bin").join(name);
			if p.is_file() {
				return Some(p.to_string_lossy().to_string());
			}
		}
	}
	None
}

fn find_stub_binary() -> anyhow::Result<Vec<u8>> {
	for loc in &["./stub/stub", "../stub/stub", "/nix/store/*/stub"] {
		if loc.contains('*') {
			if let Ok(entries) = std::fs::read_dir("/nix/store") {
				for entry in entries.flatten() {
					let candidate = entry.path().join("stub");
					if candidate.is_file() {
						return std::fs::read(&candidate).map_err(|e| anyhow::anyhow!("reading stub: {}", e));
					}
				}
			}
			continue;
		}
		let p = Path::new(loc);
		if p.is_file() {
			return std::fs::read(p).map_err(|e| anyhow::anyhow!("reading stub {}: {}", p.display(), e));
		}
	}
	anyhow::bail!("stub not found. Build with: make -C stub");
}

fn format_size(size: u64) -> String {
	const UNITS: &[&str] = &["B", "KB", "MB", "GB"];
	let mut s = size as f64;
	let mut unit_idx = 0;
	while s >= 1024.0 && unit_idx < UNITS.len() - 1 {
		s /= 1024.0;
		unit_idx += 1;
	}
	if unit_idx == 0 {
		format!("{} {}", size, UNITS[unit_idx])
	} else {
		format!("{:.1} {}", s, UNITS[unit_idx])
	}
}
