#!/usr/bin/env python3
"""Capture an isolated Mock window with an explicit test build and desktop adapter."""
import argparse
import os
from pathlib import Path
import shutil
import subprocess
import tempfile
from support import ROOT, build_binary, capture, normalize_capture, windows, action, stop_process, user_data_dir


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('page', choices=['home','resources','flows','hosts','identities','snippets','port-forward','account','settings'])
    parser.add_argument('output', type=Path)
    parser.add_argument('--selected', default='uhost:uhost-shopweb1')
    parser.add_argument('--flow', default='')
    parser.add_argument('--settings', choices=['general','terminal','connection'], default='general')
    parser.add_argument('--appearance', choices=['light','dark','system'], default='light')
    parser.add_argument('--ask', default='')
    parser.add_argument('--width', type=int, default=1440)
    parser.add_argument('--height', type=int, default=900)
    parser.add_argument('--online', action='store_true', help='use the SDK startup path instead of the test-build offline shortcut')
    parser.add_argument('--use-ai-config', action='store_true', help='copy your real AI endpoint/model config into the isolated run (never the key)')
    parser.add_argument('--capture-command', help='optional screenshot command with {pid} and {output} for another desktop')
    args = parser.parse_args()
    app = build_binary('adash-app', test_hooks=True)
    tool = build_binary('adash-tool')
    previous = next((window['id'] for window in windows() if window['is_focused']), None)
    config = user_data_dir() / 'ai.json'
    with tempfile.TemporaryDirectory(prefix='adash-capture-') as temporary:
        data = Path(temporary) / 'data'
        data.mkdir()
        if args.use_ai_config and config.is_file():
            shutil.copyfile(config, data / 'ai.json')
        env = {key: value for key, value in os.environ.items() if not key.startswith(('ADASH_START_', 'ADASH_DEMO_'))}
        env.update(UDASH_MOCK='1', ADASH_DATA_DIR=str(data), ADASH_TOOL=str(tool),
                   ADASH_START_PAGE=args.page,
                   ADASH_START_SETTINGS=str(['general','terminal','connection'].index(args.settings)),
                   ADASH_DEMO_APPEARANCE=args.appearance,
                   ADASH_START_WIDTH=str(args.width), ADASH_START_HEIGHT=str(args.height))
        for key, value in [('ADASH_START_SELECTED', args.selected), ('ADASH_START_FLOW', args.flow), ('ADASH_DEMO_ASK', args.ask)]:
            if value:
                env[key] = value
        env.pop('ADASH_OFFLINE', None)
        if not args.online:
            env['ADASH_OFFLINE'] = '1'
        with tempfile.TemporaryFile() as log:
            process = subprocess.Popen([str(app)], cwd=ROOT, env=env, stdout=log, stderr=log, start_new_session=True)
            try:
                raw = Path(temporary) / 'window.png'
                capture(process, raw, args.width, args.height, command=args.capture_command)
                normalize_capture(raw, args.output, args.width, args.height)
                print(args.output)
            finally:
                stop_process(process)
                if previous is not None and any(window['id'] == previous for window in windows()):
                    action('focus-window', '--id', previous)


if __name__ == '__main__':
    main()
