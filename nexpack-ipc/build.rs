fn main() {
	capnpc::CompilerCommand::new()
		.file("src/ipc.capnp")
		.file("src/header.capnp")
		.run()
		.expect("capnpc failed to compile IPC/header schema");
}
