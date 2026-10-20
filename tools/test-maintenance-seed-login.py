#!/usr/bin/env python3
# SPDX-License-Identifier: AGPL-3.0-only
"""Selected maintenance key acceptance in candidate loopback; client private key stays on PC."""
import argparse,base64,hashlib,io,json,pathlib,re,secrets,shlex,sys
ROOT=pathlib.Path(__file__).resolve().parents[1]
sys.path.insert(0,str(ROOT/'.local/python-deps'))

def main():
    import paramiko
    from cryptography.hazmat.primitives import serialization
    from cryptography.hazmat.primitives.asymmetric.ed25519 import Ed25519PrivateKey
    p=argparse.ArgumentParser(description=__doc__)
    for name in ('device','seed-index','client-key-index','report'):p.add_argument('--'+name,type=pathlib.Path,required=True)
    p.add_argument('--root-directory',required=True)
    a=p.parse_args()
    assert not a.report.exists(),'Use a new private report; never repeat an attempt'
    assert re.fullmatch('/var/tmp/rustsimhub-system-tree-[A-Za-z0-9]{8}/rootfs',a.root_directory)
    selected=json.loads(a.seed_index.read_text());client_index=json.loads(a.client_key_index.read_text())
    manifest=selected['manifest'];seed=selected['nas_directory']
    assert selected['passed'] is True and selected['device_installed'] is False
    assert re.fullmatch('/mnt/docker-data/rustsimhub/p0/restore-inspect-[A-Za-z0-9]{8}/maintenance-seed',seed)
    public=pathlib.Path(client_index['public_key']).read_bytes()
    assert hashlib.sha256(public).hexdigest()==client_index['public_file_sha256']==manifest['maintenance_public_sha256']
    client_key=paramiko.Ed25519Key.from_private_key_file(client_index['private_key'])
    assert public.split()[:2]==[b'ssh-ed25519',client_key.get_base64().encode()]
    cfg=json.loads(a.device.read_text(encoding='utf-8-sig'))
    report={'passed':False,'cleanup_verified':False,'private_loopback_only':True,
            'client_private_key_uploaded':False,'actual_state_guard_run':False,
            'actual_maintenance_installed':False,'production_ssh_changed':False,
            'new_calls':0,'new_sms':0,'cellular_tests':False,'partition_writes':False,
            'maintenance_seed_source':selected['source_revision']}
    nas=dev=None;unit=folder=None;stopped=False
    def save():a.report.write_text(json.dumps(report,indent=2),encoding='utf-8')
    def connect(c,sock=None):
        s=paramiko.SSHClient();s.load_host_keys(c['known_hosts']);s.set_missing_host_key_policy(paramiko.RejectPolicy())
        s.connect(c['host'],port=c.get('port',22),username=c['username'],password=c.get('password'),
                  key_filename=c.get('key_filename'),sock=sock,timeout=10,banner_timeout=10,auth_timeout=10,
                  look_for_keys=False,allow_agent=False)
        return s
    def run(s,body,sudo=False):
        i,o,_=s.exec_command("sudo -S -p '' sh -s" if sudo else 'sh -s',timeout=35);o.channel.set_combine_stderr(True)
        if sudo:i.write(cfg['jump']['password']+'\n')
        i.write('set -eu\numask 077\n'+body+'\n');i.channel.shutdown_write()
        raw=o.read().decode(errors='replace')
        if o.channel.recv_exit_status():
            report['failed_output']=raw;save()
            raise RuntimeError('Maintenance acceptance failed; inspect private report, no automatic rerun')
        return raw
    def baseline():
        raw=run(dev,'''systemctl is-active nassimhub-agent ModemManager NetworkManager rmtfs
for file in /proc/asound/card*/pcm*/sub*/status;do test "$(cat "$file")" = closed;done
test "$(stat -c %u:%g:%a /var/lib/nassimhub)" = 101:103:700
test ! -e /var/lib/nassimhub/rustsimhub-v1
test ! -L /var/lib/nassimhub/rustsimhub-v1
mmcli -m 0 --voice-list-calls
nmcli -t -f TYPE,STATE device''')
        assert raw.splitlines()[:4]==['active']*4 and 'gsm:connected' not in raw
        assert all(line.endswith('(terminated)') for line in raw.splitlines() if '/Call/' in line)
        return raw
    try:
        save();nas=connect(cfg['jump'])
        script='''python3 - <<'PY'
import base64,hashlib,importlib.util,json,pathlib
output=pathlib.Path(SEED)
tool=output.parent/'tools/prepare-maintenance-seed.py'
assert hashlib.sha256(tool.read_bytes()).hexdigest()==TOOL_SHA
spec=importlib.util.spec_from_file_location('seed',tool)
module=importlib.util.module_from_spec(spec);spec.loader.exec_module(module)
manifest,_,_=module.validate_seed(output,PUBLIC_SHA)
names=['ssh-access/authorized_keys','ssh-host/key','ssh-host/key.pub','usb.serial']
data={n:base64.b64encode((output/'rustsimhub-v1/system'/n).read_bytes()).decode() for n in names}
print(json.dumps({'manifest':manifest,'files':data}))
PY'''.replace('SEED',repr(seed)).replace('TOOL_SHA',repr(selected['tool_sha256'])).replace('PUBLIC_SHA',repr(manifest['maintenance_public_sha256']))
        raw=run(nas,script,sudo=True);material=json.loads(raw[raw.index('{'):])
        assert material['manifest']==manifest
        files={n:base64.b64decode(v,validate=True) for n,v in material['files'].items()}
        assert set(files)=={'ssh-access/authorized_keys','ssh-host/key','ssh-host/key.pub','usb.serial'}
        assert files['ssh-access/authorized_keys']==public
        host=paramiko.Ed25519Key(data=base64.b64decode(files['ssh-host/key.pub'].split()[1]))
        fingerprint='SHA256:'+base64.b64encode(hashlib.sha256(host.asbytes()).digest()).decode().rstrip('=')
        assert fingerprint==manifest['host_fingerprint'];report['host_fingerprint']=fingerprint
        dev=connect(cfg,nas.get_transport().open_channel('direct-tcpip',(cfg['host'],cfg.get('port',22)),('127.0.0.1',0),timeout=10))
        report['before']=baseline()
        raw=run(dev,'d=$(mktemp -d /var/tmp/rustsimhub-maintenance-login-XXXXXXXX);chmod 0700 "$d";mkdir -m 0700 "$d/system" "$d/system/ssh-access" "$d/system/ssh-host";printf "DIR=%s\\n" "$d"')
        folder=re.search(r'DIR=(/var/tmp/rustsimhub-maintenance-login-[A-Za-z0-9]{8})',raw).group(1)
        unit='rustsimhub-maintenance-login-'+folder[-8:];port=42000+secrets.randbelow(6000)
        report.update(directory=folder,unit=unit,port=port);save()
        overlay=ROOT/'firmware/system/overlay'
        prepare=(overlay/'usr/local/libexec/rustsimhub-ssh-prepare').read_text().replace(
            '/usr/local/libexec/rustsimhub-state-check','/bin/true').replace(
            '/var/lib/rustsimhub/rustsimhub-v1/system','/mnt/probe/system')
        config=(overlay/'etc/ssh/sshd_config.d/20-rustsimhub.conf').read_text().replace(
            '/var/lib/rustsimhub/rustsimhub-v1/system','/mnt/probe/system')
        config+=f'\nPort {port}\nListenAddress 127.0.0.1\nPidFile /run/probe-sshd.pid\nUsePAM yes\n'
        staged={'system/'+n:d for n,d in files.items()}
        staged.update({'prepare':prepare.encode(),'sshd.conf':config.encode()})
        with dev.open_sftp() as s:
            for n,d in staged.items():
                with s.open(folder+'/'+n,'wx') as f:f.write(d)
                s.chmod(folder+'/'+n,0o600)
        cmd=['systemd-run','--quiet','--collect','--service-type=exec','--unit='+unit]
        for prop in ['RootDirectory='+a.root_directory,'MountAPIVFS=yes','PrivateDevices=yes','PrivateTmp=yes',
                     'ProtectSystem=strict','NoNewPrivileges=yes',
                     'CapabilityBoundingSet=CAP_CHOWN CAP_DAC_OVERRIDE CAP_SETUID CAP_SETGID CAP_SYS_CHROOT CAP_AUDIT_WRITE',
                     'AmbientCapabilities=','TemporaryFileSystem=/mnt:mode=0755 /run:mode=0755',
                     'BindPaths='+folder+':/mnt/probe','RestrictAddressFamilies=AF_UNIX AF_INET',
                     'RuntimeMaxSec=90','MemoryMax=64M','TasksMax=16']:
            cmd+=['-p',prop]
        body='set -eu;umask 077;mkdir -m 0755 /run/sshd;sh /mnt/probe/prepare;/usr/sbin/sshd -t -f /mnt/probe/sshd.conf;exec /usr/sbin/sshd -D -e -f /mnt/probe/sshd.conf'
        run(dev,shlex.join(cmd+['/bin/sh','-ec',body]))
        run(dev,f'''for n in 1 2 3 4 5;do
systemctl is-active --quiet {unit} || exit 1
if ss -H -ltn 'sport = :{port}' | grep -q '127.0.0.1:{port}';then exit 0;fi
sleep 1
done
exit 1''')
        private=Ed25519PrivateKey.generate().private_bytes(serialization.Encoding.PEM,serialization.PrivateFormat.OpenSSH,serialization.NoEncryption()).decode()
        wrong=paramiko.Ed25519Key.from_private_key(io.StringIO(private))
        def login(key,pinned,expect=None):
            sock=dev.get_transport().open_channel('direct-tcpip',('127.0.0.1',port),('127.0.0.1',0),timeout=5)
            probe=paramiko.SSHClient();probe.get_host_keys().add(f'[127.0.0.1]:{port}','ssh-ed25519',pinned)
            probe.set_missing_host_key_policy(paramiko.RejectPolicy())
            try:
                probe.connect('127.0.0.1',port=port,username='root',pkey=key,sock=sock,timeout=5,
                              banner_timeout=5,auth_timeout=5,look_for_keys=False,allow_agent=False)
                assert expect is None,'Unexpected authentication success'
                _,o,_=probe.exec_command('/usr/bin/id -u',timeout=5)
                assert o.read().strip()==b'0' and o.channel.recv_exit_status()==0
            except Exception as exc:
                if expect is None or not isinstance(exc,expect):raise
            finally:probe.close();sock.close()
        login(client_key,host)
        login(client_key,wrong,paramiko.BadHostKeyException)
        login(wrong,host,paramiko.AuthenticationException)
        report.update(real_client_login=True,wrong_host_pin_denied=True,wrong_client_key_denied=True)
        access=folder+'/system/ssh-access/authorized_keys'
        run(dev,': > '+shlex.quote(access))
        login(client_key,host,paramiko.AuthenticationException)
        with dev.open_sftp() as s:
            with s.open(access,'wb') as f:f.write(public)
        login(client_key,host)
        report.update(revocation_denied=True,same_key_restored=True)
        with dev.open_sftp() as s:
            for n,d in files.items():
                with s.open(folder+'/system/'+n,'rb') as f:assert f.read()==d,'Selected identity changed'
        report['selected_identity_unchanged']=True
        run(dev,'systemctl stop '+shlex.quote(unit))
        stopped=True
        report['after']=baseline()
        run(dev,f'! ss -H -ltn "sport = :{port}" | grep -q "127.0.0.1:{port}"')
        report.update(passed=True,cleanup_verified=True)
        print('Selected maintenance key passed pinned candidate loopback login and denials; client private key stayed on PC; server stopped')
    finally:
        if dev and unit and not stopped:
            try:run(dev,'systemctl stop '+shlex.quote(unit))
            except Exception:report.update(stop_check_failed=True,cleanup_verified=False)
        save()
        if dev:dev.close()
        if nas:nas.close()

if __name__=='__main__':main()
