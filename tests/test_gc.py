import os, tempfile, shutil
from conftest import check, run, NXPK, require_erofs


def test():
    print("\n=== gc ===")
    with tempfile.TemporaryDirectory() as tmp:
        r = run([NXPK, "gc"])
        check("gc succeeds on empty store", r.returncode == 0)

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
            check("install for gc test succeeds", r.returncode == 0)

            store_layers = os.path.join(
                fake_home, ".local", "share", "nexpack", "store", "layers"
            )
            has_layers = (
                os.path.isdir(store_layers) and len(os.listdir(store_layers)) > 0
            )
            check("layers in store", has_layers)

            r = run([NXPK, "remove", "io.test.hello"], cwd=tmp, env=env)
            check("remove succeeds", r.returncode == 0)
            r = run([NXPK, "gc"], cwd=tmp, env=env)
            check("gc succeeds after remove", r.returncode == 0)
    finally:
        shutil.rmtree(fake_home, ignore_errors=True)
