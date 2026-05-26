pub fn pack(_spec: &str) -> anyhow::Result<()> {
	// TODO: Implement nxpk pack
	println!("nxpk pack: not yet implemented");
	println!("  Would read spec.toml, construct erofs layers,");
	println!("  build CBOR header, concatenate ELF stub, and emit .nxpk");
	Ok(())
}
