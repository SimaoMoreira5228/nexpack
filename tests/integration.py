#!/usr/bin/env python3
import os
import shutil
import subprocess
import sys
import tempfile

HERE = os.path.dirname(os.path.abspath(__file__))
REPO = os.path.dirname(HERE)
NXPK = os.path.join(REPO, "target", "debug", "nxpk")
NEXPACKD = os.path.join(REPO, "target", "debug", "nexpackd")

FAILED = 0
PASSED = 0


def check(msg, cond):
    global PASSED, FAILED
    if cond:
        PASSED += 1
        print(f"  ok  {msg}")
    else:
        FAILED += 1
        print(f"  FAIL {msg}")


def build_staging(tmp, name="hello", content="hello from nexpack"):
    d = os.path.join(tmp, "staging")
    os.makedirs(os.path.join(d, "usr", "bin"), exist_ok=True)
    binary = os.path.join(d, "usr", "bin", name)
    with open(binary, "w") as f:
        f.write(f"#!/bin/sh\necho '{content}'")
    os.chmod(binary, 0o755)
    return d


def make_spec(
    tmp, app_id="io.test.hello", version="1.0.0", entrypoint="/usr/bin/hello", extra=""
):
    path = os.path.join(tmp, "spec.toml")
    with open(path, "w") as f:
        f.write(f"""[app]
id = "{app_id}"
version = "{version}"
entrypoint = "{entrypoint}"

{extra}

[[layer]]
role = "app"
source = "./staging"
""")
    return path


def run(cmd, cwd=None, expect=0, env=None):
    cwd = cwd or REPO
    r = subprocess.run(cmd, capture_output=True, text=True, cwd=cwd, env=env)
    if expect >= 0 and r.returncode != expect:
        print(f"  command failed: {' '.join(cmd)}")
        print(f"  exit code: {r.returncode} (expected {expect})")
        print(f"  stdout: {r.stdout[:500]}")
        print(f"  stderr: {r.stderr[:500]}")
    return r


def test_tools():
    print("\n=== tool availability ===")
    check("nxpk binary exists", os.path.exists(NXPK))
    check("nexpackd binary exists", os.path.exists(NEXPACKD))
    check("mkfs.erofs available", shutil.which("mkfs.erofs") is not None)
    check("b3sum available", shutil.which("b3sum") is not None)
    r = run([NXPK, "--help"], expect=0)
    check("nxpk --help succeeds", r.returncode == 0)


def require_erofs():
    """return True if mkfs.erofs is available."""
    if shutil.which("mkfs.erofs") is None:
        print("  (skipped -- mkfs.erofs not available)")
        return False
    return True


def test_pack_and_inspect():
    print("\n=== pack + inspect ===")
    if not require_erofs():
        return

    with tempfile.TemporaryDirectory() as tmp:
        _staging = build_staging(tmp)
        spec = make_spec(tmp)

        r = run([NXPK, "pack", spec], cwd=tmp, expect=0)
        check("pack succeeds", r.returncode == 0)
        bundle = os.path.join(tmp, "hello.nxpk")
        check("bundle file created", os.path.exists(bundle))
        if os.path.exists(bundle):
            check("bundle file non-empty", os.path.getsize(bundle) > 4096)

            r = run([NXPK, "inspect", bundle], expect=0)
            check("inspect succeeds", r.returncode == 0)
            check("inspect shows app id", "io.test.hello" in r.stdout)
            check("inspect shows entrypoint", "/usr/bin/hello" in r.stdout)
            check("inspect shows layer", "app" in r.stdout)

            r = run([NXPK, "inspect", bundle, "--json"], expect=0)
            check("inspect --json succeeds", r.returncode == 0)
            check("inspect --json is valid", r.stdout.strip().startswith("{"))


def test_verify():
    print("\n=== verify ===")
    if not require_erofs():
        return
    with tempfile.TemporaryDirectory() as tmp:
        _staging = build_staging(tmp)
        spec = make_spec(tmp)
        run([NXPK, "pack", spec], cwd=tmp, expect=0)
        bundle = os.path.join(tmp, "hello.nxpk")

        r = run([NXPK, "verify", bundle], expect=0)
        check("verify succeeds on valid bundle", r.returncode == 0)

        r = run([NXPK, "verify", bundle, "--offline"], expect=0)
        check("verify --offline succeeds", r.returncode == 0)


def test_install_export_roundtrip():
    print("\n=== install + export round-trip ===")
    if not require_erofs():
        return
    fake_home = tempfile.mkdtemp()
    env = os.environ.copy()
    env["HOME"] = fake_home
    env["XDG_RUNTIME_DIR"] = tempfile.mkdtemp()

    try:
        with tempfile.TemporaryDirectory() as tmp:
            _staging = build_staging(tmp)
            spec = make_spec(tmp)
            run([NXPK, "pack", spec], cwd=tmp, expect=0)
            original = os.path.join(tmp, "hello.nxpk")
            check("original bundle created", os.path.isfile(original))

            if not os.path.isfile(original):
                print(f"  pack output not found. tmp contents: {os.listdir(tmp)}")
                return

            saved_copy = os.path.join(tmp, "original.nxpk")
            shutil.copy2(original, saved_copy)

            r = run([NXPK, "install", original], cwd=tmp, expect=0, env=env)
            check("install succeeds", r.returncode == 0)

            app_dir = os.path.join(
                fake_home,
                ".local",
                "share",
                "nexpack",
                "store",
                "apps",
                "io.test.hello",
            )
            check("app directory created", os.path.isdir(app_dir))

            meta = os.path.join(app_dir, "meta.capnp")
            check("meta.capnp exists", os.path.isfile(meta))
            if os.path.isfile(meta):
                check("meta.capnp non-empty", os.path.getsize(meta) > 0)

            current_link = os.path.join(app_dir, "current")
            check("current symlink exists", os.path.islink(current_link))

            r = run([NXPK, "export", "io.test.hello"], cwd=tmp, expect=0, env=env)
            check("export succeeds", r.returncode == 0)

            exported_path = os.path.join(tmp, "hello.nxpk")
            if os.path.exists(exported_path):
                check(
                    "exported bundle is non-empty",
                    os.path.getsize(exported_path) > 4096,
                )
                check(
                    "exported bundle starts with ELF magic",
                    open(exported_path, "rb").read(4) == b"\x7fELF",
                )
                check(
                    "original bundle starts with ELF magic",
                    open(saved_copy, "rb").read(4) == b"\x7fELF",
                )

                r = run([NXPK, "inspect", saved_copy], expect=0)
                check("original bundle inspectable", r.returncode == 0)
                r = run([NXPK, "inspect", exported_path], expect=0)
                check("exported bundle inspectable", r.returncode == 0)
            else:
                check("exported bundle found on disk", False)
                print(f"  tmp contents: {os.listdir(tmp)}")

            r = run([NXPK, "export", "io.test.hello"], cwd=tmp, expect=0, env=env)
            check("export succeeds", r.returncode == 0)

            candidates = [
                os.path.join(tmp, "hello.nxpk"),
                os.path.join(tmp, "io.test.hello.nxpk"),
                os.path.join(fake_home, "hello.nxpk"),
                os.path.join(tmp, "exported.nxpk"),
            ]
            exported_path = None
            for c in candidates:
                if os.path.exists(c):
                    exported_path = c
                    break
            likely = os.path.join(tmp, "hello.nxpk")
            if os.path.exists(likely):
                exported_path = likely
            elif os.path.exists(os.path.join(tmp, "io.test.hello.nxpk")):
                exported_path = os.path.join(tmp, "io.test.hello.nxpk")

            if exported_path and os.path.exists(exported_path):
                check(
                    "exported bundle is non-empty",
                    os.path.getsize(exported_path) > 4096,
                )
                check(
                    "exported bundle starts with ELF magic",
                    open(exported_path, "rb").read(4) == b"\x7fELF",
                )
                check(
                    "original bundle starts with ELF magic",
                    open(original, "rb").read(4) == b"\x7fELF",
                )
            else:
                check("exported bundle found on disk", False)
                print(f"  looked in: {candidates}")
                print(f"  tmp: {os.listdir(tmp)}")

            r = run([NXPK, "inspect", original], cwd=tmp, expect=0)
            if exported_path:
                r = run([NXPK, "inspect", exported_path], cwd=tmp, expect=0)
                check("exported bundle inspectable", r.returncode == 0)

    finally:
        shutil.rmtree(fake_home, ignore_errors=True)
        shutil.rmtree(env["XDG_RUNTIME_DIR"], ignore_errors=True)


def test_permissions():
    print("\n=== permissions ===")
    if not require_erofs():
        return
    with tempfile.TemporaryDirectory() as tmp:
        _staging = build_staging(tmp)
        extra = """[permissions]
network = false
display = "wayland"
"""
        spec = make_spec(tmp, extra=extra)
        run([NXPK, "pack", spec], cwd=tmp, expect=0)
        bundle = os.path.join(tmp, "hello.nxpk")

        r = run([NXPK, "inspect", bundle], expect=0)
        check(
            "inspect shows permissions",
            "permissions" in r.stdout.lower() or "network" in r.stdout.lower(),
        )

        fake_home = tempfile.mkdtemp()
        env = os.environ.copy()
        env["HOME"] = fake_home
        try:
            run([NXPK, "install", bundle], cwd=tmp, expect=0, env=env)
            r = run([NXPK, "permissions", "io.test.hello"], cwd=tmp, expect=0, env=env)
            check(
                "nxpk permissions works",
                r.returncode == 0 and "io.test.hello" in r.stdout,
            )
        finally:
            shutil.rmtree(fake_home, ignore_errors=True)


def test_gc():
    print("\n=== gc ===")
    with tempfile.TemporaryDirectory() as tmp:
        _staging = build_staging(tmp)
        _spec = make_spec(tmp)

        r = run([NXPK, "gc"], cwd=tmp, expect=0)
        check("gc succeeds on empty store", r.returncode == 0)


def main():
    print("nexpack integration tests")
    print(f"  repo: {REPO}")
    print(f"  nxpk: {NXPK}")
    print(f"  daemon: {NEXPACKD}")

    test_tools()

    if not os.path.exists(NXPK):
        print("\nerror: nxpk binary not found. run 'cargo build' first.")
        sys.exit(1)

    test_pack_and_inspect()
    test_verify()
    test_install_export_roundtrip()
    test_permissions()
    test_gc()

    total = PASSED + FAILED
    print(f"\n{'=' * 50}")
    print(f"{PASSED}/{total} passed, {FAILED} failed")
    return 0 if FAILED == 0 else 1


if __name__ == "__main__":
    sys.exit(main())
