#!/usr/bin/env python3
"""Explicit SSH/SFTP runner. Credentials and full reports stay outside Git."""
import argparse
import getpass
import json
import pathlib
import sys

ROOT = pathlib.Path(__file__).resolve().parents[1]
sys.path.insert(0, str(ROOT / '.local' / 'python-deps'))
import paramiko

def main():
    parser = argparse.ArgumentParser()
    parser.add_argument('--config', type=pathlib.Path, required=True)
    parser.add_argument('--script', type=pathlib.Path)
    parser.add_argument('--upload', type=pathlib.Path)
    parser.add_argument('--download')
    parser.add_argument('--destination')
    parser.add_argument('--output', type=pathlib.Path)
    parser.add_argument('--timeout', type=int, default=60)
    parser.add_argument('--sudo', action='store_true')
    args = parser.parse_args()
    if sum(bool(value) for value in (args.script,args.upload,args.download)) != 1:
        parser.error('Choose exactly one of --script, --upload or --download')
    config = json.loads(args.config.read_text(encoding='utf-8-sig'))
    def connect(entry, sock=None):
        ssh = paramiko.SSHClient()
        ssh.load_host_keys(entry.get('known_hosts', str(pathlib.Path.home() / '.ssh' / 'known_hosts')))
        ssh.set_missing_host_key_policy(paramiko.RejectPolicy())
        ssh.connect(entry['host'], username=entry['username'], port=entry.get('port',22),
                    password=entry.get('password'), key_filename=entry.get('key_filename'), sock=sock,
                    timeout=12, auth_timeout=12, banner_timeout=12, look_for_keys=False, allow_agent=False)
        return ssh
    jump = None
    if 'jump' in config:
        jump = connect(config['jump'])
        sock = jump.get_transport().open_channel('direct-tcpip', (config['host'],config.get('port',22)), ('127.0.0.1',0),timeout=12)
        client = connect(config, sock)
    else:
        client = connect(config)
    try:
        if args.upload:
            if not args.destination: parser.error('--upload requires --destination')
            with client.open_sftp() as sftp:
                sftp.put(str(args.upload), args.destination)
            print('Upload complete')
            return
        if args.download:
            if not args.destination: parser.error('--download requires local --destination')
            target = pathlib.Path(args.destination)
            target.parent.mkdir(parents=True,exist_ok=True)
            with client.open_sftp() as sftp:
                sftp.get(args.download,str(target))
            print('Download complete')
            return
        command = "sudo -S -p '' sh -s" if args.sudo else 'sh -s'
        stdin, stdout, stderr = client.exec_command(command, timeout=args.timeout)
        stdout.channel.set_combine_stderr(True)
        if args.sudo:
            password = config.get('password') or getpass.getpass('sudo password: ')
            stdin.write(password + '\n')
        stdin.write(args.script.read_text(encoding='utf-8')); stdin.channel.shutdown_write()
        # Combine before writing input: short commands may otherwise lose early diagnostics.
        result = stdout.read()
        code = stdout.channel.recv_exit_status()
        if args.output:
            args.output.parent.mkdir(parents=True,exist_ok=True); args.output.write_bytes(result)
            print(f'SSH exit={code}; private report saved')
        else:
            sys.stdout.buffer.write(result)
        raise SystemExit(code)
    finally:
        client.close()
        if jump: jump.close()

if __name__ == '__main__': main()
