@0x9a8b7c6d5e4f3a2b;

struct MountRequest {
	bundle @0 :Text;
	offline @1 :Bool = false;
}

struct MountResponse {
	status @0 :Text;
	appId @1 :Text;
	rootfs @2 :Text;
	entrypoint @3 :Text;
	bwrapArgs @4 :List(Text);
	seccompFilter @5 :Data;
}

struct UnmountRequest {
	appId @0 :Text;
}

struct UnmountResponse {
	status @0 :Text;
}

struct ListRequest {}

struct ListResponse {
	apps @0 :List(AppInfo);
}

struct StatusRequest {}

struct StatusResponse {
	version @0 :Text;
	uptimeSeconds @1 :Float64;
	mountedApps @2 :List(AppInfo);
}

struct AppInfo {
	appId @0 :Text;
	rootfs @1 :Text;
	entrypoint @2 :Text;
}

struct ErrorResponse {
	message @0 :Text;
}

struct Request {
	union {
		mount @0 :MountRequest;
		unmount @1 :UnmountRequest;
		list @2 :ListRequest;
		status @3 :StatusRequest;
	}
}

struct Response {
	union {
		mount @0 :MountResponse;
		unmount @1 :UnmountResponse;
		list @2 :ListResponse;
		status @3 :StatusResponse;
		error @4 :ErrorResponse;
	}
}
