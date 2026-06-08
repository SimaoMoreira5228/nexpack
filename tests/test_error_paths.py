import os, shutil, tempfile
from conftest import check, run, NXPK


def test():
    print("\n=== error paths ===")

    r = run([NXPK, "inspect", "/nonexistent/file.nxpk"], expect=1)
    check("inspect on missing file fails", r.returncode != 0)

    r = run([NXPK, "verify", "/nonexistent/file.nxpk"], expect=1)
    check("verify on missing file fails", r.returncode != 0)

    r = run([NXPK, "install", "/nonexistent/file.nxpk"], expect=1)
    check("install on missing file fails", r.returncode != 0)

    with tempfile.TemporaryDirectory() as tmp:
        spec = os.path.join(tmp, "bad.toml")
        with open(spec, "w") as f:
            f.write("not valid toml {{{")
        r = run([NXPK, "pack", spec], expect=1)
        check("pack on invalid spec fails", r.returncode != 0)

    r = run([NXPK], expect=2)
    check("nxpk without subcommand fails", r.returncode != 0)

    r = run([NXPK, "nonexistent"], expect=2)
    check("nxpk with unknown subcommand fails", r.returncode != 0)

    fake_home = tempfile.mkdtemp()
    env = os.environ.copy()
    env["HOME"] = fake_home
    try:
        r = run([NXPK, "export", "io.nonexistent.app"], env=env, expect=1)
        check("export on non-installed app fails", r.returncode != 0)
    finally:
        shutil.rmtree(fake_home, ignore_errors=True)
