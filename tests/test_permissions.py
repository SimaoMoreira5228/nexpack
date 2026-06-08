import os, shutil, tempfile
from conftest import check, run, NXPK, require_erofs


def test():
    print("\n=== permissions ===")
    if not require_erofs():
        return

    with tempfile.TemporaryDirectory() as tmp:
        extra = '[permissions]\nnetwork = false\ndisplay = "wayland"\n'
        _staging = None
        spec_path = None
        exec(
            compile(
                open(os.path.join(os.path.dirname(__file__), "conftest.py")).read(),
                "conftest.py",
                "exec",
            )
        )

        from conftest import pack_bundle

        bundle = pack_bundle(tmp, extra=extra)
        if bundle is None:
            return

        with open(bundle, "rb") as f:
            magic = f.read(4)
        check("bundle starts with ELF", magic == b"\x7fELF")

        r = run([NXPK, "inspect", bundle])
        check(
            "inspect shows permissions",
            "permissions" in r.stdout.lower() or "network" in r.stdout.lower(),
        )

        fake_home = tempfile.mkdtemp()
        env = os.environ.copy()
        env["HOME"] = fake_home
        try:
            r = run([NXPK, "install", bundle], cwd=tmp, env=env)
            check("install with permissions succeeds", r.returncode == 0)
            r = run([NXPK, "permissions", "io.test.hello"], cwd=tmp, env=env)
            check(
                "nxpk permissions works",
                r.returncode == 0 and "io.test.hello" in r.stdout,
            )
        finally:
            shutil.rmtree(fake_home, ignore_errors=True)
