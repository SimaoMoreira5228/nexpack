import os, time, tempfile, subprocess
from conftest import check, run, NXPK, NEXPACKD, require_erofs


def test():
    print("\n=== stub self-bootstrap ===")
    if not require_erofs():
        return

    fake_home = tempfile.mkdtemp()
    fake_runtime = tempfile.mkdtemp()
    env = os.environ.copy()
    env["HOME"] = fake_home
    env["XDG_RUNTIME_DIR"] = fake_runtime
    env["PATH"] = "/usr/bin:/bin"

    try:
        import tempfile as _tf

        with _tf.TemporaryDirectory() as tmp:
            os.makedirs(os.path.join(tmp, "staging", "usr", "bin"), exist_ok=True)
            with open(os.path.join(tmp, "staging", "usr", "bin", "hello"), "w") as f:
                f.write("#!/bin/sh\necho hi")
            os.chmod(os.path.join(tmp, "staging", "usr", "bin", "hello"), 0o755)

            spec = os.path.join(tmp, "spec.toml")
            with open(spec, "w") as f:
                f.write("[app]\n")
                f.write('id = "io.test.bootstrap"\n')
                f.write('version = "1.0.0"\n')
                f.write('entrypoint = "/usr/bin/hello"\n')
                f.write("\n")
                f.write("[bootstrap]\n")
                f.write(f'nexpackd = "{NEXPACKD}"\n')
                f.write(f'nxpk = "{NXPK}"\n')
                f.write("\n")
                f.write("[[layer]]\n")
                f.write('role = "app"\n')
                f.write('source = "./staging"\n')

            r = run([NXPK, "pack", spec], cwd=tmp)
            check("pack with bootstrap succeeds", r.returncode == 0)
            if r.returncode != 0:
                return

            bundles = [
                os.path.join(tmp, f) for f in os.listdir(tmp) if f.endswith(".nxpk")
            ]
            if not bundles:
                check("bundle file exists", False)
                return
            bundle = bundles[0]
            check("bundle file exists", True)

            sock_path = os.path.join(fake_runtime, "nexpack.sock")
            check("daemon socket does not exist yet", not os.path.exists(sock_path))

            os.chmod(bundle, 0o755)
            try:
                subprocess.run(
                    [bundle], capture_output=True, timeout=8, env=env, cwd=fake_home
                )
            except subprocess.TimeoutExpired:
                pass

            time.sleep(1)

            bin_dir = os.path.join(fake_home, ".local", "share", "nexpack", "bin")
            for name in ["nxpk", "nexpackd"]:
                p = os.path.join(bin_dir, name)
                check(f"{name} extracted to bin dir", os.path.isfile(p))
                if os.path.isfile(p):
                    check(f"{name} is executable", os.access(p, os.X_OK))
                    check(f"{name} non-empty", os.path.getsize(p) > 100000)

            sock_exists = os.path.exists(sock_path)
            check("daemon socket was created", sock_exists)

            d_found = False
            try:
                ps = subprocess.run(
                    ["pgrep", "-a", "nexpackd"],
                    capture_output=True,
                    text=True,
                    timeout=5,
                )
                d_found = len(ps.stdout.strip()) > 0
            except Exception:
                pass
            check("nexpackd process appeared", d_found)
    finally:
        subprocess.run(["pkill", "-f", "nexpackd"], capture_output=True, timeout=5)
        subprocess.run(["pkill", "-f", "nxpk"], capture_output=True, timeout=5)
        import shutil

        shutil.rmtree(fake_home, ignore_errors=True)
        shutil.rmtree(fake_runtime, ignore_errors=True)
