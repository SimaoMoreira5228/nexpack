import os
from conftest import check, run, pack_bundle, NXPK, require_erofs


def test():
    print("\n=== verify ===")
    if not require_erofs():
        return
    import tempfile

    with tempfile.TemporaryDirectory() as tmp:
        bundle = pack_bundle(tmp)
        if bundle is None:
            return

        r = run([NXPK, "verify", bundle])
        check("verify succeeds on valid bundle", r.returncode == 0)

        r = run([NXPK, "verify", bundle, "--offline"])
        check("verify --offline succeeds", r.returncode == 0)
