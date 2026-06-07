use std::path::PathBuf;

pub fn search(query: &str) -> anyhow::Result<()> {
	let query_lower = query.to_lowercase();
	let mut found_local = false;

	if let Ok(store) = nexpack_core::Store::user() {
		if let Ok(apps) = store.list_apps() {
			let matching: Vec<String> = apps
				.into_iter()
				.filter(|id| id.to_lowercase().contains(&query_lower))
				.collect();

			if !matching.is_empty() {
				found_local = true;
				println!("Local installed apps matching '{}':", query);
				for app_id in &matching {
					let meta = load_app_meta(&store, app_id);
					match meta {
						Some(ver) => println!("  {} v{}  (installed)", app_id, ver),
						None => println!("  {}  (installed)", app_id),
					}
				}
			}
		}
	}

	let registry_url = load_registry_url();
	if let Some(url) = registry_url {
		println!("\nQuerying registry: {}", url);
		let full_url = format!("{}/api/v1/search?q={}", url, urlencoding(query));
		match ureq::get(&full_url).call() {
			Ok(resp) if resp.status() == 200 => match resp.into_json::<RegistryResults>() {
				Ok(results) => {
					if results.apps.is_empty() {
						println!("  No remote results");
					} else {
						println!("\nRemote apps matching '{}':", query);
						for app in &results.apps {
							println!("  {} v{}", app.app_id, app.version);
							if let Some(ref desc) = app.description {
								println!("    {}", desc);
							}
						}
					}
				}
				Err(e) => println!("  Error parsing registry response: {}", e),
			},
			Ok(resp) => println!("  Registry returned HTTP {}", resp.status()),
			Err(e) => println!("  Registry query failed: {}", e),
		}
	} else if !found_local {
		println!("No local results for '{}'", query);
		println!("Configure a registry in ~/.config/nexpack/config.toml:");
		println!("  [registry]");
		println!("  url = \"https://registry.example.com\"");
	}

	Ok(())
}

fn load_app_meta(store: &nexpack_core::Store, app_id: &str) -> Option<String> {
	let meta_path = store.apps_dir().join(app_id).join("meta.cbor");
	let data = std::fs::read(meta_path).ok()?;
	let header: nexpack_core::BundleHeader = ciborium::de::from_reader(&data[..]).ok()?;
	Some(header.app_version)
}

fn load_registry_url() -> Option<String> {
	let config_path = config_dir()?.join("config.toml");
	let content = std::fs::read_to_string(config_path).ok()?;
	let config: NexpackConfig = toml::from_str(&content).ok()?;
	config.registry.and_then(|r| r.url)
}

fn config_dir() -> Option<PathBuf> {
	std::env::var("XDG_CONFIG_HOME")
		.ok()
		.map(PathBuf::from)
		.or_else(|| std::env::var("HOME").ok().map(|h| PathBuf::from(h).join(".config")))
		.map(|p| p.join("nexpack"))
}

fn urlencoding(input: &str) -> String {
	let mut out = String::with_capacity(input.len());
	for byte in input.bytes() {
		match byte {
			b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => {
				out.push(byte as char);
			}
			b' ' => out.push_str("%20"),
			_ => {
				out.push_str(&format!("%{:02X}", byte));
			}
		}
	}
	out
}

#[derive(serde::Deserialize)]
struct NexpackConfig {
	registry: Option<RegistryConfig>,
}

#[derive(serde::Deserialize)]
struct RegistryConfig {
	url: Option<String>,
}

#[derive(serde::Deserialize)]
struct RegistryResults {
	apps: Vec<RegistryApp>,
}

#[derive(serde::Deserialize)]
struct RegistryApp {
	app_id: String,
	version: String,
	description: Option<String>,
}
