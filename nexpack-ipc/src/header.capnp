@0xf1e2d3c4b5a69788;

struct LayerRef {
	digest @0 :Text;
	size @1 :UInt64;
	role @2 :Text;
}

struct PermissionValue {
	union {
		bool @0 :Bool;
		patterns @1 :List(Text);
	}
}

struct PermissionSet {
	network @0 :PermissionValue;
	filesystem @1 :List(Text);
	devices @2 :List(Text);
	ipc @3 :List(Text);
	display @4 :Text;
}

struct BundleHeader {
	version @0 :UInt32;
	appId @1 :Text;
	appVersion @2 :Text;
	entrypoint @3 :Text;
	layers @4 :List(LayerRef);
	permissions @5 :PermissionSet;
	signature @6 :Data;
	sbom @7 :Data;
	updateUrl @8 :Text;
	bootstrapSize @9 :UInt64;
}
