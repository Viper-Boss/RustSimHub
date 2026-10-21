# SPDX-License-Identifier: AGPL-3.0-only
"""Real ext4 mount plus hidden /dev; no physical device or target board access."""
import json
import os
import pathlib
import shlex
import shutil
import subprocess
import sys
import tempfile
import unittest

SCRIPT = pathlib.Path(__file__).resolve().parents[2] / 'firmware/system/overlay/usr/local/libexec/rustsimhub-state-check'
SSH_PREPARE = SCRIPT.with_name('rustsimhub-ssh-prepare')


def worker():
    def command(*args):
        try:
            return subprocess.check_output(args, text=True, stderr=subprocess.STDOUT).strip()
        except subprocess.CalledProcessError as exc:
            raise AssertionError('Fixture command failed: '+str(args)+'\n'+exc.output) from exc

    with tempfile.TemporaryDirectory(prefix='rustsimhub-state-sandbox-') as folder:
        base = pathlib.Path(folder)
        image = base/'fixture.ext4'
        with image.open('xb') as file:
            file.truncate(16*1024*1024)
        command('mkfs.ext4', '-q', '-F', '-L', 'NSHSTATE', str(image))
        state = base/'state'
        state.mkdir()
        mounted = hidden = protected_bound = system_bound = False
        protected = base/'protected'
        protected.mkdir()
        try:
            command('mount', '-o', 'loop', '-t', 'ext4', str(image), str(state))
            mounted = True
            source = command('findmnt', '-rn', '-M', str(state), '-o', 'SOURCE')
            number = command('findmnt', '-rn', '-M', str(state), '-o', 'MAJ:MIN')
            assert pathlib.Path(source).is_block_device()
            block = base/'sys-block'
            disk = block/'mmcblk0'
            part = disk/'mmcblk0p15'
            part.mkdir(parents=True)
            (block/'mmcblk0p15').symlink_to(part, target_is_directory=True)
            for name, value in {'dev':number, 'partition':'15', 'size':'264159', 'uevent':'PARTNAME=NSHSTATE'}.items():
                (part/name).write_text(value+'\n')
            compatible = base/'compatible'
            compatible.write_bytes(b'thwc,ufi003\0qcom,msm8916\0')
            guard = base/'guard'
            guard.write_text(SCRIPT.read_text().replace('/var/lib/rustsimhub', str(state))
                             .replace('/sys/class/block', str(block))
                             .replace('/proc/device-tree/compatible', str(compatible)))
            command('sh', str(guard))
            # Exercise the real SSH preparer with its real guard and selected
            # synthetic keys, not a guard stub. Reproduce the service's narrow
            # writable subtree under a read-only parent bind.
            system=state/'rustsimhub-v1/system'
            system.mkdir(parents=True,mode=0o700)
            system.parent.chmod(0o700);system.chmod(0o700)
            access=system/'ssh-access';access.mkdir(mode=0o700)
            command('ssh-keygen','-q','-t','ed25519','-N','','-f',str(base/'client'))
            key=access/'authorized_keys';key.write_bytes((base/'client.pub').read_bytes());key.chmod(0o600)
            prepare=base/'prepare'
            prepare.write_text(SSH_PREPARE.read_text().replace('/usr/local/libexec/rustsimhub-state-check',shlex.join(['sh',str(guard)]))
                               .replace('/var/lib/rustsimhub',str(state)))
            command('sh',str(prepare))
            selected_host=(system/'ssh-host/key').read_bytes()
            command('mount','--bind',str(state),str(protected));protected_bound=True
            command('mount','--bind',str(system),str(protected/'rustsimhub-v1/system'));system_bound=True
            command('mount','-o','remount,bind,ro',str(protected))
            protected_guard=base/'protected-guard'
            protected_guard.write_text(guard.read_text().replace(str(state),str(protected)))
            protected_prepare=base/'protected-prepare'
            protected_prepare.write_text(prepare.read_text().replace(str(guard),str(protected_guard)).replace(str(state),str(protected)))
            assert subprocess.run(['sh',str(protected_guard)],capture_output=True).returncode!=0
            command('sh',str(protected_guard),'--system-writer')
            # Real .mount units wait for their udev .device. A direct loop mount
            # can race label probing after a previously used loop number. Seed
            # and verify metadata for this fixture's own loop before hiding /dev.
            device = pathlib.Path('/sys/dev/block')/number
            loop_name = device.resolve().name
            assert loop_name.startswith('loop') and loop_name[4:].isdigit()
            assert pathlib.Path(command('losetup','-n','-O','BACK-FILE',source)).resolve() == image.resolve()
            command('udevadm','trigger','--action=change','--settle',str(device.resolve()))
            properties = dict(line.split('=',1) for line in command(
                'udevadm','info','--query=property','--path='+str(device.resolve())).splitlines() if '=' in line)
            assert properties.get('ID_FS_LABEL') == 'NSHSTATE'
            # Mount namespace is private. This reproduces PrivateDevices hiding
            # the physical device while leaving the already mounted state intact.
            command('mount', '-t', 'tmpfs', '-o', 'mode=0755,nosuid,noexec', 'tmpfs', '/dev')
            hidden = True
            for name,minor in [('null',3),('random',8),('urandom',9)]:
                os.mknod('/dev/'+name,0o20666,os.makedev(1,minor))
            assert not pathlib.Path(source).exists()
            command('sh', '-x', str(guard))
            isolated=['setpriv','--bounding-set=-all','--inh-caps=-all','--ambient-caps=-all']
            command(*isolated,'sh',str(protected_prepare))
            assert (system/'ssh-host/key').read_bytes()==selected_host
            denied=subprocess.run([*isolated,'touch',str(protected/'outside-system')],capture_output=True)
            assert denied.returncode!=0 and not (state/'outside-system').exists()
            (part/'dev').write_text('0:0\n')
            wrong = subprocess.run(['sh', str(guard)], capture_output=True)
            assert wrong.returncode != 0, 'Wrong mounted device accepted'
            (part/'dev').write_text(number+'\n')
            command('mount', '-o', 'remount,ro', str(state))
            readonly = subprocess.run(['sh', str(guard)], capture_output=True)
            assert readonly.returncode != 0, 'Readonly state accepted'
            print(json.dumps({'hidden_device_guard_passed':True, 'wrong_device_rejected':True,
                              'readonly_state_rejected':True, 'physical_devices_accessed':False,
                              'real_ssh_guard_readonly_parent_writable_system_empty_caps_passed':True,
                              'selected_host_key_preserved':True,'outside_system_write_denied':True}))
        finally:
            if hidden:
                command('umount', '/dev')
            if system_bound:
                command('umount',str(protected/'rustsimhub-v1/system'))
            if protected_bound:
                command('umount',str(protected))
            if mounted:
                command('umount', str(state))
            assert not command('losetup', '-j', str(image)), 'Fixture loop remained attached'


@unittest.skipUnless(hasattr(os, 'geteuid') and os.geteuid() == 0 and
                     all(shutil.which(name) for name in ['unshare','mkfs.ext4','mount','losetup','setpriv','ssh-keygen','udevadm']),
                     'isolated Linux root mount test required')
class StateGuardSandboxTests(unittest.TestCase):
    def test_real_ext4_mount_with_hidden_physical_devices(self):
        result = subprocess.run(['unshare','--mount','--propagation','private',sys.executable,
                                 str(pathlib.Path(__file__).resolve()),'--sandbox-worker'],
                                capture_output=True,text=True,timeout=30)
        self.assertEqual(result.returncode,0,result.stdout+result.stderr)
        self.assertTrue(json.loads(result.stdout)['hidden_device_guard_passed'])


if __name__ == '__main__' and sys.argv[1:] == ['--sandbox-worker']:
    worker()
