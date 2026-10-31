"""Shared build/verification helpers; no developer-specific paths."""
import json
import os
from pathlib import Path
import shlex
import shutil
import subprocess
import time
import signal
import sys


def user_data_dir():
    if os.environ.get("ADASH_DATA_DIR"):
        return Path(os.environ["ADASH_DATA_DIR"])
    if sys.platform == "win32":
        base = Path(os.environ.get("LOCALAPPDATA", str(Path.home() / "AppData/Local")))
    elif sys.platform == "darwin":
        base = Path.home() / "Library/Application Support"
    else:
        base = Path(os.environ.get("XDG_DATA_HOME", str(Path.home() / ".local/share")))
    return base / "adash"


def stop_process(process):
    if process.poll() is not None:
        return
    if os.name == "posix":
        os.killpg(process.pid, signal.SIGTERM)
    else:
        process.terminate()
    try:
        process.wait(timeout=5)
    except subprocess.TimeoutExpired:
        if os.name == "posix":
            os.killpg(process.pid, signal.SIGKILL)
        else:
            process.kill()
        process.wait()

ROOT = Path(__file__).resolve().parents[1]


def build_binary(package, test_hooks=False):
    command = ["cargo", "build", "-p", package, "--message-format=json-render-diagnostics"]
    if test_hooks:
        command += ["--features", "ui-test-hooks"]
    process = subprocess.Popen(command, cwd=ROOT, stdout=subprocess.PIPE, text=True)
    artifact = None
    for line in process.stdout:
        record = json.loads(line)
        if record.get("reason") == "compiler-artifact" and record.get("executable") and record["target"]["name"] == package:
            artifact = Path(record["executable"]).resolve()
        if record.get("reason") == "compiler-message":
            import sys
            print(record.get("message", {}).get("rendered", ""), file=sys.stderr, end="")
    if process.wait() or artifact is None:
        raise RuntimeError(f"Could not build {package}")
    return artifact


def executable(name, override):
    configured = os.environ.get(override)
    found = shutil.which(configured or name)
    if found is None:
        raise RuntimeError(f"{name} is not on PATH; set {override} to its executable path")
    return Path(found).resolve()


def sftp_server(sshd):
    configured = os.environ.get("ADASH_TEST_SFTP_SERVER")
    if configured:
        if Path(configured).is_file():
            return Path(configured).resolve()
        raise RuntimeError("ADASH_TEST_SFTP_SERVER does not exist")
    found = shutil.which("sftp-server")
    if found:
        return Path(found).resolve()
    prefix = sshd.parent.parent
    for path in [sshd.parent / "sftp-server", prefix / "lib/ssh/sftp-server", prefix / "lib/openssh/sftp-server", prefix / "libexec/sftp-server", prefix / "libexec/openssh/sftp-server"]:
        if path.is_file():
            return path
    raise RuntimeError("Cannot locate sftp-server; set ADASH_TEST_SFTP_SERVER")


def niri_available():
    return bool(os.environ.get("NIRI_SOCKET") and shutil.which("niri"))


def windows():
    if not niri_available():
        return []
    return json.loads(subprocess.check_output(["niri", "msg", "--json", "windows"]))


def action(name, *args):
    subprocess.run(["niri", "msg", "action", name, *map(str, args)], check=True)


def capture(process, raw, width, height, delay=4, command=None):
    if command:
        time.sleep(delay)
        subprocess.run([part.format(pid=process.pid, output=str(raw)) for part in shlex.split(command)], check=True)
        return
    if not niri_available():
        raise RuntimeError("No automatic capture adapter for this desktop. Supply --capture-command with {pid} and {output}, or verify the window manually.")
    window = None
    for _ in range(100):
        window = next((window for window in windows() if window["pid"] == process.pid), None)
        if window:
            break
        if process.poll() is not None:
            raise RuntimeError("Application exited before opening a window")
        time.sleep(0.1)
    if window is None:
        raise RuntimeError("Application did not open a window")
    wid = window["id"]
    if not window["is_floating"]:
        action("toggle-window-floating", "--id", wid)
    action("set-window-width", "--id", wid, width)
    action("set-window-height", "--id", wid, height)
    action("focus-window", "--id", wid)
    time.sleep(delay)
    action("screenshot-window", "--id", wid, "--path", raw)
    for _ in range(100):
        if raw.exists():
            return
        time.sleep(0.05)
    raise RuntimeError("The screenshot adapter did not write an image")


def normalize_capture(raw, output, width, height):
    magick = executable("magick", "ADASH_IMAGEMAGICK")
    # Measure the opaque window instead of assuming a 20px compositor shadow
    # or this workstation's display scale. Opaque inputs retain their full area.
    geometry = subprocess.check_output([str(magick), str(raw), "-alpha", "extract", "-threshold", "99%", "-format", "%@", "info:"], text=True).strip()
    output.parent.mkdir(parents=True, exist_ok=True)
    subprocess.run([str(magick), str(raw), "-crop", geometry, "+repage", "-resize", f"{width}x{height}!", str(output)], check=True)
