#!/usr/bin/env python3
"""Bounded actual device status baseline; uses an already approved status-only health key."""
import argparse,hashlib,json,math,pathlib,re,shlex,sys
ROOT=pathlib.Path(__file__).resolve().parents[1];sys.path.insert(0,str(ROOT/'.local/python-deps'))
import paramiko
p=argparse.ArgumentParser(description=__doc__)
for n in ['device','group-report','artifacts','report']:p.add_argument('--'+n,type=pathlib.Path,required=True)
a=p.parse_args();assert not a.report.exists();c=json.loads(a.device.read_text(encoding='utf-8-sig'));g=json.loads(a.group_report.read_text(encoding='utf-8'));m=json.loads(a.artifacts.read_text(encoding='utf-8'))
assert g['passed'] and g['cleanup_verified'] and g['source_archive_sha256']==m['source_archive_sha256']
base=g['device_directory'];directory=m['device_directory'];assert re.fullmatch('/var/tmp/rustsimhub-service-group-[A-Za-z0-9]{8}',base) and re.fullmatch('/opt/rustsimhub/dev-state-p1-[a-f0-9]{7,12}',directory)
q=shlex.quote;report={'source_archive_sha256':m['source_archive_sha256'],'new_calls':0,'new_sms':0,'network_mutations':False,'status_only':True,'latency_scope':'complete CLI process and local signed-session IPC; realtime clock; excludes separate grep after sample','performance_comparison_complete':False};jump=dev=None;started=False
def save():a.report.parent.mkdir(parents=True,exist_ok=True);a.report.write_text(json.dumps(report,indent=2),encoding='utf-8')
def run(text,check=True):
 i,o,e=dev.exec_command('sh -s',timeout=120);o.channel.set_combine_stderr(True);i.write('set -eu\numask 077\n'+text+'\n');i.channel.shutdown_write();raw=o.read().decode(errors='replace');rc=o.channel.recv_exit_status()
 if rc and check:report['failed_output']=raw;save();raise RuntimeError('Private status baseline failed')
 return rc,raw
def connect(config,sock=None):
 s=paramiko.SSHClient();s.load_host_keys(config.get('known_hosts',str(pathlib.Path.home()/'.ssh/known_hosts')));s.set_missing_host_key_policy(paramiko.RejectPolicy());s.connect(config['host'],port=config.get('port',22),username=config['username'],password=config.get('password'),key_filename=config.get('key_filename'),sock=sock,timeout=12,look_for_keys=False,allow_agent=False);return s
try:
 if 'jump' in c:
  jump=connect(c['jump']);sock=jump.get_transport().open_channel('direct-tcpip',(c['host'],c.get('port',22)),('127.0.0.1',0),timeout=12);dev=connect(c,sock)
 else:dev=connect(c)
 for name in ['simhubctl','rustsimhub-update']:run('echo '+q(m[name+'_sha256']+'  '+directory+'/'+name)+' | sha256sum -c -')
 for unit in ['rustsimhub-service-group-core-p6','rustsimhub-service-group-edge-p6','rustsimhub-status-benchmark-p7']:run('if systemctl is-active --quiet '+unit+';then exit 10;fi')
 _,raw=run('d=$(mktemp -d /var/tmp/rustsimhub-benchmark-tool-XXXXXXXX);printf "%s\\n" "$d"');helper=raw.splitlines()[-1];assert re.fullmatch('/var/tmp/rustsimhub-benchmark-tool-[A-Za-z0-9]{8}',helper)
 with dev.open_sftp() as sf:
  for name in ['device-service-group.sh','device-status-benchmark.sh']:
   sf.put(str(ROOT/'tools'/name),helper+'/'+name);sf.chmod(helper+'/'+name,0o700)
 group='sh '+q(helper+'/device-service-group.sh')
 started=True;run(group+' start '+q(base)+' '+q(directory+'/rustsimhub-update'))
 memory_script="for unit in rustsimhub-service-group-core-p6 rustsimhub-service-group-edge-p6 nassimhub-agent;do pid=$(systemctl show -p MainPID --value \"$unit\");test \"$pid\" -gt 0;printf 'UNIT=%s\\n' \"$unit\";awk '/^(Uid|VmRSS|VmHWM|Threads|CapEff|CapPrm|CapAmb|CapBnd):/{print}' /proc/\"$pid\"/status;done"
 _,before=run(memory_script);_,raw=run('sh '+q(helper+'/device-status-benchmark.sh')+' '+q(base)+' '+q(directory+'/simhubctl'));_,after=run(memory_script)
 parts=raw.split('BEGIN_SAMPLES\n',1)[1].split('END_SAMPLES',1)[0].splitlines();assert len(parts)==200 and all(re.fullmatch('[0-9]+',s) for s in parts);samples=sorted(int(s)/1_000_000 for s in parts)
 report.update({'samples':200,'min_ms':samples[0],'median_ms':samples[99],'p95_ms':samples[math.ceil(.95*200)-1],'p99_ms':samples[math.ceil(.99*200)-1],'max_ms':samples[-1],'mean_ms':sum(samples)/200,'memory_before':before,'memory_after':after,'all_requests_successful':True})
 # Real process UID/capability observations are part of the baseline, not inferred from unit files.
 for unit,expected in [('rustsimhub-service-group-core-p6','986'),('rustsimhub-service-group-edge-p6','985')]:
  block=after.split('UNIT='+unit+'\n',1)[1].split('UNIT=',1)[0];assert re.search(r'Uid:\s+'+expected+r'\s+'+expected+r'\s+'+expected+r'\s+'+expected,block)
  for field in ['CapEff','CapPrm','CapAmb','CapBnd']:assert re.search(field+r':\s+0+\b',block)
 report['actual_identity_caps_verified']=True;report['passed']=True
finally:
 if dev and started:
  rc,_=run(group+' stop '+q(base)+' '+q(directory+'/rustsimhub-update'),False)
  rc2,old=run("systemctl is-active nassimhub-agent ModemManager NetworkManager rmtfs\nfor f in /proc/asound/card*/pcm*/sub*/status;do test \"$(sed -n '1p' \"$f\")\" = closed;done\nif systemctl is-active --quiet rustsimhub-status-benchmark-p7;then exit 10;fi",False)
  report['cleanup_verified']=rc==0 and rc2==0 and old.splitlines()[:4]==['active']*4
 save()
 if dev:dev.close()
 if jump:jump.close()
if report.get('passed') and report.get('cleanup_verified'):print('Actual bounded non-root status baseline passed; 200 successful requests; comparison and long-term stability remain pending')
