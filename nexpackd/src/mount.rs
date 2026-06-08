use anyhow::Context;
use nexpack_core::Bundle;
use std::path::{Path, PathBuf};

pub fn build_overlay(app_id: &str, bundle: &Bundle, store: &nexpack_core::Store) -> anyhow::Result<PathBuf> {
	let runtime_dir = std::env::var("XDG_RUNTIME_DIR").unwrap_or_else(|_| "/tmp".into());
	let run_dir = PathBuf::from(&runtime_dir).join("nexpack").join(app_id);

	let mut lower_dirs: Vec<PathBuf> = Vec::new();
	for layer in &bundle.header.layers {
		let digest_hex = layer.digest_hex();
		let layer_path = store.layer_image(digest_hex);
		let mount_point = store.layer_mount(digest_hex);

		if !mount_point.exists() {
			std::fs::create_dir_all(&mount_point).context(format!("creating mount point {}", mount_point.display()))?;
		}

		if !is_path_mounted(&mount_point) && layer_path.exists() {
			mount_erofs(&layer_path, &mount_point)?;
		}

		lower_dirs.push(mount_point);
	}

	let upper_dir = run_dir.join("upper");
	let work_dir = run_dir.join("work");
	let merged_dir = run_dir.join("rootfs");

	std::fs::create_dir_all(&upper_dir).context(format!("creating upper dir {}", upper_dir.display()))?;
	std::fs::create_dir_all(&work_dir).context(format!("creating work dir {}", work_dir.display()))?;
	std::fs::create_dir_all(&merged_dir).context(format!("creating merged dir {}", merged_dir.display()))?;

	let overlay_ok = try_overlay_mount(&lower_dirs, &upper_dir, &work_dir, &merged_dir);
	if !overlay_ok {
		try_fuse_overlay_mount(&lower_dirs, &upper_dir, &work_dir, &merged_dir)?;
	}

	prepare_rootfs(&merged_dir)?;

	tracing::info!(
		"mounted {} overlay: {} lower layers -> {}",
		app_id,
		lower_dirs.len(),
		merged_dir.display()
	);

	Ok(merged_dir)
}

fn prepare_rootfs(rootfs: &Path) -> anyhow::Result<()> {
	for dir in &["proc", "dev", "sys", "tmp", "run"] {
		let p = rootfs.join(dir);
		if !p.exists() {
			std::fs::create_dir_all(&p).with_context(|| format!("creating {dir} in rootfs"))?;
		}
	}

	// /dev/pts is needed by bwrap's --dev for the devpts mount
	let devpts = rootfs.join("dev").join("pts");
	if !devpts.exists() {
		std::fs::create_dir_all(&devpts).context("creating /dev/pts in rootfs")?;
	}

	Ok(())
}

fn mount_erofs(image: &Path, mount_point: &Path) -> anyhow::Result<()> {
	let kernel_ok = std::process::Command::new("mount")
		.args([
			"-t",
			"erofs",
			"-o",
			"loop,ro",
			&image.to_string_lossy(),
			&mount_point.to_string_lossy(),
		])
		.status()
		.map(|s| s.success())
		.unwrap_or(false);

	if kernel_ok {
		return Ok(());
	}

	let erofsfuse = find_binary("erofsfuse")?;
	let status = std::process::Command::new(&erofsfuse)
		.arg(image.as_os_str())
		.arg(mount_point.as_os_str())
		.status()
		.context("running erofsfuse")?;

	if !status.success() {
		anyhow::bail!("erofsfuse mount failed (exit: {:?})", status.code());
	}

	tracing::info!("mounted {} via erofsfuse", image.display());
	Ok(())
}

fn try_overlay_mount(lower_dirs: &[PathBuf], upper: &Path, work: &Path, merged: &Path) -> bool {
	let lowerdir_str: String = lower_dirs
		.iter()
		.rev()
		.map(|p| p.to_string_lossy().to_string())
		.collect::<Vec<_>>()
		.join(":");

	std::process::Command::new("mount")
		.args([
			"-t",
			"overlay",
			"overlay",
			"-o",
			&format!(
				"lowerdir={},upperdir={},workdir={}",
				lowerdir_str,
				upper.display(),
				work.display()
			),
			&merged.to_string_lossy(),
		])
		.status()
		.map(|s| s.success())
		.unwrap_or(false)
}

fn try_fuse_overlay_mount(lower_dirs: &[PathBuf], upper: &Path, work: &Path, merged: &Path) -> anyhow::Result<()> {
	let fuse_ovl = find_binary("fuse-overlayfs")?;

	let lowerdir_str: String = lower_dirs
		.iter()
		.rev()
		.map(|p| p.to_string_lossy().to_string())
		.collect::<Vec<_>>()
		.join(":");

	let status = std::process::Command::new(&fuse_ovl)
		.args([
			"-o",
			&format!("lowerdir={}", lowerdir_str),
			"-o",
			&format!("upperdir={}", upper.display()),
			"-o",
			&format!("workdir={}", work.display()),
			&merged.to_string_lossy(),
		])
		.status()
		.context("running fuse-overlayfs")?;

	if !status.success() {
		anyhow::bail!("fuse-overlayfs mount failed (exit: {:?})", status.code());
	}

	tracing::info!("mounted overlay via fuse-overlayfs");
	Ok(())
}

fn find_binary(name: &str) -> anyhow::Result<String> {
	for dir in std::env::split_paths(&std::env::var_os("PATH").unwrap_or_default()) {
		let candidate = dir.join(name);
		if candidate.is_file() {
			return Ok(candidate.to_string_lossy().to_string());
		}
	}

	if let Ok(entries) = std::fs::read_dir("/nix/store") {
		for entry in entries.flatten() {
			let p = entry.path().join("bin").join(name);
			if p.is_file() {
				return Ok(p.to_string_lossy().to_string());
			}
		}
	}

	let libexec = PathBuf::from("/usr/libexec").join(name);
	if libexec.is_file() {
		return Ok(libexec.to_string_lossy().to_string());
	}

	anyhow::bail!("{} not found. Install appropriate package", name);
}

fn unmount_path(merged: &Path) -> anyhow::Result<()> {
	for cmd in &["fusermount3", "fusermount", "umount"] {
		let status = std::process::Command::new(cmd)
			.arg("-u")
			.arg(merged)
			.status()
			.unwrap_or_default();
		if status.success() {
			return Ok(());
		}
	}
	anyhow::bail!("failed to unmount {}", merged.display())
}

pub fn unmount_overlay(merged: &Path) -> anyhow::Result<()> {
	unmount_path(merged)?;

	if let Some(run_dir) = merged.parent() {
		let _ = std::fs::remove_dir_all(run_dir);
	}

	tracing::info!("unmounted {}", merged.display());
	Ok(())
}

fn is_path_mounted(path: &Path) -> bool {
	std::process::Command::new("mountpoint")
		.arg("-q")
		.arg(path)
		.status()
		.map(|s| s.success())
		.unwrap_or(false)
}
