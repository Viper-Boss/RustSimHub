#!/usr/bin/env python3
"""Private live block capture, read-only on device. Not an offline-consistent restore proof."""
import argparse,datetime,hashlib,json,pathlib,re,shlex,stat,struct,sys,time,zlib
ROOT=pathlib.Path(__file__).resolve().parents[1];sys.path.insert(0,str(ROOT/'.local/python-deps'))
import paramiko
p=argparse.ArgumentParser(description=__doc__);p.add_argument('--nas',type=pathlib.Path,required=True);p.add_argument('--device',type=pathlib.Path,required=True);p.add_argument('--report',type=pathlib.Path,required=True);args=p.parse_args()
ncfg=json.loads(args.nas.read_text(encoding='utf-8-sig'));dcfg=json.loads(args.device.read_text(encoding='utf-8-sig'))
def connect(cfg,sock=None):
 c=paramiko.SSHClient();c.load_host_keys(cfg.get('known_hosts',str(pathlib.Path.home()/'.ssh/known_hosts')));c.set_missing_host_key_policy(paramiko.RejectPolicy());c.connect(cfg['host'],port=cfg.get('port',22),username=cfg['username'],password=cfg.get('password'),key_filename=cfg.get('key_filename'),sock=sock,timeout=12,look_for_keys=False,allow_agent=False);return c
def command(c,text,sudo=False):
 i,o,e=c.exec_command("sudo -S -p '' sh -s" if sudo else 'sh -s',timeout=60);o.channel.set_combine_stderr(True)
 if sudo:i.write(ncfg['password']+'\n')
 i.write('set -eu\numask 077\n'+text+'\n');i.channel.shutdown_write();data=o.read().decode(errors='replace');rc=o.channel.recv_exit_status()
 if rc:raise RuntimeError('Private capture preflight/readback failed')
 return data
q=shlex.quote;nas=dev=ns=ds=None;report={'format':1,'device_writes':False,'capture':'live_unverified','restore_proven':False,'started':datetime.datetime.now(datetime.timezone.utc).isoformat(),'files':[]}
def save():args.report.parent.mkdir(parents=True,exist_ok=True);args.report.write_text(json.dumps(report,indent=2),encoding='utf-8')
try:
 nas=connect(ncfg);ch=nas.get_transport().open_channel('direct-tcpip',(dcfg['host'],dcfg.get('port',22)),('127.0.0.1',0),timeout=12);dev=connect(dcfg,ch)
 sectors=command(dev,'cat /sys/class/block/mmcblk0/size').strip();assert sectors.isdigit();total=int(sectors)*512;assert 1024**3<=total<=16*1024**3
 free=command(nas,"df -B1 --output=avail /mnt/docker-data/rustsimhub/p0 | tail -n 1").splitlines()[-1].strip();assert int(free)>total*3
 created=command(nas,'d=$(mktemp -d /mnt/docker-data/rustsimhub/p0/backup-private-full-XXXXXXXX);chmod 0700 "$d";printf "CAPTURE_DIR=%s\\n" "$d"')
 m=re.search(r'CAPTURE_DIR=(/mnt/docker-data/rustsimhub/p0/backup-private-full-[A-Za-z0-9]{8})',created);assert m;folder=m.group(1);report.update({'nas_directory':folder,'disk_bytes':total});save()
 ns=nas.open_sftp();ds=dev.open_sftp()
 # Read-only source metadata and bounds; never use size=0 device metadata as EOF length.
 assert stat.S_ISBLK(ds.stat('/dev/mmcblk0').st_mode)
 def copy(name,length):
  assert re.fullmatch(r'mmcblk0(?:boot[01])?',name)
  target=folder+'/'+name+'.img';h=hashlib.sha256();copied=0;started=time.monotonic()
  with ds.open('/dev/'+name,'rb',bufsize=0) as src,ns.open(target+'.partial','wx',bufsize=0) as dst:
   ns.chmod(target+'.partial',0o600);dst.set_pipelined(True)
   while copied<length:
    batch=min(4*1024*1024,length-copied);chunks=[(offset,min(32768,copied+batch-offset)) for offset in range(copied,copied+batch,32768)]
    for expected,data in zip(chunks,src.readv(chunks,max_concurrent_prefetch_requests=8),strict=True):
     if len(data)!=expected[1]:raise RuntimeError('Short block read; partial capture retained')
     dst.write(data);h.update(data);copied+=len(data)
    if copied%(64*1024*1024)==0:report['copied_bytes']=copied;save();print('Private read-only capture MiB:',copied//1024//1024,flush=True)
  assert ns.stat(target+'.partial').st_size==length;ns.rename(target+'.partial',target);report['copied_bytes']=copied
  result={'source':'/dev/'+name,'path':target,'bytes':length,'sha256':h.hexdigest(),'seconds':round(time.monotonic()-started,2)};report['files'].append(result);save();return result
 disk=copy('mmcblk0',total)
 for name in ['mmcblk0boot0','mmcblk0boot1']:
  size=command(dev,'if test -r /sys/class/block/'+name+'/size;then cat /sys/class/block/'+name+'/size;fi').strip()
  if size:assert size.isdigit();n=int(size)*512;assert n<=16*1024*1024;copy(name,n)
 # Independent NAS readback, not a hash of the transfer buffer.
 script="python3 - <<'PY'\nimport hashlib,json,pathlib,os\np=pathlib.Path("+repr(folder)+")\nrows=[]\nfor f in sorted(p.glob('*.img')):\n h=hashlib.sha256()\n with f.open('rb') as s:\n  os.fsync(s.fileno())\n  for b in iter(lambda:s.read(4*1024*1024),b''):h.update(b)\n rows.append({'path':str(f),'sha256':h.hexdigest(),'bytes':f.stat().st_size})\nprint(json.dumps(rows))\nPY"
 readback=json.loads(command(nas,script).splitlines()[-1]);assert {r['path']:(r['sha256'],r['bytes']) for r in readback}=={r['path']:(r['sha256'],r['bytes']) for r in report['files']}
 report['nas_readback_verified']=True;report['finished']=datetime.datetime.now(datetime.timezone.utc).isoformat();save()
 print('Private full block readback verified; live filesystem consistency and recovery entry remain unproven')
finally:
 save()
 for c in [ds,ns,dev,nas]:
  if c:c.close()
