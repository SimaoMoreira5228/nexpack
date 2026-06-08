import os, shutil, tempfile
from conftest import check, run, NXPK, require_erofs


def test():
    print("\n=== remove ===")
    if not require_erofs():
        return

    fake_home = tempfile.mkdtemp()
    env = os.environ.copy()
    env["HOME"] = fake_home
    try:
        with tempfile.TemporaryDirectory() as tmp:
            from conftest import pack_bundle

            bundle = pack_bundle(tmp)
            if bundle is None:
                return

            r = run([NXPK, "install", bundle], cwd=tmp, env=env)
            check("install for remove test succeeds", r.returncode == 0)
            app_dir = os.path.join(
                fake_home,
                ".local",
                "share",
                "nexpack",
                "store",
                "apps",
                "io.test.hello",
            )
            check("app dir exists after install", os.path.isdir(app_dir))

            r = run([NXPK, "remove", "io.test.hello"], cwd=tmp, env=env)
            check("remove succeeds", r.returncode == 0)
            check("app dir removed after remove", not os.path.exists(app_dir))

            r = run([NXPK, "remove", "io.test.hello"], cwd=tmp, env=env, expect=1)
            check("remove non-existent app fails gracefully", r.returncode != 0)
    finally:
        shutil.rmtree(fake_home, ignore_errors=True)
