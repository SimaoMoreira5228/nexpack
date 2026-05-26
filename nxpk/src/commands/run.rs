use nexpack_core::{Bundle, Verifier};

pub fn run(bundle_path: &str, args: &[String]) -> anyhow::Result<()> {
	let bundle = Bundle::open(bundle_path)?;

	println!("App:      {} v{}", bundle.header.app_id, bundle.header.app_version);
	println!("Entry:    {}", bundle.header.entrypoint);
	println!("Layers:   {}", bundle.header.layers.len());
	println!("Verifying layers...");
	Verifier::verify_layers(&bundle)?;
	println!("  BLAKE3 digests OK");

	Verifier::verify_signature(&bundle)?;

	let runtime_dir = std::env::var("XDG_RUNTIME_DIR").unwrap_or_else(|_| "/tmp".into());
	let socket_path = std::path::PathBuf::from(&runtime_dir).join("nexpack.sock");

	if socket_path.exists() {
		let request = serde_json::json!({
			"method": "mount",
			"bundle": bundle_path,
		});

		let response = send_ipc(&socket_path, &request)?;
		println!(
			"Daemon:   {}",
			response.get("status").and_then(|v| v.as_str()).unwrap_or("unknown")
		);

		if let Some(rootfs) = response.get("rootfs").and_then(|v| v.as_str()) {
			let entrypoint = bundle.header.entrypoint.clone();
			println!("Launching: {} in {}", entrypoint, rootfs);
			println!("(sandbox exec is stubbed — in v0.4, this will exec bwrap)");
		}
	} else {
		println!("nexpackd not running — direct execution mode");
		println!("\nTo start the daemon:");
		println!("  nexpackd &\n");

		println!("Would mount overlayfs at /run/nexpack/{}/rootfs", bundle.header.app_id);
		println!("Would exec: {}", bundle.header.entrypoint);
		if !args.is_empty() {
			println!("With args: {:?}", args);
		}
	}

	Ok(())
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
