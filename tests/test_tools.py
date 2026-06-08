import os, shutil
from conftest import check, run, NXPK, NEXPACKD


def test():
    print("\n=== tool availability ===")
    check("nxpk binary exists", os.path.exists(NXPK))
    check("nexpackd binary exists", os.path.exists(NEXPACKD))
    check("b3sum available", shutil.which("b3sum") is not None)
    r = run([NXPK, "--help"])
    check("nxpk --help succeeds", r.returncode == 0)
