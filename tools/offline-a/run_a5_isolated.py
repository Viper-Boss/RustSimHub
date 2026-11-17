#!/usr/bin/env python3
# SPDX-License-Identifier: AGPL-3.0-only
"""Root does namespace/tmpfs setup only; every test runs as a non-root UID."""
import argparse
import os
from pathlib import Path
import pwd
import subprocess
import sys
import tempfile


def main():
    p = argparse.ArgumentParser(description=__doc__)
    p.add_argument('--uid', type=int, required=True)
    p.add_argument('--mode', choices=['main','a2','a3','a4','sdk','usb','nas','sms','enospc'], required=True)
    p.add_argument('--inner', action='store_true', help=argparse.SUPPRESS)
    p.add_argument('arguments', nargs=argparse.REMAINDER)
    a = p.parse_args()
    assert os.geteuid() == 0 and a.uid > 0
    user = pwd.getpwuid(a.uid); assert user.pw_gid != 0
    args = a.arguments[1:] if a.arguments[:1] == ['--'] else a.arguments
    here = Path(__file__).resolve().parent
    if not a.inner:
        env = dict(os.environ, RSH_A3_PARENT_MNT=os.readlink('/proc/self/ns/mnt'))
        command = ['unshare','--net','--fork']
        if a.mode == 'enospc':
            command += ['--mount','--propagation','private']
        command += ['--',sys.executable,str(Path(__file__).resolve()),'--inner','--uid',str(a.uid),'--mode',a.mode,'--',*args]
        return subprocess.run(command, env=env, timeout=3600).returncode
    assert os.readlink('/proc/self/ns/net') != os.readlink('/proc/1/ns/net')
    subprocess.run(['ip','link','set','lo','up'], check=True, timeout=5)
    env = dict(os.environ, HOME=user.pw_dir, PATH=user.pw_dir+'/.cargo/bin:'+os.environ.get('PATH','/usr/bin:/bin'))
    mode = a.mode
    if mode in ['main','a2','a3','a4']:
        runner = 'validate_a5.py'; prefix = ['--profile',mode]
    elif mode == 'sdk':
        runner = 'validate_sdk_a5.py'; prefix = []
    else:
        runner = 'runtime_a5.py'; prefix = ['--mode',mode]
    script = here / runner
    assert script.is_file() and not script.is_symlink()
    command = ['setpriv','--reuid='+str(a.uid),'--regid='+str(user.pw_gid),'--clear-groups',
               '--bounding-set=-all','--inh-caps=-all','--ambient-caps=-all','--no-new-privs',
               sys.executable,str(script),*prefix,*args]
    mount = None
    try:
        if mode == 'enospc':
            assert os.readlink('/proc/self/ns/mnt') != env['RSH_A3_PARENT_MNT']
            mount = Path(tempfile.mkdtemp(prefix='rsh-a3-sms-', dir='/var/tmp')).resolve()
            assert mount.parent == Path('/var/tmp') and mount.name.startswith('rsh-a3-sms-')
            subprocess.run(['mount','-t','tmpfs','-o',f'size=16m,mode=0700,uid={a.uid},gid={user.pw_gid},nosuid,nodev,noexec',
                            'rsh-a5-enospc',str(mount)], check=True, timeout=5)
            env.update(RSH_A3_ENOSPC='1',RSH_A3_SMS_FS=str(mount))
        return subprocess.run(command, env=env, timeout=3300).returncode
    finally:
        if mount:
            subprocess.run(['umount',str(mount)], check=True, timeout=5)
            os.rmdir(mount)


if __name__ == '__main__':
    raise SystemExit(main())
