use nexpack_core::permission::{PermissionSet, PermissionValue};
use std::ffi::OsString;
use std::os::unix::process::CommandExt;
use std::process::Command;

pub fn build_bwrap_args(rootfs: &str, perm: &PermissionSet, entrypoint: &str, app_args: &[String]) -> Vec<OsString> {
	let mut args: Vec<OsString> = Vec::new();

	args.push("--proc".into());
	args.push("/proc".into());
	args.push("--dev".into());
	args.push("/dev".into());
	args.push("--tmpfs".into());
	args.push("/tmp".into());
	args.push("--tmpfs".into());
	args.push("/run".into());
	args.push("--ro-bind".into());
	args.push(rootfs.into());
	args.push("/run/app".into());

	for sys_path in ["/usr", "/lib", "/lib64", "/bin", "/sbin", "/etc", "/nix/store"] {
		if std::path::Path::new(sys_path).exists() {
			args.push("--ro-bind".into());
			args.push(sys_path.into());
			args.push(sys_path.into());
		}
	}

	let adjusted_entrypoint = format!("/run/app{}", entrypoint);

	for path in &perm.filesystem {
		let expanded = shellexpand(path);
		args.push("--bind".into());
		args.push(expanded.clone().into());
		args.push(expanded.into());
	}

	args.push("--unshare-pid".into());
	args.push("--unshare-uts".into());
	args.push("--unshare-ipc".into());
	args.push("--die-with-parent".into());

	args.push("--unshare-user".into());
	args.push("--uid".into());
	args.push("0".into());
	args.push("--gid".into());
	args.push("0".into());

	let has_network = match &perm.network {
		PermissionValue::Bool(b) => *b,
		PermissionValue::Patterns(_) => true,
	};
	if !has_network {
		args.push("--unshare-net".into());
	}

	for dev in &perm.devices {
		let path = match dev.as_str() {
			"dri" => "/dev/dri",
			"audio" => "/dev/snd",
			"input" => "/dev/input",
			_ => continue,
		};
		if std::path::Path::new(path).exists() {
			args.push("--ro-bind".into());
			args.push(path.into());
			args.push(path.into());
		}
	}

	for ipc in &perm.ipc {
		match ipc.as_str() {
			"wayland" => {
				if let Ok(runtime) = std::env::var("XDG_RUNTIME_DIR") {
					let wayland_socket = format!("{}/wayland-0", runtime);
					if std::path::Path::new(&wayland_socket).exists() {
						args.push("--ro-bind".into());
						args.push(wayland_socket.clone().into());
						args.push(wayland_socket.into());
					}
				}
			}
			"x11" => {
				args.push("--ro-bind".into());
				args.push("/tmp/.X11-unix".into());
				args.push("/tmp/.X11-unix".into());
				if let Ok(disp) = std::env::var("DISPLAY") {
					args.push("--setenv".into());
					args.push("DISPLAY".into());
					args.push(disp.into());
				}
			}
			"dbus-session" => {
				if let Ok(runtime) = std::env::var("XDG_RUNTIME_DIR") {
					let bus = format!("{}/bus", runtime);
					if std::path::Path::new(&bus).exists() {
						args.push("--ro-bind".into());
						args.push(bus.clone().into());
						args.push(bus.into());
					}
				}
				if let Ok(addr) = std::env::var("DBUS_SESSION_BUS_ADDRESS") {
					args.push("--setenv".into());
					args.push("DBUS_SESSION_BUS_ADDRESS".into());
					args.push(addr.into());
				}
			}
			_ => {}
		}
	}

	if let Ok(runtime) = std::env::var("XDG_RUNTIME_DIR") {
		let portal_path = format!("{}/doc/by-app", runtime);
		if std::path::Path::new(&portal_path).exists() {
			args.push("--ro-bind".into());
			args.push(portal_path.clone().into());
			args.push(portal_path.into());
		}
	}

	match perm.display.as_str() {
		"wayland" => {
			args.push("--setenv".into());
			args.push("WAYLAND_DISPLAY".into());
			args.push("wayland-0".into());
			args.push("--setenv".into());
			args.push("XDG_SESSION_TYPE".into());
			args.push("wayland".into());
		}
		"x11" => {
			if let Ok(disp) = std::env::var("DISPLAY") {
				args.push("--setenv".into());
				args.push("DISPLAY".into());
				args.push(disp.into());
			}
			if let Ok(xa) = std::env::var("XAUTHORITY") {
				args.push("--ro-bind-try".into());
				args.push(xa.clone().into());
				args.push(xa.into());
			}
		}
		_ => {}
	}

	for var in &["HOME", "USER", "LANG", "LC_ALL", "PATH", "TERM", "XAUTHORITY"] {
		if let Ok(val) = std::env::var(var) {
			args.push("--setenv".into());
			args.push((*var).into());
			args.push(val.into());
		}
	}

	args.push("--".into());
	args.push(adjusted_entrypoint.into());
	for a in app_args {
		args.push(a.into());
	}

	args
}

pub fn generate_seccomp_filter(perm: &PermissionSet) -> Vec<u8> {
	let has_network = match &perm.network {
		PermissionValue::Bool(b) => *b,
		PermissionValue::Patterns(_) => true,
	};

	if has_network {
		return seccomp_allow_all();
	}

	block_network_syscalls()
}

fn seccomp_allow_all() -> Vec<u8> {
	let mut prog = Vec::new();

	prog.extend_from_slice(&bpf_stmt(BPF_RET | BPF_K, SECCOMP_RET_ALLOW));
	prog
}

fn block_network_syscalls() -> Vec<u8> {
	const BLACKLIST: &[u32] = &[41, 42, 43, 44, 45, 46, 47, 48, 49, 50, 51, 52, 53, 54, 57, 288];

	let mut prog = Vec::new();

	prog.extend_from_slice(&bpf_stmt(BPF_LD | BPF_W | BPF_ABS, 4));

	prog.extend_from_slice(&bpf_jump(BPF_JMP | BPF_JEQ | BPF_K, AUDIT_ARCH_X86_64, 1, 0));

	prog.extend_from_slice(&bpf_stmt(BPF_RET | BPF_K, SECCOMP_RET_KILL));

	prog.extend_from_slice(&bpf_stmt(BPF_LD | BPF_W | BPF_ABS, 0));

	for &sysno in BLACKLIST {
		prog.extend_from_slice(&bpf_jump(BPF_JMP | BPF_JEQ | BPF_K, sysno, 0, 1));
		prog.extend_from_slice(&bpf_stmt(BPF_RET | BPF_K, SECCOMP_RET_ERRNO | EPERM));
	}

	prog.extend_from_slice(&bpf_stmt(BPF_RET | BPF_K, SECCOMP_RET_ALLOW));

	prog
}

const BPF_LD: u16 = 0x00;
const BPF_JMP: u16 = 0x05;
const BPF_RET: u16 = 0x06;
const BPF_W: u16 = 0x00;
const BPF_ABS: u16 = 0x20;
const BPF_JEQ: u16 = 0x10;
const BPF_K: u16 = 0x00;

const SECCOMP_RET_KILL: u32 = 0x00000000;
const SECCOMP_RET_ERRNO: u32 = 0x00050000;
const SECCOMP_RET_ALLOW: u32 = 0x7fff0000;

const EPERM: u32 = 1;

const AUDIT_ARCH_X86_64: u32 = 0xC000003Eu32;

fn bpf_stmt(code: u16, k: u32) -> [u8; 8] {
	let mut buf = [0u8; 8];
	buf[0..2].copy_from_slice(&code.to_le_bytes());
	buf[2] = 0;
	buf[3] = 0;
	buf[4..8].copy_from_slice(&k.to_le_bytes());
	buf
}

fn bpf_jump(code: u16, k: u32, jt: u8, jf: u8) -> [u8; 8] {
	let mut buf = [0u8; 8];
	buf[0..2].copy_from_slice(&code.to_le_bytes());
	buf[2] = jt;
	buf[3] = jf;
	buf[4..8].copy_from_slice(&k.to_le_bytes());
	buf
}

#[allow(dead_code)]
pub fn exec_sandbox(rootfs: &str, perm: &PermissionSet, entrypoint: &str, app_args: &[String]) -> std::io::Error {
	let args = build_bwrap_args(rootfs, perm, entrypoint, app_args);

	let mut cmd = Command::new("bwrap");
	cmd.args(&args);

	tracing::info!("executing bwrap sandbox: {} {}", entrypoint, app_args.join(" "));
	cmd.exec()
}

fn shellexpand(s: &str) -> String {
	let mut out = String::new();
	let chars: Vec<char> = s.chars().collect();
	let mut i = 0;
	while i < chars.len() {
		if chars[i] == '~' && (i + 1 >= chars.len() || chars[i + 1] == '/') {
			if let Ok(home) = std::env::var("HOME") {
				out.push_str(&home);
			}
			i += 1;
		} else if chars[i] == '$' {
			let mut varname = String::new();
			i += 1;
			while i < chars.len() && (chars[i].is_alphanumeric() || chars[i] == '_') {
				varname.push(chars[i]);
				i += 1;
			}
			if let Ok(val) = std::env::var(&varname) {
				out.push_str(&val);
			}
		} else {
			out.push(chars[i]);
			i += 1;
		}
	}
	out
}
