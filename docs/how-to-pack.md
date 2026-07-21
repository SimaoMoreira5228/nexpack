# How to Pack a Bundle

First you need a directory with your app in it. This is called the **staging directory**. Everything in it becomes the app's root filesystem once the bundle is mounted. So if you have a binary at `./staging/usr/bin/myapp`, the entrypoint inside the bundle will be `/usr/bin/myapp`.

Next you write a **spec file**. This is a TOML file that tells `nxpk` what your app is and where to find its files. Here is the simplest possible one:

```toml
[app]
id = "com.example.myapp"
version = "1.0.0"
entrypoint = "/usr/bin/myapp"

[[layer]]
role = "app"
source = "./staging"
```

Put this in a file called `spec.toml` next to your staging directory. Then run:

```bash
nxpk pack spec.toml
```

This creates `myapp.nxpk` in the current directory. You can now run it with:

```bash
# Direct execution (requires bootstrap — see below)
chmod +x myapp.nxpk && ./myapp.nxpk

# With nxpk installed
nxpk run myapp.nxpk                # no sandbox (default)
nxpk run myapp.nxpk --sandbox      # with sandbox
```

## Sandbox Permissions

Sandboxing is **opt-in**. By default `nxpk run` runs the app without sandbox (full filesystem access). Pass `--sandbox` to enable bubblewrap isolation.

If you want sandbox permissions, add a `[permissions]` section to the spec:

```toml
[app]
id = "com.example.myapp"
version = "1.0.0"
entrypoint = "/usr/bin/myapp"

[permissions]
network = false
filesystem = ["$HOME/.config/myapp"]
display = "wayland"

[[layer]]
role = "app"
source = "./staging"
```

The permissions section tells Nexpack what the app needs access to *when sandboxed*. If you set `network` to `false`, blocked syscalls return `-EPERM` (the app gets an error instead of crashing). If you set `display` to `wayland`, it gets Wayland socket access but not X11.

## Multiple Layers

If your app depends on a shared runtime like Electron or Qt, you can split it into multiple layers:

```toml
[[layer]]
role = "runtime"
source = "./staging-runtime"

[[layer]]
role = "app"
source = "./staging-app"
```

Layers are merged via OverlayFS in order. If two bundles share the same runtime layer (same blake3 digest), it is stored once on disk.

## Verifying and Inspecting

```bash
nxpk verify myapp.nxpk
nxpk verify myapp.nxpk --offline
nxpk inspect myapp.nxpk
nxpk inspect myapp.nxpk --json
```

`verify` checks BLAKE3 digests and the Sigstore signature. `inspect` prints the header, layers, permissions, and SBOM.

## Self-Bootstrapping (Recommended)

So users can `chmod +x myapp.nxpk && ./myapp.nxpk` with zero prior setup, embed the nexpack runtime binaries:

```toml
[bootstrap]
nexpackd = "./target/release/nexpackd"
nxpk = "./target/release/nxpk"
```

On first run the ELF stub extracts them to `~/.local/share/nexpack/bin/`, starts the daemon, and launches your app. Subsequent runs reuse the extracted binaries. Without bootstrap the user needs `nxpk` installed in PATH.

## Signing

```bash
nxpk sign myapp.nxpk
```

This calls `cosign sign-blob` under the hood and embeds a Sigstore bundle in the header. Users can verify with `nxpk verify myapp.nxpk` and the daemon checks the signature at mount time too.
