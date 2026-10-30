#!/usr/bin/env python3
"""Build both binaries and install the current checkout in the user's launcher."""
import json
import os
from pathlib import Path
import shlex
import shutil
import subprocess
import sys
import tempfile

ROOT = Path(__file__).resolve().parents[1]


def atomic_write(path, content, mode=0o644):
    path.parent.mkdir(parents=True, exist_ok=True)
    temporary = None
    try:
        with tempfile.NamedTemporaryFile(mode="w", dir=path.parent, delete=False) as output:
            temporary = Path(output.name)
            output.write(content)
            output.flush()
            os.fchmod(output.fileno(), mode)
        temporary.replace(path)
    finally:
        if temporary is not None:
            temporary.unlink(missing_ok=True)


def desktop_value(value):
    return str(value).replace("\\", "\\\\").replace("\n", "\\n").replace("\r", "\\r")


def desktop_exec(path):
    # Exec is parsed by the desktop launcher, not by a shell.
    escaped = str(path).replace("%", "%%")
    for character in ["\\", '"', "`", "$"]:
        escaped = escaped.replace(character, "\\" + character)
    return '"' + escaped + '"'


def main():
    # The embedded pi SDK host is TypeScript; build it before the desktop entry
    # so the launcher never starts an application with a stale agent runtime.
    runtime = ROOT / "agent/runtime"
    if not (runtime / "node_modules").is_dir():
        subprocess.run(["npm", "--prefix", str(runtime), "install", "--no-audit", "--no-fund"], check=True)
    subprocess.run(["npm", "--prefix", str(runtime), "run", "build"], check=True)
    extension = ROOT / "agent/extensions/ucloud"
    if not (extension / "node_modules").is_dir():
        subprocess.run(["npm", "--prefix", str(extension), "install", "--no-audit", "--no-fund"], check=True)

    binaries = {}
    build = subprocess.Popen(
        ["cargo", "build", "-p", "adash-app", "-p", "adash-tool",
         "--message-format=json-render-diagnostics"],
        cwd=ROOT, stdout=subprocess.PIPE, text=True,
    )
    for line in build.stdout:
        record = json.loads(line)
        if record.get("reason") == "compiler-message":
            rendered = record.get("message", {}).get("rendered")
            if rendered:
                print(rendered, file=sys.stderr, end="")
        if record.get("reason") == "compiler-artifact" and record.get("executable"):
            binaries[record["target"]["name"]] = Path(record["executable"]).resolve()
    if build.wait():
        raise SystemExit(build.returncode)
    if not all(name in binaries for name in ("adash-app", "adash-tool")):
        raise SystemExit("Build did not produce both adash-app and adash-tool")

    # Resolve Cargo's actual output directory (including CARGO_TARGET_DIR).
    bundle = binaries["adash-app"].parent.parent / "desktop"
    launcher = bundle / "launch-adash"
    atomic_write(launcher, "\n".join([
        "#!/bin/sh", "set -eu",
        "cd " + shlex.quote(str(ROOT)),
        "export ADASH_ROOT=" + shlex.quote(str(ROOT)),
        "export ADASH_TOOL=" + shlex.quote(str(binaries["adash-tool"])),
        "export UDASH_MOCK=1",
        "unset ADASH_START_PAGE ADASH_START_SELECTED ADASH_START_FLOW ADASH_DEMO_ASK ADASH_DEMO_REPLAY ADASH_OFFLINE",
        "exec " + shlex.quote(str(binaries["adash-app"])), "",
    ]), 0o755)
    desktop = "\n".join([
        "[Desktop Entry]", "Type=Application", "Version=1.0",
        "Name=adash", "GenericName=UCloud Agent Dashboard",
        "Comment=Cloud resources, conversations and service graphs (Mock)",
        "Comment[zh_CN]=云资源、代理对话与服务图（Mock 演示账户）",
        "Name[zh_CN]=adash 云管家",
        "Exec=" + desktop_exec(launcher),
        "Icon=adash",
        "StartupWMClass=adash",
        "Terminal=false", "Categories=Development;",
        "Keywords=adash;UCloud;cloud;agent;云资源;", "StartupNotify=false", "",
    ])
    # Install the icon under hicolor so launchers that do not accept absolute
    # SVG paths still resolve it by name.
    icons = Path(os.environ.get("XDG_DATA_HOME", str(Path.home() / ".local/share"))) / "icons/hicolor/scalable/apps"
    atomic_write(icons / "adash.svg", (ROOT / "assets/adash.svg").read_text())
    artifact = bundle / "adash.desktop"
    atomic_write(artifact, desktop)
    validator = shutil.which("desktop-file-validate")
    if validator:
        subprocess.run([validator, str(artifact)], check=True)
    applications = Path(os.environ.get("XDG_DATA_HOME", str(Path.home() / ".local/share"))) / "applications"
    installed = applications / "adash.desktop"
    atomic_write(installed, desktop)
    updater = shutil.which("update-desktop-database")
    if updater:
        subprocess.run([updater, str(applications)], check=True)
    print(f"Desktop artifact: {artifact}\nLauncher installed: {installed}")


if __name__ == "__main__":
    main()
