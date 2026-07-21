import os
import subprocess
import shutil
import sys

HERE = os.path.dirname(os.path.abspath(__file__))
REPO = os.path.dirname(HERE)
NXPK = os.path.join(REPO, "target", "debug", "nxpk")
NEXPACKD = os.path.join(REPO, "target", "debug", "nexpackd")
TEST_APPS_DIR = os.path.join(HERE, "test-apps")

PASSED = 0
FAILED = 0
SKIPPED = 0


def _build_test_apps():
    r = subprocess.run(
        ["make", "-C", TEST_APPS_DIR, "all"],
        capture_output=True, text=True, timeout=60,
    )
    if r.returncode != 0:
        print(f"  warning: test-app build failed:\n{r.stderr}", file=sys.stderr)


_build_test_apps()


def check(msg, cond):
    global PASSED, FAILED
    if cond:
        PASSED += 1
        print(f"  ok  {msg}")
    else:
        FAILED += 1
        print(f"  FAIL {msg}")


def skip(msg):
    global SKIPPED
    SKIPPED += 1
    print(f"  skip {msg}")


def build_staging(tmp, app="simple", entrypoint="hello"):
    d = os.path.join(tmp, "staging")
    os.makedirs(os.path.join(d, "usr", "bin"), exist_ok=True)
    binary = os.path.join(d, "usr", "bin", entrypoint)
    src = os.path.join(TEST_APPS_DIR, app)
    if os.path.isfile(src):
        shutil.copy2(src, binary)
    else:
        with open(binary, "w") as f:
            f.write("#!/bin/sh\necho 'hello from nexpack'")
    os.chmod(binary, 0o755)
    return d


def make_spec(
    tmp, app_id="io.test.hello", version="1.0.0", entrypoint="/usr/bin/hello", extra=""
):
    path = os.path.join(tmp, "spec.toml")
    with open(path, "w") as f:
        f.write(f"[app]\n")
        f.write(f'id = "{app_id}"\n')
        f.write(f'version = "{version}"\n')
        f.write(f'entrypoint = "{entrypoint}"\n')
        if extra:
            f.write(f"{extra}\n")
        f.write(f"\n[[layer]]\n")
        f.write(f'role = "app"\n')
        f.write(f'source = "./staging"\n')
    return path


def run(cmd, cwd=None, expect=0, env=None, timeout=30):
    cwd = cwd or REPO
    try:
        r = subprocess.run(
            cmd, capture_output=True, text=True, cwd=cwd, env=env, timeout=timeout
        )
        if expect >= 0 and r.returncode != expect:
            print(f"  command failed: {' '.join(cmd)}")
            print(f"  exit code: {r.returncode} (expected {expect})")
            if r.stderr:
                print(f"  stderr: {r.stderr[:500]}")
        return r
    except subprocess.TimeoutExpired:
        print(f"  command timed out: {' '.join(cmd)}")
        r = subprocess.CompletedProcess(cmd, -1, "", "timed out")
        return r


def require_erofs():
    if shutil.which("mkfs.erofs") is None:
        skip("mkfs.erofs not available")
        return False
    return True


def pack_bundle(tmp, extra="", bootstrap=False, app_id="io.test.hello"):
    _staging = build_staging(tmp)
    spec_parts = [
        f'[app]\nid = "{app_id}"\nversion = "1.0.0"\nentrypoint = "/usr/bin/hello"\n'
    ]
    if extra:
        spec_parts.append(extra + "\n")
    if bootstrap:
        spec_parts.append(f'[bootstrap]\nnexpackd = "{NEXPACKD}"\nnxpk = "{NXPK}"\n')
    spec_parts.append('[[layer]]\nrole = "app"\nsource = "./staging"\n')
    spec_path = os.path.join(tmp, "spec.toml")
    with open(spec_path, "w") as f:
        f.writelines(spec_parts)
    r = run([NXPK, "pack", spec_path], cwd=tmp)
    if r.returncode != 0:
        return None
    candidates = [os.path.join(tmp, f) for f in os.listdir(tmp) if f.endswith(".nxpk")]
    return candidates[0] if candidates else None


def print_summary():
    total = PASSED + FAILED + SKIPPED
    print(f"\n  {PASSED}/{total} passed, {FAILED} failed, {SKIPPED} skipped")
    return FAILED == 0
