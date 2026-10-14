#!/usr/bin/env python3
"""Strict HTTPS first-owner proof in an offline, empty device test realm; no browser claims."""
import argparse,json,pathlib,re,shlex,ssl,sys,uuid
from http.client import HTTPSConnection
ROOT=pathlib.Path(__file__).resolve().parents[1];sys.path.insert(0,str(ROOT/'.local/python-deps'))
import paramiko
p=argparse.ArgumentParser(description=__doc__)
for n in ['nas','fixture','certificate','ctl','artifacts','report']:p.add_argument('--'+n,type=pathlib.Path,required=True)
a=p.parse_args();nc=json.loads(a.nas.read_text(encoding='utf-8-sig'));fixture=json.loads(a.fixture.read_text(encoding='utf-8'));m=json.loads(a.artifacts.read_text(encoding='utf-8'))
assert fixture['hardware_access_disabled'] and not fixture['original_data_read']
assert not a.report.exists(), 'Use a new report; reconcile prior attempts before repeating'
pin=fixture['device_pin'];code=fixture['temporary_setup_code'];assert re.fullmatch('[a-f0-9]{64}',pin) and re.fullmatch('[a-f0-9]{64}',code)
context=ssl.create_default_context(cafile=str(a.certificate));assert context.check_hostname and context.verify_mode==ssl.CERT_REQUIRED
q=shlex.quote;report={'browser_verified':False,'hardware_access_disabled':True,'original_data_read':False,'new_calls':0,'new_sms':0};nas=sf=None
def save():a.report.parent.mkdir(parents=True,exist_ok=True);a.report.write_text(json.dumps(report,indent=2),encoding='utf-8')
def run(text,sudo=False,check=True):
 i,o,e=nas.exec_command("sudo -S -p '' sh -s" if sudo else 'sh -s',timeout=60);o.channel.set_combine_stderr(True)
 if sudo:i.write(nc['password']+'\n')
 i.write('set -eu\numask 077\n'+text+'\n');i.channel.shutdown_write();raw=o.read().decode(errors='replace');rc=o.channel.recv_exit_status()
 if rc and check:report['failed_output']=raw;save();raise RuntimeError('Private signing command failed')
 return rc,raw
def http(body,expected_status=200):
 identity=str(uuid.uuid4());c=HTTPSConnection(fixture['wifi_address'],8448,context=context,timeout=20)
 try:
  raw=json.dumps({'protocol':2,'id':identity,'command':body},separators=(',',':'))
  c.request('POST','/api/v2',body=raw,headers={'Host':fixture['wifi_address']+':8448','Origin':fixture['upstream_origin'],'Content-Type':'application/json','X-RustSimHub-Request':'2'})
  r=c.getresponse();data=r.read(65537);assert r.status==expected_status and len(data)<=65536
  if expected_status!=200:return None
  value=json.loads(data);assert value['protocol']==2 and value['id']==identity;return value['result']
 finally:c.close()
try:
 import hashlib
 assert hashlib.sha256(a.ctl.read_bytes()).hexdigest()==m['simhubctl_sha256']
 nas=paramiko.SSHClient();nas.load_host_keys(nc.get('known_hosts',str(pathlib.Path.home()/'.ssh/known_hosts')));nas.set_missing_host_key_policy(paramiko.RejectPolicy());nas.connect(nc['host'],port=nc.get('port',22),username=nc['username'],password=nc.get('password'),key_filename=nc.get('key_filename'),timeout=12,look_for_keys=False,allow_agent=False);sf=nas.open_sftp()
 identity=http({'op':'identity'});assert bytes(identity['public_key']).hex()==pin and identity['owner_setup_available'] is True
 _,raw=run('d=$(mktemp -d /mnt/docker-data/rustsimhub/p0/local-owner-client-private-XXXXXXXX);printf "BASE=%s\\n" "$d";cat /mnt/docker-data/rustsimhub/p0/audio-builder-image.txt')
 match=re.search(r'BASE=(/mnt/docker-data/rustsimhub/p0/local-owner-client-private-[A-Za-z0-9]{8})\n(sha256:[a-f0-9]{64})',raw);assert match;client,image=match.groups();report['nas_key_directory']=client
 sf.put(str(a.ctl),client+'/simhubctl');sf.chmod(client+'/simhubctl',0o755)
 container='docker run --rm -i --network none --read-only --cap-drop ALL --security-opt no-new-privileges --user 1000:1000 --memory 96m --pids-limit 16 --mount '+q('type=bind,src='+client+',dst=/client')+' '+q(image)+' /client/simhubctl '
 _,raw=run(container+'keygen /client/key',True);public=json.loads(raw.splitlines()[-1])['public_key']
 def sign(c,purpose):
  delimiter='RSH_OFFLINE_SETUP_PROOF_END';_,raw=run(container+'sign /client/key '+q(pin)+' '+purpose+" <<'"+delimiter+"'\n"+json.dumps(c,separators=(',',':'))+'\n'+delimiter,True);return json.loads(raw.splitlines()[-1])
 assert http({'op':'setup_challenge','setup_code':'0'*64,'public_key':public}).get('error');report['wrong_code_denied']=True
 http({'op':'setup_challenge','setup_code':code,'public_key':public,'role':'owner'},400);report['injected_role_denied']=True
 c=http({'op':'setup_challenge','setup_code':code,'public_key':public});assert 'id' in c
 assert http({'op':'setup_finish','setup_code':code,'challenge_id':c['id'],'signature':[0]*64}).get('error');report['bad_signature_denied']=True
 assert http({'op':'setup_finish','setup_code':code,'challenge_id':c['id'],'signature':sign(c,'bootstrap')}).get('error');report['consumed_challenge_replay_denied']=True
 c=http({'op':'setup_challenge','setup_code':code,'public_key':public});signature=sign(c,'bootstrap');report['finish_attempted_once']=True;save()
 result=http({'op':'setup_finish','setup_code':code,'challenge_id':c['id'],'signature':signature});assert result.get('owner_initialized') is True
 assert http({'op':'setup_finish','setup_code':code,'challenge_id':c['id'],'signature':signature}).get('error')
 assert http({'op':'setup_challenge','setup_code':code,'public_key':public}).get('error')
 assert http({'op':'identity'})['owner_setup_available'] is False;report['window_closed_and_owner_replacement_denied']=True
 c=http({'op':'challenge','public_key':public});token=http({'op':'authenticate','challenge_id':c['id'],'signature':sign(c,'login')})['token']
 assert http({'op':'health','token':token})['ok'] is True
 assert http({'op':'inbox','token':token,'after':None,'limit':32})['messages']==[]
 assert http({'op':'network_status','token':token}).get('error')
 assert http({'op':'prepare_sms','token':token,'line_epoch':1,'idempotency_key':'offline-denial','recipient':'+12025550123','text':'synthetic'}).get('error')
 report.update({'owner_login_and_empty_test_data_verified':True,'network_hardware_and_sms_write_denied':True,'passed':True});save();print('Strict HTTPS local owner API passed in empty offline realm; browser remains unverified')
finally:
 save()
 if sf:sf.close()
 if nas:nas.close()
