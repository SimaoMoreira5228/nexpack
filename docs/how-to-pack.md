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
nxpk run myapp.nxpk
```

Or install it:

```bash
nxpk install myapp.nxpk
```

## Sandbox Permissions

If you want sandbox permissions you add a `[permissions]` section to the spec:

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

The permissions section tells Nexpack what the app needs access to. If you set `network` to `false`, the app cannot call `connect()` or `bind()`. If you set `display` to `wayland`, it gets Wayland socket access but not X11. The sandbox enforces all of this at runtime. If the app tries to do something the permissions don't allow, it gets a blocked syscall and crashes.

## Multiple Layers

If your app depends on a shared runtime like Electron or Qt, you can split it into multiple layers. The runtime layer goes first, then the app layer:

```toml
[[layer]]
role = "runtime"
source = "./staging-runtime"

[[layer]]
role = "app"
source = "./staging-app"
```

Layers are merged via OverlayFS in the order they appear in the spec. The runtime layer is the base, the app layer goes on top. If two bundles share the same runtime layer (same digest), it is stored once on disk.

## Verifying and Inspecting

You can verify the bundle after packing:

```bash
nxpk verify myapp.nxpk
nxpk verify myapp.nxpk --offline
nxpk inspect myapp.nxpk
nxpk inspect myapp.nxpk --json
```

`verify` checks the BLAKE3 digests of every layer and the Sigstore signature if one is embedded. `inspect` prints the header, layers, permissions, and SBOM.

## Self-Bootstrapping

If you want the bundle to self-bootstrap so users can `chmod +x myapp.nxpk && ./myapp.nxpk` with zero prior setup, you need to embed static binaries:

```toml
[bootstrap]
nexpackd = "./target/release/nexpackd"
nxpk = "./target/release/nxpk"
```

These get extracted to `~/.local/share/nexpack/bin/` on first run. The user does not need Nexpack installed beforehand. This is optional. Without it the user just has to run `nxpk run myapp.nxpk` or `nexpackd &` first.

## Signing

For signing you run this after packing:

```bash
nxpk sign myapp.nxpk
```

This calls `cosign sign-blob` under the hood and embeds a Sigstore bundle in the header. Users can then verify with `nxpk verify myapp.nxpk` and the daemon will check the signature at mount time too.
