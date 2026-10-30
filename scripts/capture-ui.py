#!/usr/bin/env python3
"""Capture a Mock window on niri without changing the user's account data.

Requires a built adash-app, niri, wl-paste and ImageMagick. Only the process
launched by this script is resized and terminated. XDG_DATA_HOME isolates state.
"""
import argparse
import json
import os
from pathlib import Path
import subprocess
import tempfile
import time

ROOT = Path(__file__).resolve().parents[1]


def windows():
    return json.loads(subprocess.check_output(["niri", "msg", "--json", "windows"]))


def action(name, *arguments):
    subprocess.run(["niri", "msg", "action", name, *arguments], check=True)


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("page", choices=["home", "resources", "flows", "hosts", "identities", "snippets", "port-forward", "settings", "session"])
    parser.add_argument("output", type=Path)
    parser.add_argument("--selected", default="uhost:uhost-shopweb1")
    parser.add_argument("--ask", default="")
    parser.add_argument("--width", type=int, default=1440)
    parser.add_argument("--height", type=int, default=900)
    parser.add_argument("--online", action="store_true", help="start the real SDK agent instead of the local responder")
    args = parser.parse_args()
    previous = next((window["id"] for window in windows() if window["is_focused"]), None)
    with tempfile.TemporaryDirectory(prefix="adash-capture-") as temporary:
        env = os.environ.copy()
        for variable in ["ADASH_DEMO_ASK", "ADASH_DEMO_REPLAY"]:
            env.pop(variable, None)
        env.update(
            UDASH_MOCK="1", XDG_DATA_HOME=temporary,
            ADASH_START_PAGE=args.page, ADASH_START_FLOW="shop-api",
            ADASH_START_SELECTED=args.selected,
        )
        if not args.online:
            env["ADASH_OFFLINE"] = "1"
        if args.ask:
            env["ADASH_DEMO_ASK"] = args.ask
        with tempfile.TemporaryFile() as log:
            process = subprocess.Popen([str(ROOT / "target/debug/adash-app")], cwd=ROOT,
                                       env=env, stdout=log, stderr=log)
            try:
                window = None
                for _ in range(100):
                    window = next((item for item in windows() if item["pid"] == process.pid), None)
                    if window:
                        break
                    if process.poll() is not None:
                        raise RuntimeError("adash-app exited before creating a window")
                    time.sleep(0.1)
                if window is None:
                    raise RuntimeError("adash-app did not create a window within 10 seconds")
                window_id = str(window["id"])
                if not window["is_floating"]:
                    action("toggle-window-floating", "--id", window_id)
                action("set-window-width", "--id", window_id, str(args.width))
                action("set-window-height", "--id", window_id, str(args.height))
                action("focus-window", "--id", window_id)
                time.sleep(4)
                action("focus-window", "--id", window_id)
                time.sleep(0.15)
                action("screenshot-window", "--id", window_id)
                time.sleep(2)
                raw = Path(temporary) / "window.png"
                raw.write_bytes(subprocess.check_output(["wl-paste", "--type", "image/png"]))
                args.output.parent.mkdir(parents=True, exist_ok=True)
                # Compositor shadow is outside the window. Crop its even inset,
                # then normalize the output to logical pixels for documentation.
                width, height = map(int, subprocess.check_output(
                    ["magick", "identify", "-format", "%w %h", str(raw)], text=True).split())
                # niri adds a 20 logical pixel shadow on each side.
                scale = min(width / (args.width + 40), height / (args.height + 40))
                inset = round(20 * scale)
                subprocess.run(["magick", str(raw), "-crop",
                                f"{round(args.width * scale)}x{round(args.height * scale)}+{inset}+{inset}",
                                "+repage", "-resize", f"{args.width}x{args.height}!", str(args.output)], check=True)
                print(args.output)
            finally:
                process.terminate()
                try:
                    process.wait(timeout=8)
                except subprocess.TimeoutExpired:
                    process.kill()
                    process.wait()
                if previous is not None and any(item["id"] == previous for item in windows()):
                    action("focus-window", "--id", str(previous))


if __name__ == "__main__":
    main()
