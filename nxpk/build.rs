use std::path::Path;

fn main() {
	let stub_dir = if Path::new("../stub/Makefile").exists() {
		"../stub"
	} else if Path::new("stub/Makefile").exists() {
		"stub"
	} else {
		panic!("stub/Makefile not found -- are you in the nexpack workspace root?");
	};

	let status = std::process::Command::new("make")
		.args(["-C", stub_dir])
		.status()
		.expect("failed to run make for the elf stub");
	assert!(status.success(), "elf stub build failed");

	let stub_bytes = std::fs::read(format!("{}/stub", stub_dir)).expect("stub binary not found after build");

	let out_dir = std::env::var("OUT_DIR").unwrap();
	let code = format!(
		"pub const STUB_BYTES: &[u8] = &{:?};\npub const STUB_SIZE: usize = {};",
		stub_bytes,
		stub_bytes.len(),
	);
	std::fs::write(format!("{}/stub_bytes.rs", out_dir), code).unwrap();

	println!("cargo:rerun-if-changed=../stub/stub.c");
	println!("cargo:rerun-if-changed=../stub/Makefile");
}
