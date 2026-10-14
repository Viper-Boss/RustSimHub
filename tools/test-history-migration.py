#!/usr/bin/env python3
"""Actual NAS/stick acceptance using the legacy full schema and synthetic history only."""
import argparse,hashlib,json,pathlib,re,shlex,sqlite3,subprocess,sys,tempfile
ROOT=pathlib.Path(__file__).resolve().parents[1];sys.path.insert(0,str(ROOT/'.local/python-deps'))
import paramiko
p=argparse.ArgumentParser(description=__doc__)
for n in ['nas','device','binary','archive','legacy-schema','report']:p.add_argument('--'+n,type=pathlib.Path,required=True)
p.add_argument('--revision',required=True);a=p.parse_args()
assert re.fullmatch('[a-f0-9]{7,12}',a.revision) and not a.report.exists()
nc=json.loads(a.nas.read_text(encoding='utf-8-sig'));dc=json.loads(a.device.read_text(encoding='utf-8-sig'));q=shlex.quote
source=hashlib.sha256(a.archive.read_bytes()).hexdigest();binary_hash=hashlib.sha256(a.binary.read_bytes()).hexdigest()
schema=a.legacy_schema.read_text(encoding='utf-8');report={'source_archive_sha256':source,'binary_sha256':binary_hash,'legacy_schema_sha256':hashlib.sha256(schema.encode()).hexdigest(),'synthetic_only':True,'new_calls':0,'new_sms':0,'cellular_tests':False};nas=dev=None
def save():a.report.parent.mkdir(parents=True,exist_ok=True);a.report.write_text(json.dumps(report,indent=2),encoding='utf-8')
def connect(c,sock=None):
 s=paramiko.SSHClient();s.load_host_keys(c.get('known_hosts',str(pathlib.Path.home()/'.ssh/known_hosts')));s.set_missing_host_key_policy(paramiko.RejectPolicy());s.connect(c['host'],port=c.get('port',22),username=c['username'],password=c.get('password'),key_filename=c.get('key_filename'),sock=sock,timeout=12,look_for_keys=False,allow_agent=False);return s
def run(client,script,sudo=False,expected=0):
 i,o,e=client.exec_command("sudo -S -p '' sh -s" if sudo else 'sh -s',timeout=90);o.channel.set_combine_stderr(True)
 if sudo:i.write(nc['password']+'\n')
 i.write('set -eu\numask 077\n'+script+'\n');i.channel.shutdown_write();raw=o.read().decode(errors='replace');rc=o.channel.recv_exit_status()
 if rc!=expected:report['failure_output']=raw;save();raise RuntimeError('Private history acceptance failed')
 return raw
def value(raw):return json.loads(next(line for line in reversed(raw.splitlines()) if line.startswith('{')))
try:
 with tempfile.TemporaryDirectory(prefix='history-fixture-',dir=ROOT/'.private') as local:
  local=pathlib.Path(local);snapshot=local/'snapshot.sqlite';db=sqlite3.connect(snapshot);db.execute('PRAGMA foreign_keys=ON');db.executescript(schema)
  db.executemany("INSERT INTO modemdeck_users(id,username,password_hash,role) VALUES(?,?,?,'member')",[(u,u,'SYNTHETIC_SECRET_NOT_IMPORTED') for u in ['legacy_user_a','legacy_user_b']])
  # Independent original schema, not the Rust importer's projection-based unit fixture.
  db.execute("INSERT INTO modemdeck_lines(line_id,line_label) VALUES('line_synthetic','合成线路')")
  inserts=(ROOT/'crates/compat/src/history/test-schema.sql').read_text(encoding='utf-8').split('INSERT INTO modemdeck_admin_credentials',1)[1]
  inserts='INSERT INTO modemdeck_admin_credentials'+inserts
  inserts=inserts.replace("INSERT INTO modemdeck_lines(line_id,line_label) VALUES('line_synthetic','合成线路');",'')
  # The original schema requires updated_at/assigned_at, unlike the nullable unit fixture.
  inserts='\n'.join(line.replace(',NULL)',",'2026-10-14T00:00:00Z')") if line.startswith('INSERT INTO modemdeck_user_') else line for line in inserts.splitlines())
  db.executescript(inserts);assert db.execute('PRAGMA foreign_key_check').fetchall()==[];db.commit();db.close()
  initial=hashlib.sha256(snapshot.read_bytes()).hexdigest();report['snapshot_sha256']=initial
  nas=connect(nc);raw=run(nas,'d=$(mktemp -d /mnt/docker-data/rustsimhub/p0/history-accept-XXXXXXXX);printf "BASE=%s\\n" "$d"');match=re.search(r'BASE=(/mnt/docker-data/rustsimhub/p0/history-accept-[A-Za-z0-9]{8})',raw);assert match;base=match[1];report['nas_directory']=base
  with nas.open_sftp() as sf:
   sf.put(str(a.binary),base+'/rustsimhub-migrate');sf.chmod(base+'/rustsimhub-migrate',0o700);sf.put(str(snapshot),base+'/snapshot.sqlite');sf.chmod(base+'/snapshot.sqlite',0o400)
  info=value(run(nas,q(base+'/rustsimhub-migrate')+' --build-info'));assert info['source_archive_sha256']==source and info['offline_history_only'] and info['hardware_operations'] is False
  run(nas,'install -d -m 0700 '+q(base+'/archive'))
  namespace='a'*64
  for action in ['validate','import','import','inspect']:
   args=[base+'/rustsimhub-migrate',action]+([base+'/archive'] if action=='inspect' else [base+'/snapshot.sqlite',namespace]+([base+'/archive'] if action=='import' else []))
   result=value(run(nas,shlex.join(args)));assert result['history_only'] and not result['credentials_imported'] and result['hardware_operations']==0 and result['rows_by_entity']['sms']==3
   if action=='import':
    assert (result['inserted'],result['unchanged']) in [(15,0),(0,15)]
  assert initial in run(nas,'sha256sum '+q(base+'/snapshot.sqlite'));report['nas_import_repeat_verified']=True
  sock=nas.get_transport().open_channel('direct-tcpip',(dc['host'],dc.get('port',22)),('127.0.0.1',0),timeout=12);dev=connect(dc,sock)
  path='/opt/rustsimhub/dev-history-p5-'+a.revision
  raw=run(dev,'test ! -e '+q(path)+';d=$(mktemp -d /var/tmp/rustsimhub-history-XXXXXXXX);printf "BASE=%s\\n" "$d"');match=re.search(r'BASE=(/var/tmp/rustsimhub-history-[A-Za-z0-9]{8})',raw);assert match;realm=match[1];report['device_directory']=realm
  with dev.open_sftp() as sf:
   sf.put(str(a.binary),realm+'/binary');sf.chmod(realm+'/binary',0o600);sf.put(str(snapshot),realm+'/snapshot.sqlite');sf.chmod(realm+'/snapshot.sqlite',0o400)
  run(dev,'\n'.join(['echo '+q(binary_hash+'  '+realm+'/binary')+' | sha256sum -c -','install -d -m 0755 '+q(path),'install -m 0555 '+q(realm+'/binary')+' '+q(path+'/rustsimhub-migrate'),'chmod 0711 '+q(realm),'install -d -o rustsimhub-core -g rustsimhub-core -m 0700 '+q(realm+'/input')+' '+q(realm+'/archive'),'install -o rustsimhub-core -g rustsimhub-core -m 0400 '+q(realm+'/snapshot.sqlite')+' '+q(realm+'/input/snapshot.sqlite')]))
  def device_run(action):
   args=[path+'/rustsimhub-migrate',action]+([realm+'/archive'] if action=='inspect' else [realm+'/input/snapshot.sqlite',namespace]+([realm+'/archive'] if action=='import' else []))
   cmd=['systemd-run','--quiet','--wait','--pipe','--collect','--unit=rustsimhub-history-accept-p5','-p','User=rustsimhub-core','-p','Group=rustsimhub-core','-p','CapabilityBoundingSet=','-p','AmbientCapabilities=','-p','NoNewPrivileges=yes','-p','ProtectSystem=strict','-p','ProtectHome=yes','-p','PrivateDevices=yes','-p','PrivateTmp=yes','-p','InaccessiblePaths=/run/dbus','-p','BindReadOnlyPaths='+realm+'/input','-p','BindPaths='+realm+'/archive','-p','ReadWritePaths='+realm+'/archive','-p','RuntimeMaxSec=30','-p','MemoryMax=64M','-p','TasksMax=4','-p','LimitCORE=0']+args
   return value(run(dev,shlex.join(cmd)))
  results=[]
  for action in ['validate','import','import','inspect']:
   r=device_run(action);assert r['rows_by_entity']['sms']==3 and r['history_only'] and not r['credentials_imported'] and r['hardware_operations']==0;results.append(r)
  assert (results[1]['inserted'],results[2]['inserted'],results[2]['unchanged'])==(15,0,15)
  assert initial in run(dev,'sha256sum '+q(realm+'/input/snapshot.sqlite'))
  # Download only the synthetic archive to independently verify preserved data with Python.
  with dev.open_sftp() as sf:sf.get(realm+'/archive/history.sqlite',str(local/'device-history.sqlite'))
  db=sqlite3.connect((local/'device-history.sqlite').as_uri()+'?mode=ro',uri=True);rows=db.execute('SELECT entity,legacy_key,payload FROM history_records').fetchall();assert len(rows)==15
  items={(entity,key):json.loads(payload) for entity,key,payload in rows};assert items[('sms','[43]')]['deleted_at'] is not None and items[('sms','[44]')]['state']=='submitted';assert items[('contacts','["contact_synthetic"]')]['owner_user_id']=='legacy_user_a';assert sum(entity=='modemdeck_user_message_thread_state' for entity,_,_ in rows)==2
  assert not db.execute("SELECT name FROM sqlite_schema WHERE name IN ('principals','sms_operations','call_operations','modemdeck_admin_credentials')").fetchall();db.close();assert b'MUST_NEVER_IMPORT_THIS_AUTH_SECRET' not in (local/'device-history.sqlite').read_bytes()
  report['device_preservation_repeat_no_replay_verified']=True
  raw=run(dev,"systemctl is-active nassimhub-agent ModemManager NetworkManager rmtfs\nn=0;for f in /proc/asound/card*/pcm*/sub*/status;do test -f \"$f\";test \"$(sed -n '1p' \"$f\")\" = closed;n=$((n+1));done;test \"$n\" -gt 0\nif systemctl is-active --quiet rustsimhub-history-accept-p5;then exit 1;fi");assert raw.splitlines()[:4]==['active']*4;report['old_services_active']=True;report['pcm_closed_and_test_units_stopped']=True
  report['passed']=True;save();print('Full legacy schema history imported on NAS and isolated non-root stick; repeat and preserved states verified, no hardware actions')
finally:
 save()
 if dev:dev.close()
 if nas:nas.close()
