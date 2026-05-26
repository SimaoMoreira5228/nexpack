use std::fs;
use std::path::{Path, PathBuf};

pub fn trust(app_id_pattern: &str, identity: &str) -> anyhow::Result<()> {
	let config_dir = dirs().ok_or_else(|| anyhow::anyhow!("cannot determine config directory"))?;
	fs::create_dir_all(&config_dir)?;

	let trust_file = config_dir.join("trust.toml");

	let mut content = String::new();
	if trust_file.exists() {
		content = fs::read_to_string(&trust_file)?;
	}

	content.push_str(&format!(
		r#"
[policy."{}"]
identities = ["{}"]
require_rekor = true
"#,
		app_id_pattern, identity
	));

	fs::write(&trust_file, &content)?;
	println!("Trust policy added: {} -> {}", app_id_pattern, identity);
	println!("  Config: {}", trust_file.display());

	Ok(())
}

fn dirs() -> Option<std::path::PathBuf> {
	std::env::var("XDG_CONFIG_HOME")
		.ok()
		.map(PathBuf::from)
		.or_else(|| std::env::var("HOME").ok().map(|h| Path::new(&h).join(".config")))
		.map(|p| p.join("nexpack"))
}
