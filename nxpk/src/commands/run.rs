use nexpack_core::{Bundle, Verifier};
use std::os::unix::io::AsRawFd;
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
		anyhow::bail!("nexpackd not running. Start with: nexpackd &");
	}

	let request = serde_json::json!({
		"method": "mount",
		"bundle": bundle_path,
		"offline": offline,
	});

	let response = send_ipc(&socket_path, &request)?;

	let status = response.get("status").and_then(|v| v.as_str()).unwrap_or("error");
	if status != "mounted" {
		anyhow::bail!("daemon mount failed: {}", response);
	}

	let rootfs = response
		.get("rootfs")
		.and_then(|v| v.as_str())
		.ok_or_else(|| anyhow::anyhow!("no rootfs in response"))?;

	let entrypoint = bundle.header.entrypoint.clone();

	let use_sandbox = sandbox.unwrap_or_else(|| response.get("bwrap_args").is_some());

	if use_sandbox {
		let bwrap_args_val = response
			.get("bwrap_args")
			.and_then(|v| v.as_array())
			.ok_or_else(|| anyhow::anyhow!("daemon did not provide sandbox args. Use --no-sandbox to run unsandboxed."))?;

		eprintln!("Launching sandboxed: {} (via bwrap)", entrypoint);

		let bwrap_path = find_bwrap()?;
		let mut cmd = Command::new(&bwrap_path);

		
		let mut seccomp_fd: Option<i32> = None;
		if let Some(filter_b64) = response.get("seccomp_filter").and_then(|v| v.as_str()) {
			seccomp_fd = Some(setup_seccomp_fd(filter_b64)?);
		}

		for arg in bwrap_args_val {
			cmd.arg(arg.as_str().unwrap_or_default());
		}

		if let Some(fd) = seccomp_fd {
			cmd.arg("--seccomp");
			cmd.arg(fd.to_string());
		}

		for a in args {
			cmd.arg(a);
		}

		let err = cmd.exec();
		anyhow::bail!("failed to exec bwrap: {}", err);
	}

	eprintln!("Launching: {} in {} (no sandbox)", entrypoint, rootfs);

	if !args.is_empty() {
		eprintln!("With args: {:?}", args);
	}

	Ok(())
}

fn setup_seccomp_fd(filter_b64: &str) -> anyhow::Result<i32> {
	let filter = base64_decode(filter_b64)?;

	
	let tmp_path = std::env::temp_dir().join(format!("nexpack-seccomp-{}", std::process::id()));
	std::fs::write(&tmp_path, &filter)?;
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

fn base64_decode(input: &str) -> anyhow::Result<Vec<u8>> {
	
	const DECODE: [i8; 256] = {
		let mut table = [-1i8; 256];
		let chars = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
		let mut i = 0;
		while i < chars.len() {
			table[chars[i] as usize] = i as i8;
			i += 1;
		}
		table
	};

	let bytes: Vec<u8> = input.bytes().filter(|&b| b != b'=' && b != b'\n').collect();
	let mut out = Vec::with_capacity(bytes.len() / 4 * 3);
	for chunk in bytes.chunks(4) {
		if chunk.len() < 4 {
			break;
		}
		let a = DECODE[chunk[0] as usize];
		let b = DECODE[chunk[1] as usize];
		let c = DECODE[chunk[2] as usize];
		let d = DECODE[chunk[3] as usize];
		if a < 0 || b < 0 || c < 0 || d < 0 {
			anyhow::bail!("invalid base64 character");
		}
		let triple = ((a as u32) << 18) | ((b as u32) << 12) | ((c as u32) << 6) | (d as u32);
		out.push((triple >> 16) as u8);
		if chunk.len() > 2 {
			out.push((triple >> 8) as u8);
		}
		if chunk.len() > 3 {
			out.push(triple as u8);
		}
	}
	Ok(out)
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

fn send_ipc(socket_path: &std::path::Path, request: &serde_json::Value) -> anyhow::Result<serde_json::Value> {
	use std::io::{BufRead, Write};
	use std::os::unix::net::UnixStream;

	let mut stream = UnixStream::connect(socket_path)?;
	let mut req_str = serde_json::to_string(request)?;
	req_str.push('\n');
	stream.write_all(req_str.as_bytes())?;

	let mut reader = std::io::BufReader::new(stream);
	let mut line = String::new();
	reader.read_line(&mut line)?;

	Ok(serde_json::from_str(&line)?)
}
