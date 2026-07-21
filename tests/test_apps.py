import os, time, tempfile, subprocess, shutil
from conftest import check, skip, NXPK, NEXPACKD, require_erofs, pack_bundle, run, build_staging, make_spec

HEREDIR = os.path.dirname(os.path.abspath(__file__))


def _start_daemon(env):
    proc = subprocess.Popen(
        [NEXPACKD], stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL, env=env
    )
    sock_path = os.path.join(env["XDG_RUNTIME_DIR"], "nexpack.sock")
    for _ in range(50):
        if os.path.exists(sock_path):
            break
        time.sleep(0.1)
    return proc, sock_path


def _stop_daemon(proc):
    if proc:
        proc.terminate()
        try:
            proc.wait(timeout=5)
        except subprocess.TimeoutExpired:
            proc.kill()
            proc.wait()
    subprocess.run(["pkill", "-f", "nexpackd"], capture_output=True, timeout=5)


def _env():
    e = os.environ.copy()
    e["HOME"] = tempfile.mkdtemp()
    e["XDG_RUNTIME_DIR"] = tempfile.mkdtemp()
    return e


def _clean_env(e):
    shutil.rmtree(e["HOME"], ignore_errors=True)
    shutil.rmtree(e["XDG_RUNTIME_DIR"], ignore_errors=True)


def _run_app(bundle, env, extra_args=None, timeout=15):
    cmd = [NXPK, "run", bundle, "--sandbox"]
    if extra_args:
        cmd.extend(extra_args)
    return subprocess.run(cmd, capture_output=True, text=True, timeout=timeout, env=env)


def test():
    print("\n=== compiled test apps ===")
    if not require_erofs():
        return
    if shutil.which("bwrap") is None:
        skip("bwrap not available")
        return

    daemon = None
    env = _env()

    try:
        daemon, sock_path = _start_daemon(env)
        check("daemon socket ready", os.path.exists(sock_path))
        if not os.path.exists(sock_path):
            return

        with tempfile.TemporaryDirectory() as tmp:
            bundle = pack_bundle(tmp)
            if bundle is None:
                return
            check("simple bundle packed", True)

            r = _run_app(bundle, env)
            check("simple: exit 0", r.returncode == 0)
            check("simple: Hello output", "Hello from Nexpack!" in r.stdout)
            check("simple: PID shown", "PID:" in r.stdout)
            check("simple: CWD shown", "CWD:" in r.stdout)
            check("simple: marker OK", "OK" in r.stdout)
            check("simple: sandbox launch msg", "Launching sandboxed" in r.stderr)

        with tempfile.TemporaryDirectory() as tmp:
            build_staging(tmp, app="probe")
            spec = make_spec(
                tmp,
                app_id="io.test.probe",
                extra=(
                    "[permissions]\n"
                    'network = false\n'
                    'filesystem = ["$HOME"]\n'
                    'display = "wayland"\n'
                ),
            )
            r2 = run([NXPK, "pack", spec], cwd=tmp)
            check("probe: pack succeeds", r2.returncode == 0)
            bundles = [os.path.join(tmp, f) for f in os.listdir(tmp) if f.endswith(".nxpk")]
            if not bundles:
                return
            bundle2 = bundles[0]
            check("probe: bundle exists", True)

            r = _run_app(bundle2, env)
            check("probe: app ran", r.returncode >= 0)
            check("probe: summary present", "Summary:" in r.stdout)
            check("probe: output has probes", "socket" in r.stdout)

            lines = r.stdout.strip().split("\n")
            summary = next((l for l in lines if "Summary:" in l), "")
            check("probe: summary has counts", "passed" in summary and "failed" in summary)

        with tempfile.TemporaryDirectory() as tmp:
            bundle3 = pack_bundle(tmp, app_id="io.test.nosandbox")
            if bundle3 is None:
                return

            r = subprocess.run(
                [NXPK, "run", bundle3, "--no-sandbox"],
                capture_output=True, text=True, timeout=15, env=env,
            )
            check("no-sandbox: exit 0", r.returncode == 0)
            check("no-sandbox: shows msg", "no sandbox" in r.stderr)

    except subprocess.TimeoutExpired as e:
        check(f"test timed out: {e}", False)
    except Exception as e:
        check(f"unexpected error: {e}", False)
        import traceback
        traceback.print_exc()
    finally:
        _stop_daemon(daemon)
        _clean_env(env)
