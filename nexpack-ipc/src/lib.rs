pub mod ipc_capnp {
	include!(concat!(env!("OUT_DIR"), "/src/ipc_capnp.rs"));
}

pub mod header_capnp {
	include!(concat!(env!("OUT_DIR"), "/src/header_capnp.rs"));
}
