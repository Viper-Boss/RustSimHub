#!/usr/bin/env python3
# SPDX-License-Identifier: AGPL-3.0-only
import argparse,json,pathlib,re,shlex,sys
root=pathlib.Path(__file__).resolve().parents[1];sys.path.insert(0,str(root/'.local/python-deps'))
import paramiko
from cryptography.hazmat.primitives import serialization
from cryptography.hazmat.primitives.asymmetric.ed25519 import Ed25519PrivateKey
p=argparse.ArgumentParser(description='Synthetic key maintenance login in candidate private loopback; never modifies host SSH')
p.add_argument('--device',type=pathlib.Path,required=True)
p.add_argument('--report',type=pathlib.Path,required=True)
p.add_argument('--root-directory',required=True)
a=p.parse_args()
assert re.fullmatch('/var/tmp/rustsimhub-system-tree-[A-Za-z0-9]{8}/rootfs',a.root_directory)
cfg=json.loads(a.device.read_text(encoding='utf-8-sig'))
def connect(c,sock=None):
 s=paramiko.SSHClient();s.load_host_keys(c['known_hosts']);s.set_missing_host_key_policy(paramiko.RejectPolicy());s.connect(c['host'],port=c.get('port',22),username=c['username'],password=c.get('password'),key_filename=c.get('key_filename'),sock=sock,timeout=12,look_for_keys=False,allow_agent=False);return s
def run(s,body):
 i,o,e=s.exec_command('sh -s',timeout=45);o.channel.set_combine_stderr(True);i.write('set -eu\numask 077\n'+body+'\n');i.channel.shutdown_write();raw=o.read().decode(errors='replace');code=o.channel.recv_exit_status()
 if code:report['failed_output']=raw;raise RuntimeError('Private SSH login check failed; inspect report')
 return raw
report={'passed':False,'production_ssh_changed':False,'actual_state_guard_run':False,'real_maintenance_key_installed':False,'private_network_only':True,'new_calls':0,'new_sms':0,'cellular_tests':False}
path=a.report;assert not path.exists();nas=dev=None
try:
 nas=connect(cfg['jump']);dev=connect(cfg,nas.get_transport().open_channel('direct-tcpip',(cfg['host'],cfg.get('port',22)),('127.0.0.1',0),timeout=12))
 folder=run(dev,'d=$(mktemp -d /var/tmp/rustsimhub-ssh-login-XXXXXXXX);chmod 0700 "$d";mkdir -m 0700 "$d/system" "$d/system/ssh-access";echo "$d"').strip();assert re.fullmatch('/var/tmp/rustsimhub-ssh-login-[A-Za-z0-9]{8}',folder)
 report['directory']=folder
 key=Ed25519PrivateKey.generate();public=key.public_key().public_bytes(serialization.Encoding.OpenSSH,serialization.PublicFormat.OpenSSH)+b' synthetic-test\n'
 private=key.private_bytes(serialization.Encoding.PEM,serialization.PrivateFormat.OpenSSH,serialization.NoEncryption())
 wrong=Ed25519PrivateKey.generate()
 wrong_private=wrong.private_bytes(serialization.Encoding.PEM,serialization.PrivateFormat.OpenSSH,serialization.NoEncryption())
 wrong_public=wrong.public_key().public_bytes(serialization.Encoding.OpenSSH,serialization.PublicFormat.OpenSSH)+b' synthetic-replacement\n'
 prefix='firmware/system/overlay/'
 prepare=(root/(prefix+'usr/local/libexec/rustsimhub-ssh-prepare')).read_text().replace('/usr/local/libexec/rustsimhub-state-check','/bin/true').replace('/var/lib/rustsimhub/rustsimhub-v1/system','/mnt/probe/system')
 config=(root/(prefix+'etc/ssh/sshd_config.d/20-rustsimhub.conf')).read_text().replace('/var/lib/rustsimhub/rustsimhub-v1/system','/mnt/probe/system')
 config+='\nPort 2222\nListenAddress 127.0.0.1\nPidFile /run/probe-sshd.pid\nUsePAM yes\n'
 with dev.open_sftp() as s:
  for name,data in [('prepare',prepare.encode()),('sshd.conf',config.encode()),('client',private),('wrong-client',wrong_private),('replacement.pub',wrong_public),('system/ssh-access/authorized_keys',public)]:
   with s.open(folder+'/'+name,'wx') as f:f.write(data)
   s.chmod(folder+'/'+name,0o600)
 script='''set -eu
umask 077
mkdir -m 0755 /run/sshd
sh /mnt/probe/prepare
printf '[127.0.0.1]:2222 %s\n' "$(cat /mnt/probe/system/ssh-host/key.pub)" > /mnt/probe/known_hosts
/usr/sbin/sshd -t -f /mnt/probe/sshd.conf
/usr/sbin/sshd -D -e -f /mnt/probe/sshd.conf > /mnt/probe/server.log 2>&1 &
server=$!
cleanup(){ kill "$server" 2>/dev/null || true;wait "$server" 2>/dev/null || true; }
trap cleanup EXIT HUP INT TERM
sleep .3
kill -0 "$server"
login(){ /usr/bin/ssh -F /dev/null -T -p 2222 -i "$1" -o BatchMode=yes -o StrictHostKeyChecking=yes -o "UserKnownHostsFile=$2" -o GlobalKnownHostsFile=/dev/null -o IdentityAgent=none -o IdentitiesOnly=yes -o ConnectTimeout=5 root@127.0.0.1 /usr/bin/id -u; }
login /mnt/probe/client /mnt/probe/known_hosts > /mnt/probe/login.out 2> /mnt/probe/client.log
test "$(cat /mnt/probe/login.out)" = 0
printf '[127.0.0.1]:2222 %s\n' "$(cat /mnt/probe/replacement.pub)" > /mnt/probe/wrong_hosts
if login /mnt/probe/client /mnt/probe/wrong_hosts > /mnt/probe/wrong-pin.out 2>&1;then exit 13;fi
if login /mnt/probe/wrong-client /mnt/probe/known_hosts > /mnt/probe/wrong-key.out 2>&1;then exit 13;fi
before=$(sha256sum /mnt/probe/system/ssh-host/key)
cp /mnt/probe/replacement.pub /mnt/probe/system/ssh-access/authorized_keys.next
chmod 0600 /mnt/probe/system/ssh-access/authorized_keys.next
mv -T /mnt/probe/system/ssh-access/authorized_keys.next /mnt/probe/system/ssh-access/authorized_keys
if login /mnt/probe/client /mnt/probe/known_hosts > /mnt/probe/revoked-key.out 2>&1;then exit 13;fi
login /mnt/probe/wrong-client /mnt/probe/known_hosts > /mnt/probe/replacement.out 2> /mnt/probe/replacement.log
test "$(cat /mnt/probe/replacement.out)" = 0
test "$before" = "$(sha256sum /mnt/probe/system/ssh-host/key)"
echo CANDIDATE_ROOT_PUBLIC_KEY_LOGIN_WITH_EXACT_HOST_PIN_PASSED
echo WRONG_HOST_PIN_WRONG_CLIENT_AND_REVOKED_KEY_DENIED
cleanup
trap - EXIT HUP INT TERM
'''
 cmd=['systemd-run','--quiet','--wait','--pipe','--collect','--expand-environment=no','--unit=rustsimhub-ssh-login-'+folder[-8:]]
 for prop in ['RootDirectory='+a.root_directory,'MountAPIVFS=yes','PrivateDevices=yes','PrivateNetwork=yes','ProtectSystem=strict','PrivateTmp=yes','NoNewPrivileges=yes','CapabilityBoundingSet=CAP_CHOWN CAP_DAC_OVERRIDE CAP_SETUID CAP_SETGID CAP_SYS_CHROOT CAP_AUDIT_WRITE','AmbientCapabilities=','TemporaryFileSystem=/mnt:mode=0755 /run:mode=0755','BindPaths='+folder+':/mnt/probe','RuntimeMaxSec=30']:cmd+=['-p',prop]
 output=run(dev,shlex.join(cmd+['/bin/sh','-ec',script]));report['private_output']=output
 assert 'CANDIDATE_ROOT_PUBLIC_KEY_LOGIN_WITH_EXACT_HOST_PIN_PASSED' in output
 assert 'WRONG_HOST_PIN_WRONG_CLIENT_AND_REVOKED_KEY_DENIED' in output
 report['wrong_host_pin_denied']=True;report['wrong_key_denied']=True;report['revoked_key_denied']=True;report['replacement_key_accepted_with_stable_host_identity']=True
 old=run(dev,'systemctl is-active nassimhub-agent ModemManager NetworkManager rmtfs\nfor p in /proc/asound/card*/pcm*/sub*/status;do test "$(cat "$p")" = closed;done');assert old.splitlines()==['active']*4
 report['passed']=True;report['cleanup_verified']=True
 print('Candidate private SSH login, wrong host pin/key denial and key revocation/replacement passed; stable host identity, transient server stopped')
finally:
 path.write_text(json.dumps(report,indent=2),encoding='utf-8')
 if dev:dev.close()
 if nas:nas.close()
