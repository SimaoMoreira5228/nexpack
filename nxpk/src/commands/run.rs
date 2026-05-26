use nexpack_core::{Bundle, Verifier};
use std::os::unix::process::CommandExt;
use std::process::Command;

pub fn run(bundle_path: &str, args: &[String]) -> anyhow::Result<()> {
	let bundle = Bundle::open(bundle_path)?;

	eprintln!("App:      {} v{}", bundle.header.app_id, bundle.header.app_version);
	eprintln!("Entry:    {}", bundle.header.entrypoint);
	eprintln!("Layers:   {}", bundle.header.layers.len());
	Verifier::verify_layers(&bundle)?;

	Verifier::verify_signature(&bundle)?;

	let runtime_dir = std::env::var("XDG_RUNTIME_DIR").unwrap_or_else(|_| "/tmp".into());
	let socket_path = std::path::PathBuf::from(&runtime_dir).join("nexpack.sock");

	if !socket_path.exists() {
		anyhow::bail!("nexpackd not running. Start with: nexpackd &");
	}

	let request = serde_json::json!({
		"method": "mount",
		"bundle": bundle_path,
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

	if let Some(bwrap_args) = response.get("bwrap_args").and_then(|v| v.as_array()) {
		eprintln!("Launching sandboxed: {} (via bwrap)", entrypoint);

		let bwrap_path = find_bwrap()?;
		let mut cmd = Command::new(&bwrap_path);

		for arg in bwrap_args {
			cmd.arg(arg.as_str().unwrap_or_default());
		}

		for a in args {
			cmd.arg(a);
		}

		let err = cmd.exec();
		anyhow::bail!("failed to exec bwrap: {}", err);
	}

	eprintln!("Launching: {} in {} (no sandbox)", entrypoint, rootfs);
	eprintln!("(add --sandbox flag to enable bwrap)");

	if !args.is_empty() {
		eprintln!("With args: {:?}", args);
	}

	Ok(())
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
