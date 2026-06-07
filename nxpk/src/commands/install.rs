use nexpack_core::{Bundle, Store, Verifier};
use std::path::PathBuf;

pub fn install(bundle_path: &str) -> anyhow::Result<()> {
	let bundle = Bundle::open(bundle_path)?;
	let store = Store::user()?;

	println!("Installing: {} v{}", bundle.header.app_id, bundle.header.app_version);

	for (i, layer) in bundle.header.layers.iter().enumerate() {
		let digest_hex = layer.digest_hex();
		if store.has_layer(digest_hex) {
			println!(
				"  layer [{:02}] {} — already present, skipping",
				i,
				digest_hex.chars().take(16).collect::<String>()
			);
			continue;
		}

		let data = bundle.extract_layer(i)?;
		Verifier::verify_digest(data, digest_hex)?;
		store.store_layer(data, digest_hex)?;
		println!(
			"  layer [{:02}] {} — stored",
			i,
			digest_hex.chars().take(16).collect::<String>()
		);
	}

	let current_digest = bundle.header.layers.last().map(|l| l.digest_hex()).unwrap_or("");

	let meta = bundle.header.encode()?;
	store.register_app(&bundle.header.app_id, current_digest, &meta)?;
	store.add_gc_root(&bundle.header.app_id, current_digest)?;

	println!("\nInstalled: {}", bundle.header.app_id);
	println!("  Layers in store: {}", bundle.header.layers.len());
	println!("  Run with: nxpk run {}", bundle_path);

	create_desktop_entry(&bundle)?;

	Ok(())
}

fn create_desktop_entry(bundle: &Bundle) -> anyhow::Result<()> {
	let app_id = &bundle.header.app_id;
	let name = app_id.rsplit('.').next().unwrap_or(app_id);

	let data_home = std::env::var("XDG_DATA_HOME").map(PathBuf::from).unwrap_or_else(|_| {
		let home = std::env::var("HOME").unwrap_or_else(|_| "/tmp".into());
		PathBuf::from(home).join(".local").join("share")
	});

	let apps_dir = data_home.join("applications");
	std::fs::create_dir_all(&apps_dir)?;

	let bundle_path = find_bundle_path(app_id);
	let desktop_path = apps_dir.join(format!("{}.desktop", app_id));

	let content = format!(
		"[Desktop Entry]\n\
		 Type=Application\n\
		 Name={}\n\
		 Comment=Nexpack application\n\
		 Exec=nxpk run {} %%F\n\
		 Terminal=false\n\
		 Categories=Utility;\n\
		 X-Nexpack-AppId={}\n",
		name, bundle_path, app_id,
	);

	std::fs::write(&desktop_path, &content)?;
	println!("  Desktop entry: {}", desktop_path.display());
	Ok(())
}

fn find_bundle_path(app_id: &str) -> String {
	let store_path = format!("~/.local/share/nexpack/apps/{}/current", app_id);
	let expanded = shellexpand(&store_path);
	if std::path::Path::new(&expanded).exists() {
		return expanded;
	}

	app_id.to_string()
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
		} else {
			out.push(chars[i]);
			i += 1;
		}
	}
	out
}
