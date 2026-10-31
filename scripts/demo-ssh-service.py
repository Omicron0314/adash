#!/usr/bin/env python3
"""Start an isolated loopback SSH demo and seed adash's host profile.

The service listens on 127.0.0.1 only, uses a generated temporary host key and
an ed25519 demo identity, and executes in a dedicated demo directory. It does
not modify system sshd/accounts. Stop it with --stop.
"""
from __future__ import annotations

import argparse
import base64
import getpass
import hashlib
import json
import os
from pathlib import Path
import shutil
import signal
import shlex
import socket
import subprocess
import sys
import time

DEMO_ID = "demo-local"
IDENTITY_ID = "demo-identity"
FORWARD_ID = "demo-http"


def data_dir() -> Path:
    if os.environ.get("ADASH_DATA_DIR"):
        return Path(os.environ["ADASH_DATA_DIR"])
    if sys.platform == "win32":
        base = Path(os.environ.get("LOCALAPPDATA", Path.home() / "AppData/Local"))
    elif sys.platform == "darwin":
        base = Path.home() / "Library/Application Support"
    else:
        base = Path(os.environ.get("XDG_DATA_HOME", Path.home() / ".local/share"))
    return base / "adash"


def which(name: str, env: str) -> str:
    configured = os.environ.get(env)
    found = shutil.which(configured or name)
    if not found:
        raise SystemExit(f"{name} not found; set {env} to its executable")
    return str(Path(found).resolve())


def free_port() -> int:
    with socket.socket() as probe:
        probe.bind(("127.0.0.1", 0))
        return probe.getsockname()[1]


def stop(pid: int) -> None:
    try:
        os.kill(pid, signal.SIGTERM)
    except ProcessLookupError:
        return
    for _ in range(30):
        try:
            os.kill(pid, 0)
        except ProcessLookupError:
            return
        time.sleep(0.05)


def read_profile(path: Path) -> dict:
    if not path.exists():
        return {"hosts": {}, "identities": {}, "snippets": {}, "forwards": {}, "known_hosts": {}, "recent": []}
    try:
        data = json.loads(path.read_text())
    except json.JSONDecodeError as error:
        raise SystemExit(f"{path} is not valid JSON; refusing to overwrite it: {error}")
    for key, value in {"hosts": {}, "identities": {}, "snippets": {}, "forwards": {}, "known_hosts": {}, "recent": []}.items():
        data.setdefault(key, value)
    return data


def write_profile(path: Path, data: dict) -> None:
    path.parent.mkdir(parents=True, exist_ok=True)
    temporary = path.with_name(path.name + ".demo-tmp")
    temporary.write_text(json.dumps(data, ensure_ascii=False, indent=2) + "\n")
    os.chmod(temporary, 0o600)
    temporary.replace(path)
    os.chmod(path, 0o600)


def fingerprint(public_key: Path) -> str:
    parts = public_key.read_text().split()
    digest = hashlib.sha256(base64.b64decode(parts[1])).digest()
    return "SHA256:" + base64.b64encode(digest).decode().rstrip("=")


def main() -> None:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--stop", action="store_true", help="stop the demo sshd/http services")
    parser.add_argument("--port", type=int, default=0, help="SSH port; 0 chooses a free loopback port")
    args = parser.parse_args()
    data = data_dir()
    demo = data / "demo-ssh"
    state_path = demo / "service.json"
    if args.stop:
        if state_path.exists():
            try:
                state = json.loads(state_path.read_text())
                for key in ("sshd_pid", "http_pid"):
                    if state.get(key):
                        stop(int(state[key]))
            except (json.JSONDecodeError, OSError):
                pass
            state_path.unlink(missing_ok=True)
            print("adash demo SSH stopped")
        else:
            print("adash demo SSH is not running")
        return
    if state_path.exists():
        try:
            state = json.loads(state_path.read_text())
            if state.get("sshd_pid") and Path(f"/proc/{state['sshd_pid']}").exists():
                print(f"adash demo SSH already running on 127.0.0.1:{state.get('ssh_port')}")
                print(f"open 主机 → {state.get('host_name', 'Demo Local SSH')}")
                return
        except (json.JSONDecodeError, OSError):
            pass
        state_path.unlink(missing_ok=True)

    sshd = which("sshd", "ADASH_TEST_SSHD")
    keygen = which("ssh-keygen", "ADASH_TEST_SSH_KEYGEN")
    bash = shutil.which("bash") or shutil.which("sh")
    shell = shutil.which("sh") or bash
    sftp = os.environ.get("ADASH_TEST_SFTP_SERVER") or shutil.which("sftp-server")
    if not sftp:
        for candidate in [
            Path(sshd).parent / "sftp-server",
            Path(sshd).parent.parent / "lib/ssh/sftp-server",
            Path(sshd).parent.parent / "lib/openssh/sftp-server",
        ]:
            if candidate.is_file():
                sftp = str(candidate)
                break
    if not bash or not shell or not sftp:
        raise SystemExit("bash/sh/sftp-server not found; set ADASH_TEST_SFTP_SERVER")
    demo.mkdir(parents=True, exist_ok=True)
    work = demo / "work"
    for directory in (work, work / "logs"):
        directory.mkdir(exist_ok=True)
    (work / "README.md").write_text(
        "# Demo workspace\n\n这是 adash 演示 SSH 服务的工作目录。\n\n"
        "- 终端、SFTP、监控和片段都连接到 127.0.0.1 的临时 sshd\n"
        "- 没有修改系统账号或系统 sshd\n"
    )
    (work / "logs" / "app.log").write_text(
        "2026-11-01 09:00 demo service started\n"
        "2026-11-01 09:01 GET /health 200\n"
        "2026-11-01 09:02 order.created #1042\n"
    )
    (work / "config.example.json").write_text('{"port": 8080, "region": "cn-bj2", "debug": false}\n')
    site = demo / "www"
    site.mkdir(exist_ok=True)
    (site / "index.html").write_text(
        "<!doctype html><meta charset=utf-8><title>adash demo</title>"
        "<h1>adash demo service</h1><p>This page is reached through the demo SSH port forward.</p>"
    )
    host_key = demo / "host_ed25519"
    identity = demo / "identity_ed25519"
    for path in (host_key,):
        if not path.exists():
            subprocess.run([keygen, "-q", "-t", "ed25519", "-N", "", "-f", str(path)], check=True)
    if not identity.exists():
        subprocess.run([keygen, "-q", "-t", "ed25519", "-N", "", "-f", str(identity)], check=True)
    shutil.copyfile(str(identity) + ".pub", demo / "authorized_keys")
    os.chmod(demo / "authorized_keys", 0o600)

    port = args.port or free_port()
    http_port = free_port()
    rc = demo / "bashrc"
    rc.write_text(
        "PS1='demo@adash:\\w$ '\n"
        "cd \"$ADASH_DEMO_WORK\" 2>/dev/null || true\n"
        "printf '\\033[32madash demo shell\\033[0m  type help for demo commands\\n'\n"
    )
    dispatch = demo / "session.sh"
    dispatch.write_text(
        f"#!{shell}\n"
        f"cd {shlex.quote(str(work))} || exit 1\n"
        f"case \"$SSH_ORIGINAL_COMMAND\" in\n"
        f"  internal-sftp*) exec {shlex.quote(str(sftp))} -d {shlex.quote(str(work))} ;;\n"
        f"  \"\") exec {shlex.quote(str(bash))} --noprofile --rcfile {shlex.quote(str(rc))} -i ;;\n"
        f"  *) exec {shlex.quote(str(shell))} -c \"$SSH_ORIGINAL_COMMAND\" ;;\n"
        f"esac\n"
    )
    os.chmod(dispatch, 0o700)
    config = demo / "sshd_config"
    quote = lambda value: '"' + str(value).replace("\\", "\\\\").replace('"', '\\"') + '"'
    config.write_text(f"""Port {port}
ListenAddress 127.0.0.1
HostKey {quote(host_key)}
PidFile {quote(demo / 'sshd.pid')}
AuthorizedKeysFile {quote(demo / 'authorized_keys')}
StrictModes no
UsePAM no
PasswordAuthentication no
KbdInteractiveAuthentication no
PubkeyAuthentication yes
AllowUsers {getpass.getuser()}
AllowTcpForwarding yes
Subsystem sftp internal-sftp
ForceCommand {quote(dispatch)}
LogLevel ERROR
""")
    log = (demo / "service.log").open("w")
    sshd_process = subprocess.Popen(
        [sshd, "-D", "-e", "-f", str(config)],
        stdout=log, stderr=log, start_new_session=True,
    )
    time.sleep(0.4)
    if sshd_process.poll() is not None:
        raise SystemExit(f"temporary sshd failed; see {demo / 'service.log'}")
    http_process = subprocess.Popen(
        [sys.executable, "-m", "http.server", str(http_port), "--bind", "127.0.0.1", "--directory", str(site)],
        stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL, start_new_session=True,
    )
    time.sleep(0.2)
    if http_process.poll() is not None:
        http_process = None
        http_port = 0
    profile_path = data / "hosts.json"
    profile = read_profile(profile_path)
    profile["identities"][IDENTITY_ID] = {
        "id": IDENTITY_ID, "name": "Demo Local Key", "kind": "PrivateKey",
        "key_path": str(identity),
    }
    profile["hosts"][DEMO_ID] = {
        "id": DEMO_ID, "name": "Demo Local SSH", "address": "127.0.0.1",
        "port": port, "username": getpass.getuser(), "identity": IDENTITY_ID,
    }
    profile["known_hosts"][f"[127.0.0.1]:{port}"] = fingerprint(host_key.with_name(host_key.name + ".pub"))
    snippets = {
        "demo-uptime": ("Demo: 运行时间", "uptime"),
        "demo-files": ("Demo: 工作目录", "ls -la"),
        "demo-disk": ("Demo: 磁盘", "df -h"),
        "demo-health": ("Demo: 健康检查", "printf 'demo service: healthy\\n'"),
    }
    for snippet_id, (name, command) in snippets.items():
        profile["snippets"].setdefault(snippet_id, {"id": snippet_id, "name": name, "command": command})
    if http_process is not None:
        bind_port = free_port()
        profile["forwards"][FORWARD_ID] = {
            "id": FORWARD_ID, "name": "Demo HTTP", "host_id": DEMO_ID,
            "direction": "Local", "bind_port": bind_port,
            "target_host": "127.0.0.1", "target_port": http_port,
        }
    profile["recent"] = [DEMO_ID] + [item for item in profile.get("recent", []) if item != DEMO_ID]
    write_profile(profile_path, profile)
    state_path.write_text(json.dumps({
        "sshd_pid": sshd_process.pid,
        "http_pid": http_process.pid if http_process else None,
        "ssh_port": port,
        "http_port": http_port,
        "host_name": profile["hosts"][DEMO_ID]["name"],
    }, indent=2) + "\n")
    print(f"adash demo SSH ready on 127.0.0.1:{port}")
    print(f"host: Demo Local SSH · workspace: {work}")
    if http_process is not None:
        print(f"port forward: Demo HTTP · local 127.0.0.1:{profile['forwards'][FORWARD_ID]['bind_port']} → 127.0.0.1:{http_port}")
    print("open 主机 → Demo Local SSH → 连接；stop with --stop")


if __name__ == "__main__":
    main()
