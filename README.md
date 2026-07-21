# Nexpack — Modern Linux Portable Apps

Nexpack is a portable application format for Linux. Distribute a single `.nxpk` file that users can `chmod +x && ./app.nxpk` with zero setup. Sandboxing via bubblewrap is opt-in (`--sandbox`); by default apps run with full filesystem access.

## Quick Start

```bash
# chmod +x && run — no install needed
chmod +x myapp.nxpk && ./myapp.nxpk

# Or with nxpk installed
nxpk run myapp.nxpk
nxpk run myapp.nxpk --sandbox    # enable sandbox
nxpk run myapp.nxpk --no-sandbox # explicit no sandbox (default)

# Install to the local store
nxpk install myapp.nxpk
nxpk run io.example.myapp

# Check for updates (if the bundle declared an update_url)
nxpk update io.example.myapp

# Remove it
nxpk remove io.example.myapp

# Garbage collect unused layers
nxpk gc
```

## Packing a Bundle

`nxpk pack` takes a `spec.toml`. The `source` directory is baked into an erofs layer and becomes the app's root filesystem. `entrypoint` is the path inside that rootfs that gets executed.

Minimal spec:
```toml
[app]
id = "io.helix-editor.helix"
version = "25.03"
entrypoint = "/usr/bin/hx"

[[layer]]
role = "app"
source = "./staging"
```

Apps that need to download or install to arbitrary locations (launchers, game managers, etc.) can run without sandbox — that's the default. Add `[permissions]` to declare what the sandbox should grant when `--sandbox` is used:

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

## Self-Bootstrapping (Recommended)

Embed the nexpack runtime so users don't need anything pre-installed:

```toml
[bootstrap]
nexpackd = "./target/release/nexpackd"
nxpk = "./target/release/nxpk"
```

The user can then:

```bash
chmod +x myapp.nxpk && ./myapp.nxpk
```

On first run, the ELF stub extracts `nexpackd` and `nxpk` to `~/.local/share/nexpack/bin/`, starts the daemon, and launches the app. The extracted binaries are reused on subsequent runs.

## Sandbox

Sandbox is **opt-in**. By default, the app runs with full filesystem access (no bubblewrap, no seccomp). Pass `--sandbox` to enable:

- **Filesystem**: restricted to paths declared in `[permissions]`
- **Network**: blocked or restricted based on the `network` field
- **Namespaces**: PID, UTS, IPC, and User isolation via bubblewrap
- **Seccomp**: blocked network syscalls return `-EPERM` (not SIGSYS)

## Layers

Split your app into multiple layers for deduplication:

```toml
[[layer]]
role = "runtime"
source = "./staging-runtime"

[[layer]]
role = "app"
source = "./staging-app"
```

Layers are content-addressed by their blake3 hash. Shared layers (e.g., Electron, Qt) are stored once on disk across bundles.

## Signing

```bash
nxpk sign myapp.nxpk
nxpk verify myapp.nxpk
```

Calls `cosign sign-blob` under the hood, embedding a Sigstore bundle in the header.

## AppImage Compat

```bash
nxpk compat run some-app.appimage
nxpk compat convert some-app.appimage
```

The converter does a heuristic permission scan and spits out a `spec.toml`.

## Daemon

`nexpackd` runs as a per-user daemon. It starts automatically when needed. Configuration at `~/.config/nexpack/daemon.toml`:

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
  bin/               # extracted bootstrap binaries
    nexpackd
    nxpk
```

## Building from Source

```bash
nix develop      # or add capnp and erofs-utils to your system
cargo build
cargo build -r   # release build for bootstrap binaries
make -C stub     # ELF stub (freestanding C, ~5 KB)
```

## The Project

Four crates:

- `nexpack-core`: types, bundle parsing, store, verifier
- `nexpackd`: the daemon
- `nxpk`: the cli (pack, run, install, sign, etc.)
- `nexpack-ipc`: capnp schema for ipc and header format

The ELF stub is in `stub/stub.c` — ~530 lines of freestanding C, direct syscalls, no libc.

## Releases

Current release: [v0.1.0](https://github.com/SimaoMoreira5228/nexpack/releases/tag/v0.1.0)

Downloads:
- [nxpk-linux-x86_64](https://github.com/SimaoMoreira5228/nexpack/releases/latest/download/nxpk-linux-x86_64)
- [nexpackd-linux-x86_64](https://github.com/SimaoMoreira5228/nexpack/releases/latest/download/nexpackd-linux-x86_64)
- [nexpack-source.tar.gz](https://github.com/SimaoMoreira5228/nexpack/releases/latest/download/nexpack-source.tar.gz)
- [sha256sums.txt](https://github.com/SimaoMoreira5228/nexpack/releases/latest/download/sha256sums.txt)

This section is updated by the release workflow. The downloads link to the latest release via `/latest/download/` urls, so they always point to whatever the current version is.
