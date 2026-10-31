#!/usr/bin/env python3
"""Open adash against a temporary loopback sshd for native UI verification.

No existing hosts, accounts, credentials or services are modified. --capture
writes the actual window then cleans up; otherwise Ctrl+C closes both processes.
Requires sshd, ssh-keygen, niri, ImageMagick and a built adash-app.
"""
import argparse
import base64
import hashlib
import json
import os
from pathlib import Path
import shutil
import signal
import socket
import subprocess
import tempfile
import time

ROOT = Path(__file__).resolve().parents[1]


def action(name, *args):
    subprocess.run(["niri", "msg", "action", name, *map(str, args)], check=True)


def windows():
    return json.loads(subprocess.check_output(["niri", "msg", "--json", "windows"]))


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--tab", choices=["terminal", "files", "monitor"], default="terminal")
    parser.add_argument("--capture", type=Path)
    parser.add_argument("--untrusted", action="store_true")
    parser.add_argument("--dialog", action="store_true", help="open the read-only remote-name confirmation surface")
    args = parser.parse_args()
    with tempfile.TemporaryDirectory(prefix="adash-ssh-ui-") as directory:
        directory = Path(directory)
        work = directory / "work"
        work.mkdir()
        (work / "readme.txt").write_text("SSH/SFTP native UI verification\n颜色与中文必须保留在同一行。\n")
        (work / "logs").mkdir()
        for name in ["identity", "host"]:
            subprocess.run(["ssh-keygen", "-q", "-t", "ed25519", "-N", "", "-f", str(directory / name)], check=True)
        shutil.copyfile(directory / "identity.pub", directory / "authorized_keys")
        with socket.socket() as probe:
            probe.bind(("127.0.0.1", 0))
            port = probe.getsockname()[1]
        user = subprocess.check_output(["id", "-un"], text=True).strip()
        rc = directory / "bashrc"
        rc.write_text(f"HISTFILE='{directory / 'history'}'\nPS1='local-test:\\w$ '\nprintf '\\033[32mSSH connected\\033[0m  two spaces  中文\\n'\n")
        dispatch = directory / "session.sh"
        dispatch.write_text(f'#!/bin/sh\ncd "{work}" || exit 1\nif [ -z "$SSH_ORIGINAL_COMMAND" ]; then exec /bin/bash --noprofile --rcfile "{rc}" -i; else exec /bin/sh -c "$SSH_ORIGINAL_COMMAND"; fi\n')
        config = directory / "sshd_config"
        config.write_text(f"""Port {port}
ListenAddress 127.0.0.1
HostKey {directory / 'host'}
PidFile {directory / 'sshd.pid'}
AuthorizedKeysFile {directory / 'authorized_keys'}
StrictModes no
UsePAM no
PasswordAuthentication no
KbdInteractiveAuthentication no
PubkeyAuthentication yes
AllowUsers {user}
AllowTcpForwarding yes
Subsystem sftp /usr/lib/ssh/sftp-server
ForceCommand /bin/sh {dispatch}
LogLevel ERROR
""")
        data = directory / "data/adash"
        data.mkdir(parents=True)
        fingerprint = "SHA256:" + base64.b64encode(hashlib.sha256(base64.b64decode((directory / "host.pub").read_text().split()[1])).digest()).decode().rstrip("=")
        identity_id = f"native-{directory.name}"
        profiles = {
            "hosts": {"local-test": {"id": "local-test", "name": "Local SSH verification", "address": "127.0.0.1", "port": port, "username": user, "identity": identity_id}},
            "identities": {identity_id: {"id": identity_id, "name": "Temporary test key", "kind": "PrivateKey", "key_path": str(directory / "identity")}},
            "known_hosts": {} if args.untrusted else {f"[127.0.0.1]:{port}": fingerprint},
        }
        (data / "hosts.json").write_text(json.dumps(profiles))
        (data / "hosts.json").chmod(0o600)
        env = os.environ.copy()
        for name in ["ADASH_DATA_DIR", "ADASH_START_SETTINGS", "ADASH_DEMO_ASK", "ADASH_DEMO_REPLAY", "ADASH_DEMO_APPEARANCE", "ADASH_DEMO_DIALOG"]:
            env.pop(name, None)
        env.update(XDG_DATA_HOME=str(directory / "data"), UDASH_MOCK="1", ADASH_OFFLINE="1",
                   ADASH_START_HOST="local-test", ADASH_START_REMOTE_DIR=str(work),
                   ADASH_START_SESSION_TAB=str(["terminal", "files", "monitor"].index(args.tab)))
        if args.dialog:
            env["ADASH_DEMO_DIALOG"] = "1"
        previous = next((window["id"] for window in windows() if window["is_focused"]), None)
        with open("/tmp/adash-ssh-ui.log", "w") as log:
            server = subprocess.Popen(["/usr/bin/sshd", "-D", "-e", "-f", str(config)], stdout=log, stderr=log)
            app = None
            try:
                time.sleep(0.3)
                if server.poll() is not None:
                    raise RuntimeError("temporary sshd did not start; see /tmp/adash-ssh-ui.log")
                app = subprocess.Popen([str(ROOT / "target/debug/adash-app")], cwd=ROOT, env=env, stdout=log, stderr=log)
                print(json.dumps({"app_pid": app.pid, "server_pid": server.pid, "directory": str(directory)}), flush=True)
                window = None
                for _ in range(100):
                    window = next((window for window in windows() if window["pid"] == app.pid), None)
                    if window:
                        break
                    time.sleep(0.1)
                if window is None:
                    raise RuntimeError("adash did not create a window")
                wid = window["id"]
                if not window["is_floating"]:
                    action("toggle-window-floating", "--id", wid)
                action("set-window-width", "--id", wid, 1440)
                action("set-window-height", "--id", wid, 900)
                action("focus-window", "--id", wid)
                if args.capture:
                    # Monitoring needs two actual samples for CPU deltas.
                    time.sleep(12 if args.tab == "monitor" else 5)
                    action("focus-window", "--id", wid)
                    raw = directory / "capture.png"
                    action("screenshot-window", "--id", wid, "--path", raw)
                    for _ in range(100):
                        if raw.exists():
                            break
                        time.sleep(0.05)
                    if not raw.exists():
                        raise RuntimeError("niri did not write the window screenshot")
                    width, height = map(int, subprocess.check_output(["magick", "identify", "-format", "%w %h", str(raw)], text=True).split())
                    scale = min(width / 1480, height / 940)
                    inset = round(20 * scale)
                    args.capture.parent.mkdir(parents=True, exist_ok=True)
                    subprocess.run(["magick", str(raw), "-crop", f"{round(1440 * scale)}x{round(900 * scale)}+{inset}+{inset}", "+repage", "-resize", "1440x900!", str(args.capture)], check=True)
                    print(args.capture, flush=True)
                else:
                    app.wait()
            finally:
                for process in [app, server]:
                    if process is not None:
                        process.terminate()
                        try:
                            process.wait(timeout=5)
                        except subprocess.TimeoutExpired:
                            process.kill()
                            process.wait()
                if previous is not None and any(window["id"] == previous for window in windows()):
                    action("focus-window", "--id", previous)


if __name__ == "__main__":
    signal.signal(signal.SIGTERM, lambda *_: (_ for _ in ()).throw(KeyboardInterrupt()))
    try:
        main()
    except KeyboardInterrupt:
        pass
