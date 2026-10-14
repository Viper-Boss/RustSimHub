#!/usr/bin/env python3
"""Fresh offline browser-owner realm: system D-Bus/devices blocked, no original data."""
import argparse,ipaddress,json,pathlib,re,secrets,shlex,sys,uuid
ROOT=pathlib.Path(__file__).resolve().parents[1];sys.path.insert(0,str(ROOT/'.local/python-deps'))
import paramiko
p=argparse.ArgumentParser(description=__doc__)
for n in ['nas','device','artifacts','report','certificate']:p.add_argument('--'+n,type=pathlib.Path,required=True)
a=p.parse_args();nc=json.loads(a.nas.read_text(encoding='utf-8-sig'));dc=json.loads(a.device.read_text(encoding='utf-8-sig'));m=json.loads(a.artifacts.read_text(encoding='utf-8'));directory=m['device_directory'];q=shlex.quote
assert re.fullmatch(r'/opt/rustsimhub/dev-state-p1-[a-f0-9]{7,12}',directory)
units=['rustsimhub-browser-owner-core-p1','rustsimhub-browser-owner-edge-p1'];nas=dev=sf=None;started=False
report={'hardware_access_disabled':True,'original_data_read':False,'new_calls':0,'new_sms':0,'source_archive_sha256':m['source_archive_sha256']}
def save():a.report.parent.mkdir(parents=True,exist_ok=True);a.report.write_text(json.dumps(report,indent=2),encoding='utf-8')
def connect(c,sock=None):
 s=paramiko.SSHClient();s.load_host_keys(c.get('known_hosts',str(pathlib.Path.home()/'.ssh/known_hosts')));s.set_missing_host_key_policy(paramiko.RejectPolicy());s.connect(c['host'],port=c.get('port',22),username=c['username'],password=c.get('password'),key_filename=c.get('key_filename'),sock=sock,timeout=12,look_for_keys=False,allow_agent=False);return s
def run(text):
 i,o,e=dev.exec_command('sh -s',timeout=60);o.channel.set_combine_stderr(True);i.write('set -eu\numask 077\n'+text+'\n');i.channel.shutdown_write();raw=o.read().decode(errors='replace');rc=o.channel.recv_exit_status()
 if rc:report['failed_output']=raw;save();raise RuntimeError('Browser fixture failed; inspect private report')
 return raw
def write(path,data,uid,gid):
 with sf.open(path,'wx') as f:f.write(data)
 sf.chmod(path,0o600);sf.chown(path,uid,gid)
try:
 nas=connect(nc);ch=nas.get_transport().open_channel('direct-tcpip',(dc['host'],dc.get('port',22)),('127.0.0.1',0),timeout=12);dev=connect(dc,ch);sf=dev.open_sftp()
 for unit in units:run('if systemctl is-active --quiet '+unit+';then exit 10;fi')
 for name in ['rustsimhub-core','rustsimhub-edge']:run('echo '+q(m[name+'_sha256']+'  '+directory+'/'+name)+' | sha256sum -c -')
 raw=run('ip -4 -o address show dev wlan0');match=re.search(r'\binet ([0-9.]+)/',raw);assert match;address=match.group(1);assert ipaddress.ip_address(address).is_private
 raw=run('d=$(mktemp -d /var/tmp/rustsimhub-browser-owner-XXXXXXXX);chmod 0711 "$d";install -d -o rustsimhub-core -g rustsimhub-core -m 0700 "$d/state" "$d/identity" "$d/setup";install -d -o rustsimhub-core -g rustsimhub-ipc -m 2750 "$d/runtime";install -d -o rustsimhub-edge -g rustsimhub-edge -m 0700 "$d/edge";printf "BASE=%s\\n" "$d";id -u rustsimhub-core;id -g rustsimhub-core;id -u rustsimhub-edge;id -g rustsimhub-edge')
 match=re.search(r'BASE=(/var/tmp/rustsimhub-browser-owner-[A-Za-z0-9]{8})\n(\d+)\n(\d+)\n(\d+)\n(\d+)',raw);assert match;base,coreuid,coregid,edgeuid,edgegid=match.groups();report['device_directory']=base
 secret=secrets.token_bytes(32);write(base+'/setup/code',secret,int(coreuid),int(coregid));report['temporary_setup_code']=secret.hex();del secret
 core=directory+'/rustsimhub-core';edge=directory+'/rustsimhub-edge';socket=base+'/runtime/core.sock'
 run('runuser -u rustsimhub-edge -- '+edge+' --init-tls '+q(base+'/edge/tls')+' '+q(address))
 config={'listen':address+':8448','origin':'https://'+address+':8448','core_socket':socket,'core_uid':int(coreuid),'tls_directory':base+'/edge/tls'};write(base+'/edge/config.json',json.dumps(config).encode(),int(edgeuid),int(edgegid))
 run('systemd-run --unit '+units[0]+' --collect -p User=rustsimhub-core -p Group=rustsimhub-core -p SupplementaryGroups=rustsimhub-ipc -p CapabilityBoundingSet= -p AmbientCapabilities= -p NoNewPrivileges=yes -p ProtectSystem=strict -p ProtectHome=yes -p PrivateDevices=yes -p PrivateTmp=yes -p InaccessiblePaths=/run/dbus -p '+q('BindPaths='+base+'/state '+base+'/identity '+base+'/runtime')+' -p '+q('BindReadOnlyPaths='+base+'/setup')+' -p RuntimeMaxSec=390 -p TimeoutStopSec=6 -p MemoryMax=96M -p TasksMax=16 -p LimitCORE=0 '+core+' --serve-local-setup '+q(socket)+' '+edgeuid+' '+q(base+'/state')+' '+q(base+'/identity')+' '+q(base+'/setup/code'));started=True
 run('n=0;while ! test -S '+q(socket)+';do n=$((n+1));test "$n" -lt 15;sleep 1;done')
 payload=json.dumps({'protocol':2,'id':str(uuid.uuid4()),'command':{'op':'identity'}},separators=(',',':'));delimiter='RSH_BROWSER_OWNER_IDENTITY_END'
 raw=run('runuser -u rustsimhub-edge -- '+core+' --request-auth '+q(socket)+" <<'"+delimiter+"'\n"+payload+'\n'+delimiter);identity=json.loads(raw.splitlines()[-1])['result'];assert identity['owner_setup_available'] is True;report['device_pin']=bytes(identity['public_key']).hex()
 run('systemd-run --unit '+units[1]+' --collect -p User=rustsimhub-edge -p Group=rustsimhub-edge -p SupplementaryGroups=rustsimhub-ipc -p CapabilityBoundingSet= -p AmbientCapabilities= -p NoNewPrivileges=yes -p ProtectSystem=strict -p ProtectHome=yes -p PrivateDevices=yes -p PrivateTmp=yes -p InaccessiblePaths=/run/dbus -p '+q('BindReadOnlyPaths='+base+'/edge '+base+'/runtime')+' -p RuntimeMaxSec=370 -p TimeoutStopSec=5 -p MemoryMax=64M -p TasksMax=8 -p LimitCORE=0 '+edge+' --serve '+q(base+'/edge/config.json'))
 a.certificate.parent.mkdir(parents=True,exist_ok=True);sf.get(base+'/edge/tls/cert.pem',str(a.certificate))
 report.update({'wifi_address':address,'upstream_origin':config['origin'],'ready':True,'units':units});save();started=False
 print('New offline browser-owner fixture ready; private code/pin saved, no original data or hardware API access')
finally:
 if dev and started:
  for unit in units:
   try:run('systemctl stop '+unit)
   except Exception:pass
 save()
 for s in [sf,dev,nas]:
  if s:s.close()
