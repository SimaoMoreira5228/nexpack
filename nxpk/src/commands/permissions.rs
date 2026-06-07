use nexpack_core::Store;
use nexpack_core::permission::PermissionValue;
use std::io::{self, Write};

pub fn permissions(app_id: &str, edit: bool) -> anyhow::Result<()> {
	let store = Store::user()?;
	let app_dir = store.apps_dir().join(app_id);
	let meta_path = app_dir.join("meta.cbor");

	if !meta_path.exists() {
		anyhow::bail!("app '{}' is not installed", app_id);
	}

	let meta = std::fs::read(&meta_path)?;
	let header: nexpack_core::BundleHeader =
		nexpack_core::BundleHeader::parse(&meta).map_err(|e| anyhow::anyhow!("header decode: {}", e))?;

	println!("Permissions for: {}", app_id);
	println!("  network:    {:?}", header.permissions.network);
	println!("  filesystem: {:?}", header.permissions.filesystem);
	println!("  devices:    {:?}", header.permissions.devices);
	println!("  ipc:        {:?}", header.permissions.ipc);
	println!("  display:    {:?}", header.permissions.display);

	if edit {
		edit_permissions(app_id, &meta_path, &header)?;
	}

	Ok(())
}

fn edit_permissions(app_id: &str, meta_path: &std::path::Path, header: &nexpack_core::BundleHeader) -> anyhow::Result<()> {
	let mut perm = header.permissions.clone();
	let stdin = io::stdin();
	let mut stdout = io::stdout();

	loop {
		println!("\n--- Edit permissions for {} ---", app_id);
		println!("  1) network:    {}", fmt_perm_value(&perm.network));
		println!("  2) filesystem: {:?}", perm.filesystem);
		println!("  3) devices:    {:?}", perm.devices);
		println!("  4) ipc:        {:?}", perm.ipc);
		println!("  5) display:    {}", perm.display);
		println!("  6) Save & exit");
		print!("Choose (1-6): ");
		stdout.flush()?;

		let mut choice = String::new();
		stdin.read_line(&mut choice)?;

		match choice.trim() {
			"1" => {
				print!("network (true/false): ");
				stdout.flush()?;
				let mut val = String::new();
				stdin.read_line(&mut val)?;
				perm.network = match val.trim() {
					"true" => PermissionValue::Bool(true),
					_ => PermissionValue::Bool(false),
				};
			}
			"2" => {
				println!("filesystem paths (empty line to finish):");
				let mut paths = Vec::new();
				loop {
					print!("  path: ");
					stdout.flush()?;
					let mut p = String::new();
					stdin.read_line(&mut p)?;
					let p = p.trim().to_string();
					if p.is_empty() {
						break;
					}
					paths.push(p);
				}
				perm.filesystem = paths;
			}
			"3" => {
				println!("devices (empty line to finish):");
				let mut devs = Vec::new();
				loop {
					print!("  device (dri/audio/input): ");
					stdout.flush()?;
					let mut d = String::new();
					stdin.read_line(&mut d)?;
					let d = d.trim().to_string();
					if d.is_empty() {
						break;
					}
					devs.push(d);
				}
				perm.devices = devs;
			}
			"4" => {
				println!("IPC protocols (empty line to finish):");
				let mut protos = Vec::new();
				loop {
					print!("  protocol (wayland/x11/dbus-session): ");
					stdout.flush()?;
					let mut p = String::new();
					stdin.read_line(&mut p)?;
					let p = p.trim().to_string();
					if p.is_empty() {
						break;
					}
					protos.push(p);
				}
				perm.ipc = protos;
			}
			"5" => {
				print!("display (wayland/x11/headless): ");
				stdout.flush()?;
				let mut val = String::new();
				stdin.read_line(&mut val)?;
				let val = val.trim().to_string();
				if !val.is_empty() {
					perm.display = val;
				}
			}
			"6" => {
				let mut new_header = header.clone();
				new_header.permissions = perm;
				let encoded = new_header.encode()?;
				std::fs::write(meta_path, &encoded)?;
				println!("Permissions saved.");
				break;
			}
			_ => {
				println!("Invalid choice.");
			}
		}
	}

	Ok(())
}

fn fmt_perm_value(v: &PermissionValue) -> String {
	match v {
		PermissionValue::Bool(b) => format!("{}", b),
		PermissionValue::Patterns(p) => format!("{:?}", p),
	}
}
