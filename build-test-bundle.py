#!/usr/bin/env python3
import struct
import sys
import os
import subprocess
import tempfile
import shutil

MAGIC = b"NX01"


def blake3_hash(data: bytes) -> str:
    result = subprocess.run(["b3sum", "--no-names"], input=data, capture_output=True)
    if result.returncode != 0:
        raise RuntimeError(f"b3sum failed: {result.stderr.decode().strip()}")
    return "blake3:" + result.stdout.decode().strip()


def make_erofs_layer(source_dir: str, output_path: str) -> bytes:
    subprocess.run(
        [
            "mkfs.erofs",
            "-z", "lz4",
            "--ignore-mtime",
            "--force-uid=0",
            "--force-gid=0",
            output_path,
            source_dir,
        ],
        check=True,
        capture_output=True,
    )
    with open(output_path, "rb") as f:
        return f.read()


def cbor_encode(value) -> bytes:
    if value is None:
        return bytes([0xF6])
    if isinstance(value, bool):
        return bytes([0xF5 if value else 0xF4])
    if isinstance(value, int):
        if value < 0:
            raise ValueError("negative integers not supported")
        if value < 24:
            return bytes([value])
        if value < 0x100:
            return bytes([0x18, value])
        if value < 0x10000:
            return struct.pack(">BH", 0x19, value)
        if value < 0x1_0000_0000:
            return struct.pack(">BI", 0x1A, value)
        return struct.pack(">BQ", 0x1B, value)
    if isinstance(value, str):
        data = value.encode("utf-8")
        return _cbor_length_prefix(0x60, data) + data
    if isinstance(value, bytes):
        return _cbor_length_prefix(0x40, value) + value
    if isinstance(value, list):
        header = _cbor_count_prefix(0x80, len(value))
        return header + b"".join(cbor_encode(item) for item in value)
    if isinstance(value, dict):
        items = list(value.items())
        header = _cbor_count_prefix(0xA0, len(items))
        body = b"".join(cbor_encode(k) + cbor_encode(v) for k, v in items)
        return header + body
    raise TypeError(f"unsupported type {type(value)!r}: {value!r}")


def _cbor_length_prefix(major: int, data: bytes) -> bytes:
    n = len(data)
    if n < 24:
        return bytes([major | n])
    if n < 0x100:
        return bytes([major | 0x18, n])
    if n < 0x10000:
        return struct.pack(">BH", major | 0x19, n)
    return struct.pack(">BI", major | 0x1A, n)


def _cbor_count_prefix(major: int, count: int) -> bytes:
    if count < 24:
        return bytes([major | count])
    if count < 0x100:
        return bytes([major | 0x18, count])
    if count < 0x10000:
        return struct.pack(">BH", major | 0x19, count)
    return struct.pack(">BI", major | 0x1A, count)


def encode_header(header: dict) -> bytes:
    try:
        import cbor2
        return MAGIC + cbor2.dumps(header)
    except ImportError:
        pass
    try:
        import cbor
        return MAGIC + cbor.dumps(header)
    except ImportError:
        pass
    return MAGIC + cbor_encode(header)


def build_layer(tmpdir: str) -> tuple[bytes, str]:
    app_dir = os.path.join(tmpdir, "app")
    bin_dir = os.path.join(app_dir, "usr", "bin")
    os.makedirs(bin_dir)

    hello = os.path.join(bin_dir, "hello.sh")
    with open(hello, "w") as f:
        f.write("#!/bin/sh\necho 'Hello from Nexpack!'\n")
    os.chmod(hello, 0o755)

    with open(os.path.join(app_dir, "hello.txt"), "w") as f:
        f.write("Hello, Nexpack world!\n")

    layer_path = os.path.join(tmpdir, "layer.erofs")
    layer_data = make_erofs_layer(app_dir, layer_path)
    return layer_data, blake3_hash(layer_data)


def main():
    output = sys.argv[1] if len(sys.argv) > 1 else "test.nxpk"

    script_dir = os.path.dirname(os.path.abspath(__file__))
    stub_path = os.path.join(script_dir, "stub", "stub")
    if not os.path.isfile(stub_path):
        stub_path = None

    tmpdir = tempfile.mkdtemp(prefix="nexpack-test-")
    try:
        print("Building erofs layer...")
        layer_data, layer_digest = build_layer(tmpdir)

        stub_data = b""
        if stub_path:
            print("Reading stub...")
            with open(stub_path, "rb") as f:
                stub_data = f.read()

        header = {
            "version": 1,
            "app_id": "org.nexpack.test",
            "app_version": "0.1.0",
            "entrypoint": "/usr/bin/hello.sh",
            "layers": [
                {
                    "digest": layer_digest,
                    "size": len(layer_data),
                    "role": "app",
                }
            ],
            "permissions": {
                "network": False,
                "filesystem": [],
                "devices": [],
                "ipc": ["wayland", "dbus-session"],
                "display": "wayland",
            },
            "signature": None,
            "sbom": None,
            "update_url": None,
        }

        print("Encoding header...")
        header_bytes = encode_header(header)

        print(f"Writing {output}...")
        with open(output, "wb") as f:
            f.write(stub_data)
            f.write(header_bytes)
            f.write(layer_data)

        file_size = os.path.getsize(output)
        print(f"\nDone! {output} ({file_size:,} bytes)")
        print(f"  App:    {header['app_id']} v{header['app_version']}")
        print(f"  Entry:  {header['entrypoint']}")
        print(f"  Layers: {len(header['layers'])}")
        for layer in header["layers"]:
            print(f"    {layer['digest'][:50]}...  {layer['size']:,} bytes  role={layer['role']}")
        print(f"\n  nxpk inspect {output}")
        print(f"  nxpk verify  {output}")

    finally:
        shutil.rmtree(tmpdir, ignore_errors=True)


if __name__ == "__main__":
    main()