use nexpack_core::{Bundle, Verifier};
use std::os::fd::AsRawFd;
use std::os::unix::process::CommandExt;
use std::process::Command;

pub fn run(bundle_path: &str, args: &[String], sandbox: Option<bool>, offline: bool) -> anyhow::Result<()> {
	let bundle = Bundle::open(bundle_path)?;

	eprintln!("App:      {} v{}", bundle.header.app_id, bundle.header.app_version);
	eprintln!("Entry:    {}", bundle.header.entrypoint);
	eprintln!("Layers:   {}", bundle.header.layers.len());
	Verifier::verify_layers(&bundle)?;

	match Verifier::verify_signature_opt(&bundle, offline) {
		Ok(()) => eprintln!("Signature: OK{}", if offline { " (offline mode)" } else { "" }),
		Err(e) => eprintln!("Signature: {}", e),
	}

	if bundle.header.signature.is_some() {
		if let Ok(trust) = nexpack_core::TrustConfig::load() {
			if let Some((_pattern, entry)) = trust.match_policy(&bundle.header.app_id) {
				if let Some(identity) = entry.identities.first() {
					eprintln!("Trust policy: requiring identity \"{}\"", identity);
				}
			}
		}
	}

	let runtime_dir = std::env::var("XDG_RUNTIME_DIR").unwrap_or_else(|_| "/tmp".into());
	let socket_path = std::path::PathBuf::from(&runtime_dir).join("nexpack.sock");

	if !socket_path.exists() {
		eprintln!("nexpackd not running, starting it...");
		start_daemon()?;
		wait_for_socket(&socket_path, std::time::Duration::from_secs(5))
			.map_err(|_| anyhow::anyhow!("nexpackd failed to start within 5 seconds. Try: nexpackd &"))?;
	}

	let stream = std::os::unix::net::UnixStream::connect(&socket_path)
		.map_err(|_| anyhow::anyhow!("failed to connect to nexpackd at {}", socket_path.display()))?;
	let mut reader = std::io::BufReader::new(&stream);
	let mut writer = &stream;

	{
		let mut msg = capnp::message::Builder::new_default();
		let mut req = msg.init_root::<nexpack_ipc::ipc_capnp::request::Builder>();
		let mut mount_req = req.init_mount();
		mount_req.set_bundle(bundle_path);
		mount_req.set_offline(offline);
		capnp::serialize::write_message(&mut writer, &msg)?;
	}

	let response_reader = capnp::serialize::read_message(&mut reader, capnp::message::ReaderOptions::default())?;
	let response = response_reader.get_root::<nexpack_ipc::ipc_capnp::response::Reader>()?;

	let mount_resp = match response.which()? {
		nexpack_ipc::ipc_capnp::response::Mount(m) => m?,
		nexpack_ipc::ipc_capnp::response::Error(e) => anyhow::bail!("daemon error: {}", e?.get_message()?.to_str()?),
		_ => anyhow::bail!("unexpected daemon response"),
	};

	let status = mount_resp.get_status()?.to_str()?;
	if status != "mounted" {
		anyhow::bail!("daemon mount failed: {}", status);
	}

	let rootfs = mount_resp.get_rootfs()?.to_str()?;
	let entrypoint = bundle.header.entrypoint.clone();

	let bwrap_args_val: Vec<String> = mount_resp
		.get_bwrap_args()?
		.iter()
		.filter_map(|s| s.ok())
		.flat_map(|s| s.to_str())
		.map(|s| s.to_string())
		.collect();

	let seccomp_filter_data = mount_resp.get_seccomp_filter()?.to_vec();

	let use_sandbox = sandbox.unwrap_or(false);

	if use_sandbox {
		eprintln!("Launching sandboxed: {} (via bwrap)", entrypoint);

		let bwrap_path = find_bwrap()?;
		let mut cmd = Command::new(&bwrap_path);

		let mut seccomp_fd: Option<i32> = None;
		if !seccomp_filter_data.is_empty() {
			seccomp_fd = Some(setup_seccomp_fd(&seccomp_filter_data)?);
		}

		let mut seccomp_inserted = false;
		for arg in &bwrap_args_val {
			if arg == "--" && !seccomp_inserted {
				if let Some(fd) = seccomp_fd {
					cmd.arg("--seccomp");
					cmd.arg(fd.to_string());
				}
				seccomp_inserted = true;
			}
			cmd.arg(&arg);
		}

		for a in args {
			cmd.arg(a);
		}

		let err = cmd.exec();
		anyhow::bail!("failed to exec bwrap: {}", err);
	}

	let app_path = std::path::PathBuf::from(rootfs).join(entrypoint.strip_prefix("/").unwrap_or(&entrypoint));
	eprintln!("Launching: {} (no sandbox)", app_path.display());

	if !std::fs::metadata(&app_path).map(|m| m.is_file()).unwrap_or(false) {
		anyhow::bail!("entrypoint not found at {}", app_path.display());
	}

	let err = Command::new(&app_path).args(args).exec();
	anyhow::bail!("failed to exec {}: {}", app_path.display(), err);
}

fn setup_seccomp_fd(filter: &[u8]) -> anyhow::Result<i32> {
	let tmp_path = std::env::temp_dir().join(format!("nexpack-seccomp-{}", std::process::id()));
	std::fs::write(&tmp_path, filter)?;
	let file = std::fs::File::open(&tmp_path)?;
	let _ = std::fs::remove_file(&tmp_path);

	let fd = file.as_raw_fd();

	unsafe {
		let ret = libc::fcntl(fd, libc::F_SETFD, 0);
		if ret < 0 {
			anyhow::bail!("failed to clear CLOEXEC on seccomp fd: {}", std::io::Error::last_os_error());
		}
	}

	std::mem::forget(file);

	Ok(fd)
}

fn find_bwrap() -> anyhow::Result<String> {
	for dir in std::env::split_paths(&std::env::var_os("PATH").unwrap_or_default()) {
		let candidate = dir.join("bwrap");
		if candidate.is_file() {
			return Ok(candidate.to_string_lossy().to_string());
		}
	}
	anyhow::bail!("bwrap not found in PATH. Install bubblewrap.")
}

fn start_daemon() -> anyhow::Result<()> {
	let nexpackd_path = find_nexpackd_in_path()?;

	let mut cmd = Command::new(&nexpackd_path);
	cmd.stdout(std::process::Stdio::null());
	cmd.stderr(std::process::Stdio::null());
	cmd.stdin(std::process::Stdio::null());

	unsafe {
		cmd.pre_exec(|| {
			libc::setsid();
			Ok(())
		});
	}

	cmd.spawn().map_err(|e| anyhow::anyhow!("failed to start nexpackd: {}", e))?;
	Ok(())
}

fn wait_for_socket(path: &std::path::Path, timeout: std::time::Duration) -> anyhow::Result<()> {
	let start = std::time::Instant::now();
	while start.elapsed() < timeout {
		if path.exists() {
			return Ok(());
		}
		std::thread::sleep(std::time::Duration::from_millis(100));
	}
	anyhow::bail!("socket not ready after {:?}", timeout)
}

fn find_nexpackd_in_path() -> anyhow::Result<String> {
	for dir in std::env::split_paths(&std::env::var_os("PATH").unwrap_or_default()) {
		let candidate = dir.join("nexpackd");
		if candidate.is_file() {
			return Ok(candidate.to_string_lossy().to_string());
		}
	}
	if let Ok(exe) = std::env::current_exe() {
		if let Some(parent) = exe.parent() {
			let sibling = parent.join("nexpackd");
			if sibling.is_file() {
				return Ok(sibling.to_string_lossy().to_string());
			}
		}
	}
	anyhow::bail!("nexpackd not found in PATH or next to nxpk binary. Start it manually: nexpackd &")
}
