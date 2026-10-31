#!/usr/bin/env python3
"""Build a relocatable application directory and, on Linux, its desktop entry."""
import argparse
import os
from pathlib import Path
import shutil
import subprocess
import sys
import tempfile
from support import ROOT, build_binary


def atomic_write(path, content, mode=0o644):
    path.parent.mkdir(parents=True, exist_ok=True)
    with tempfile.NamedTemporaryFile(mode="w", dir=path.parent, delete=False) as output:
        temporary = Path(output.name)
        try:
            output.write(content)
            output.flush()
            os.fchmod(output.fileno(), mode)
            temporary.replace(path)
        finally:
            temporary.unlink(missing_ok=True)


def desktop_exec(path):
    escaped = str(path).replace("%", "%%")
    for character in ["\\", '"', "`", "$"]:
        escaped = escaped.replace(character, "\\" + character)
    return '"' + escaped + '"'


def copy_binary(source, target):
    temporary = target.with_suffix(".new")
    shutil.copy2(source, temporary)
    temporary.replace(target)


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--output", type=Path, help="application directory; defaults to Cargo's actual target directory/desktop")
    parser.add_argument("--no-install", action="store_true", help="build without changing the user's launcher")
    args = parser.parse_args()
    runtime = ROOT / "agent/runtime"
    if not (runtime / "node_modules").is_dir():
        subprocess.run(["npm", "--prefix", str(runtime), "ci", "--no-audit", "--no-fund"], check=True)
    subprocess.run(["npm", "--prefix", str(runtime), "run", "build"], check=True)
    app = build_binary("adash-app")
    tool = build_binary("adash-tool")
    bundle = (args.output or app.parent.parent / "desktop").resolve()
    if bundle in [ROOT, app.parent, tool.parent]:
        raise SystemExit("Choose a separate output directory for the bundle")
    bundle.mkdir(parents=True, exist_ok=True)
    copy_binary(app, bundle / app.name)
    copy_binary(tool, bundle / tool.name)
    for relative in ["agent/runtime/dist", "agent/extensions/ucloud", "agent/skills", "agent/prompts", "assets"]:
        source, target = ROOT / relative, bundle / relative
        if target.exists():
            shutil.rmtree(target)
        shutil.copytree(source, target, ignore=shutil.ignore_patterns("node_modules", "package-lock.json", ".env", "*.local.json", "__pycache__"))
    # One dependency tree resolves both the SDK host and pinned extension.
    modules = bundle / "node_modules"
    if modules.exists():
        shutil.rmtree(modules)
    shutil.copytree(runtime / "node_modules", modules)
    if (ROOT / "LICENSE").is_file():
        shutil.copy2(ROOT / "LICENSE", bundle / "LICENSE")
    launcher = bundle / "launch-adash"
    atomic_write(launcher, '\n'.join([
        '#!/bin/sh', 'set -eu',
        'APP_FILE=$0',
        'while [ -L "$APP_FILE" ]; do',
        '  APP_DIR=$(CDPATH= cd -P -- "$(dirname -- "$APP_FILE")" && pwd)',
        '  APP_FILE=$(readlink "$APP_FILE")',
        '  case "$APP_FILE" in /*) ;; *) APP_FILE="$APP_DIR/$APP_FILE" ;; esac',
        'done',
        'APP_DIR=$(CDPATH= cd -P -- "$(dirname -- "$APP_FILE")" && pwd)',
        'exec "$APP_DIR/' + app.name + '" "$@"', '',
    ]), 0o755)
    desktop = '\n'.join([
        '[Desktop Entry]', 'Type=Application', 'Version=1.0', 'Name=adash', 'Name[zh_CN]=adash 云管家',
        'GenericName=UCloud Agent Dashboard', 'Comment=Cloud resources, conversations and service graphs',
        'Exec=' + desktop_exec(launcher), 'Icon=adash', 'StartupWMClass=adash',
        'Terminal=false', 'Categories=Development;', 'Keywords=adash;UCloud;cloud;agent;云资源;', 'StartupNotify=false', '',
    ])
    atomic_write(bundle / 'adash.desktop', desktop)
    validator = shutil.which('desktop-file-validate')
    if validator:
        subprocess.run([validator, str(bundle / 'adash.desktop')], check=True)
    if not args.no_install and sys.platform.startswith('linux'):
        data = Path(os.environ.get('XDG_DATA_HOME', str(Path.home() / '.local/share')))
        atomic_write(data / 'icons/hicolor/scalable/apps/adash.svg', (ROOT / 'assets/adash.svg').read_text())
        installed = data / 'applications/adash.desktop'
        atomic_write(installed, desktop)
        updater = shutil.which('update-desktop-database')
        if updater:
            subprocess.run([updater, str(installed.parent)], check=True)
        print('Launcher installed:', installed)
    print('Relocatable application directory:', bundle)


if __name__ == '__main__':
    main()
