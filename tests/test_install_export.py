import os, shutil, tempfile
from conftest import check, run, pack_bundle, NXPK, require_erofs


def test():
    print("\n=== install + export round-trip ===")
    if not require_erofs():
        return

    fake_home = tempfile.mkdtemp()
    fake_runtime = tempfile.mkdtemp()
    env = os.environ.copy()
    env["HOME"] = fake_home
    env["XDG_RUNTIME_DIR"] = fake_runtime

    try:
        with tempfile.TemporaryDirectory() as tmp:
            bundle = pack_bundle(tmp)
            if bundle is None:
                return

            saved = bundle + ".orig"
            shutil.copy2(bundle, saved)

            r = run([NXPK, "install", bundle], cwd=tmp, env=env)
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

            r = run([NXPK, "export", "io.test.hello"], cwd=tmp, env=env)
            check("export succeeds", r.returncode == 0)

            exported = os.path.join(tmp, "hello.nxpk")
            check("exported bundle exists", os.path.isfile(exported))
            if os.path.isfile(exported):
                check("exported bundle non-empty", os.path.getsize(exported) > 4096)
                check(
                    "exported starts with ELF magic",
                    open(exported, "rb").read(4) == b"\x7fELF",
                )
                check(
                    "original starts with ELF magic",
                    open(saved, "rb").read(4) == b"\x7fELF",
                )
                r = run([NXPK, "inspect", saved])
                check("original inspectable", r.returncode == 0)
                r = run([NXPK, "inspect", exported])
                check("exported inspectable", r.returncode == 0)
    finally:
        shutil.rmtree(fake_home, ignore_errors=True)
        shutil.rmtree(fake_runtime, ignore_errors=True)
