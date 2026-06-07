# Nexpack -- Modern Linux Portable Apps

Nexpack is a portable application format for Linux that replaces AppImage. It lets you distribute a single `.nxpk` file that users download and run, while getting proper sandboxing, delta updates, layer deduplication, and enforced signing.

## Quick Start

```bash
# Build a bundle from a spec file
nxpk pack spec.toml

# Run it directly
nxpk run myapp.nxpk

# Install to the local store
nxpk install myapp.nxpk

# Run from the store later
nxpk run io.example.myapp

# Check for updates (if the bundle declared an update_url)
nxpk update io.example.myapp

# Remove it
nxpk remove io.example.myapp

# Garbage collect unused layers
nxpk gc
```

## Packing a Bundle

`nxpk pack` takes a `spec.toml` that describes your app. Here is a minimal one:

```toml
[app]
id = "io.helix-editor.helix"
version = "25.03"
entrypoint = "/usr/bin/hx"

[[layer]]
role = "app"
source = "./staging"
```

The `source` directory is what gets baked into the erofs layer. Everything inside `source/` becomes the app's root filesystem. `entrypoint` is the path inside that rootfs that gets executed.

If you want sandbox permissions you add a `[permissions]` section:

```toml
[app]
id = "io.helix-editor.helix"
version = "25.03"
entrypoint = "/usr/bin/hx"

[permissions]
network = false
filesystem = ["$HOME/.config/helix"]
display = "wayland"

[[layer]]
role = "app"
source = "./staging"
```

If you want the bundle to self-bootstrap (so it runs without `nexpackd` or `nxpk` pre-installed) you embed static binaries:

```toml
[bootstrap]
nexpackd = "./target/release/nexpackd"
nxpk = "./target/release/nxpk"
```

The user can then `chmod +x myapp.nxpk && ./myapp.nxpk` and it works with zero setup. The embedded binaries get extracted on first run.

## Layers

You can split your app into multiple layers. Common use: one layer for the runtime (like Electron or Qt) shared across multiple apps, and one layer for the app itself.

```toml
[[layer]]
role = "runtime"
source = "./staging-runtime"

[[layer]]
role = "app"
source = "./staging-app"
```

Layers are content-addressed by their blake3 hash. If two bundles share a layer it is stored once on disk.

## Signing

Sign a bundle after packing it:

```bash
nxpk sign myapp.nxpk
```

This calls `cosign sign-blob` under the hood, producing a sigstore bundle embedded in the header. Verify with:

```bash
nxpk verify myapp.nxpk
```

## AppImage Compat

Run an AppImage under the Nexpack sandbox:

```bash
nxpk compat run some-app.appimage
```

Convert an AppImage to a `.nxpk`:

```bash
nxpk compat convert some-app.appimage
```

The converter does a heuristic permission scan and spits out a `spec.toml` you can edit before repacking.

## Daemon

`nexpackd` runs as a per-user daemon. It manages layer mounts, handles IPC from `nxpk` and the ELF stub, and checks for updates in the background.

It starts automatically when you run `nxpk`. You can also start it manually:

```bash
nexpackd &
```

Configuration lives in `~/.config/nexpack/daemon.toml`:

```toml
idle_timeout = 300
update_interval = 3600
```

## Directory Structure

```text
~/.local/share/nexpack/
  store/
    layers/blake3-<hex>/
      image.erofs
      mnt/
    apps/<app-id>/
      current -> ../../../layers/...
      meta.capnp
    gc-roots/
```

## Building from Source

You need Rust, the capnp protobuf compiler, and `erofs-utils`.

```bash
nix develop      # or add capnp and erofs-utils to your system
cargo build
cargo build -r   # release build for the bootstrap binaries
```

The ELF stub is built separately:

```bash
make -C stub
```

## The Project

Four crates:

- `nexpack-core`: types, bundle parsing, store, verifier
- `nexpackd`: the daemon
- `nxpk`: the cli
- `nexpack-ipc`: capnp schema for ipc and header format

The ELF stub is in `stub/stub.c` -- ~150 lines of freestanding C, direct syscalls, no libc.

## Releases

Current release: [v0.1.0](https://github.com/SimaoMoreira5228/nexpack/releases/tag/v0.1.0)

Downloads:
- [nxpk-linux-x86_64](https://github.com/SimaoMoreira5228/nexpack/releases/latest/download/nxpk-linux-x86_64)
- [nexpackd-linux-x86_64](https://github.com/SimaoMoreira5228/nexpack/releases/latest/download/nexpackd-linux-x86_64)
- [nexpack-source.tar.gz](https://github.com/SimaoMoreira5228/nexpack/releases/latest/download/nexpack-source.tar.gz)
- [sha256sums.txt](https://github.com/SimaoMoreira5228/nexpack/releases/latest/download/sha256sums.txt)

This section is updated by the release workflow. The downloads link to the latest release via `/latest/download/` urls, so they always point to whatever the current version is.
