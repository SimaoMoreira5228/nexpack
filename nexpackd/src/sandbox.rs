use nexpack_core::permission::{PermissionSet, PermissionValue};
use std::ffi::OsString;
use std::os::unix::process::CommandExt;
use std::process::Command;

pub fn build_bwrap_args(rootfs: &str, perm: &PermissionSet, entrypoint: &str, app_args: &[String]) -> Vec<OsString> {
	let mut args: Vec<OsString> = Vec::new();

	args.push("--ro-bind".into());
	args.push(rootfs.into());
	args.push("/".into());
	args.push("--proc".into());
	args.push("/proc".into());
	args.push("--dev".into());
	args.push("/dev".into());
	args.push("--tmpfs".into());
	args.push("/tmp".into());
	args.push("--tmpfs".into());
	args.push("/run".into());

	for path in &perm.filesystem {
		let expanded = shellexpand(path);
		args.push("--bind".into());
		args.push(expanded.clone().into());
		args.push(expanded.into());
	}

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
		}
		_ => {}
	}

	for var in &["HOME", "USER", "LANG", "LC_ALL", "PATH", "TERM"] {
		if let Ok(val) = std::env::var(var) {
			args.push("--setenv".into());
			args.push((*var).into());
			args.push(val.into());
		}
	}

	args.push("--".into());
	args.push(entrypoint.into());
	for a in app_args {
		args.push(a.into());
	}

	args
}

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
