fn main() {
	capnpc::CompilerCommand::new()
		.file("src/ipc.capnp")
		.run()
		.expect("capnpc failed to compile IPC schema");
}
