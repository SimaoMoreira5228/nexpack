import os
from conftest import check, run, pack_bundle, NXPK


def test():
    print("\n=== pack + inspect ===")
    import tempfile

    with tempfile.TemporaryDirectory() as tmp:
        bundle = pack_bundle(tmp)
        if bundle is None:
            check("pack succeeds", False)
            return
        check("pack succeeds", True)
        check("bundle file non-empty", os.path.getsize(bundle) > 4096)

        r = run([NXPK, "inspect", bundle])
        check("inspect succeeds", r.returncode == 0)
        check("inspect shows app id", "io.test.hello" in r.stdout)
        check("inspect shows entrypoint", "/usr/bin/hello" in r.stdout)
        check("inspect shows layer", "app" in r.stdout)

        r = run([NXPK, "inspect", bundle, "--json"])
        check("inspect --json succeeds", r.returncode == 0)
        check("inspect --json is valid", r.stdout.strip().startswith("{"))
