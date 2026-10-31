#!/usr/bin/env python3
"""Run the native UI against an isolated loopback sshd; no system service changes."""
import argparse
import base64
import getpass
import hashlib
import json
import os
from pathlib import Path
import shlex
import shutil
import signal
import socket
import subprocess
import tempfile
import time
from support import ROOT, build_binary, executable, sftp_server, capture, normalize_capture, windows, action, stop_process


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--tab', choices=['terminal','files','monitor'], default='terminal')
    parser.add_argument('--capture', type=Path)
    parser.add_argument('--capture-command', help='another desktop adapter with {pid} and {output}')
    parser.add_argument('--untrusted', action='store_true')
    parser.add_argument('--dialog', action='store_true')
    args = parser.parse_args()
    app_binary = build_binary('adash-app', test_hooks=True)
    sshd = executable('sshd', 'ADASH_TEST_SSHD')
    keygen = executable('ssh-keygen', 'ADASH_TEST_SSH_KEYGEN')
    bash = executable('bash', 'ADASH_TEST_BASH')
    shell = executable('sh', 'ADASH_TEST_SHELL')
    sftp = sftp_server(sshd)
    previous = next((window['id'] for window in windows() if window['is_focused']), None)
    with tempfile.TemporaryDirectory(prefix='adash-ssh-ui-') as temporary:
        directory = Path(temporary)
        work = directory / 'work'
        work.mkdir()
        (work / 'readme.txt').write_text('SSH/SFTP native UI verification\n颜色与中文必须保留在同一行。\n')
        (work / 'logs').mkdir()
        for name in ['identity','host']:
            subprocess.run([str(keygen),'-q','-t','ed25519','-N','','-f',str(directory / name)],check=True)
        shutil.copyfile(directory / 'identity.pub', directory / 'authorized_keys')
        with socket.socket() as probe:
            probe.bind(('127.0.0.1',0))
            port = probe.getsockname()[1]
        rc = directory / 'bashrc'
        rc.write_text(f"HISTFILE={shlex.quote(str(directory / 'history'))}\nPS1='local-test:\\w$ '\nprintf '\\033[32mSSH connected\\033[0m  two spaces  中文\\n'\n")
        dispatch = directory / 'session.sh'
        dispatch.write_text(f'#!{shell}\ncd {shlex.quote(str(work))} || exit 1\nif [ -z "$SSH_ORIGINAL_COMMAND" ]; then exec {shlex.quote(str(bash))} --noprofile --rcfile {shlex.quote(str(rc))} -i; else exec {shlex.quote(str(shell))} -c "$SSH_ORIGINAL_COMMAND"; fi\n')
        config = directory / 'sshd_config'
        quote = lambda path: '"' + str(path).replace('\\','\\\\').replace('"','\\"') + '"'
        config.write_text(f'''Port {port}
ListenAddress 127.0.0.1
HostKey {quote(directory / 'host')}
PidFile {quote(directory / 'sshd.pid')}
AuthorizedKeysFile {quote(directory / 'authorized_keys')}
StrictModes no
UsePAM no
PasswordAuthentication no
KbdInteractiveAuthentication no
PubkeyAuthentication yes
AllowUsers {getpass.getuser()}
AllowTcpForwarding yes
Subsystem sftp {quote(sftp)}
ForceCommand {quote(shell)} {quote(dispatch)}
LogLevel ERROR
''')
        data = directory / 'data'
        data.mkdir()
        fingerprint = 'SHA256:' + base64.b64encode(hashlib.sha256(base64.b64decode((directory / 'host.pub').read_text().split()[1])).digest()).decode().rstrip('=')
        identity_id = 'native-' + directory.name
        profiles = {
            'hosts': {'local-test': {'id':'local-test','name':'Local SSH verification','address':'127.0.0.1','port':port,'username':getpass.getuser(),'identity':identity_id}},
            'identities': {identity_id:{'id':identity_id,'name':'Temporary test key','kind':'PrivateKey','key_path':str(directory / 'identity')}},
            'known_hosts': {} if args.untrusted else {f'[127.0.0.1]:{port}':fingerprint},
        }
        profile = data / 'hosts.json'
        profile.write_text(json.dumps(profiles)); profile.chmod(0o600)
        env = {key:value for key,value in os.environ.items() if not key.startswith(('ADASH_START_','ADASH_DEMO_'))}
        env.update(ADASH_DATA_DIR=str(data),UDASH_MOCK='1',ADASH_OFFLINE='1',ADASH_START_HOST='local-test',ADASH_START_REMOTE_DIR=str(work),ADASH_START_SESSION_TAB=str(['terminal','files','monitor'].index(args.tab)))
        if args.dialog:
            env['ADASH_DEMO_DIALOG']='1'
        log_path = directory / 'verification.log'
        with log_path.open('w') as log:
            server = subprocess.Popen([str(sshd),'-D','-e','-f',str(config)],stdout=log,stderr=log,start_new_session=True)
            app = None
            try:
                time.sleep(0.3)
                if server.poll() is not None:
                    raise RuntimeError('Temporary sshd failed: ' + log_path.read_text())
                app = subprocess.Popen([str(app_binary)],cwd=ROOT,env=env,stdout=log,stderr=log,start_new_session=True)
                print(json.dumps({'app_pid':app.pid,'server_pid':server.pid,'directory':str(directory)}),flush=True)
                if args.capture:
                    raw = directory / 'capture.png'
                    capture(app,raw,1440,900,delay=12 if args.tab=='monitor' else 5,command=args.capture_command)
                    normalize_capture(raw,args.capture,1440,900)
                    print(args.capture)
                else:
                    app.wait()
            finally:
                for process in [app,server]:
                    if process is not None:
                        stop_process(process)
                if previous is not None and any(window['id']==previous for window in windows()):
                    action('focus-window','--id',previous)


if __name__=='__main__':
    signal.signal(signal.SIGTERM,lambda *_: (_ for _ in ()).throw(KeyboardInterrupt()))
    try:
        main()
    except KeyboardInterrupt:
        pass
