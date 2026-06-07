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

pub fn edit_trust() -> anyhow::Result<()> {
	let config_dir = dirs().ok_or_else(|| anyhow::anyhow!("cannot determine config directory"))?;
	std::fs::create_dir_all(&config_dir)?;

	let trust_file = config_dir.join("trust.toml");

	if !trust_file.exists() {
		let template = r#"# Nexpack Trust Policy
# Format:
#   [policy."app.id.pattern.*"]
#   identities = ["identity@issuer"]
#   require_rekor = true
#
# Use wildcard * to match multiple apps.
# The default policy is used when no specific match is found.

[policy.default]
action = "prompt"

"#;
		std::fs::write(&trust_file, template)?;
	}

	let editor = std::env::var("EDITOR")
		.or_else(|_| std::env::var("VISUAL"))
		.unwrap_or_else(|_| "nano".to_string());

	let status = std::process::Command::new(&editor)
		.arg(&trust_file)
		.status()
		.map_err(|e| anyhow::anyhow!("failed to launch editor '{}': {}", editor, e))?;

	if !status.success() {
		anyhow::bail!("editor exited with code {:?}", status.code());
	}

	let content = std::fs::read_to_string(&trust_file)?;

	if !content.trim().is_empty() {
		let _config: nexpack_core::TrustConfig =
			toml::from_str(&content).map_err(|e| anyhow::anyhow!("invalid trust.toml: {}", e))?;
		println!("Trust policy updated: {} is valid", trust_file.display());
	} else {
		println!("Trust policy file is empty: {}", trust_file.display());
	}

	Ok(())
}

fn dirs() -> Option<std::path::PathBuf> {
	std::env::var("XDG_CONFIG_HOME")
		.ok()
		.map(PathBuf::from)
		.or_else(|| std::env::var("HOME").ok().map(|h| Path::new(&h).join(".config")))
		.map(|p| p.join("nexpack"))
}
