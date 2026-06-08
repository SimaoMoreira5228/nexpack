import os, time, socket, subprocess, tempfile, shutil
from conftest import check, NEXPACKD


def test():
    print("\n=== daemon IPC ===")

    fake_home = tempfile.mkdtemp()
    fake_runtime = tempfile.mkdtemp()
    sock_path = os.path.join(fake_runtime, "nexpack.sock")
    env = os.environ.copy()
    env["HOME"] = fake_home
    env["XDG_RUNTIME_DIR"] = fake_runtime

    proc = None
    try:
        proc = subprocess.Popen(
            [NEXPACKD],
            stdout=subprocess.DEVNULL,
            stderr=subprocess.DEVNULL,
            env=env,
        )
        for _ in range(50):
            if os.path.exists(sock_path):
                break
            time.sleep(0.1)
        check("daemon socket appeared after starting", os.path.exists(sock_path))
        if not os.path.exists(sock_path):
            return

        raw = socket.socket(socket.AF_UNIX, socket.SOCK_STREAM)
        raw.settimeout(5)
        raw.connect(sock_path)
        check("raw socket connect succeeds", True)
        raw.close()
    except Exception as e:
        check(f"daemon test: {e}", False)
    finally:
        if proc:
            proc.terminate()
            try:
                proc.wait(timeout=5)
            except subprocess.TimeoutExpired:
                proc.kill()
                proc.wait()
        shutil.rmtree(fake_home, ignore_errors=True)
        shutil.rmtree(fake_runtime, ignore_errors=True)
