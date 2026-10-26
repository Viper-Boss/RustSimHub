# SPDX-License-Identifier: AGPL-3.0-only
"""Prepare a finite first-install delivery after external signature verification.

No SSH, services, account changes or trust installation. On-device apply is a
separate Root maintenance step, pinned by the delivery and helper digests.
"""
import argparse
import ipaddress
import json
from pathlib import Path
import re
import sys

ROOT=Path(__file__).resolve().parents[2]
sys.path.insert(0,str(ROOT/'tools'))
import setup_ap_package as package
from application_package import checked,unique_object

CORE='/var/lib/rustsimhub/rustsimhub-v1/core'
PERSISTENT='/var/lib/rustsimhub/rustsimhub-v1/system/setup-ap'
SETUP_EDGE='/var/lib/rustsimhub/rustsimhub-v1/setup-edge'
DIRECTORIES=(PERSISTENT,SETUP_EDGE,CORE+'/setup-ap-journal',PERSISTENT+'/journal','/run/rustsimhub-setup-ap-core','/run/rustsimhub-setup-ap-root')
JOINT_EXEC=next(line.split('=',1)[1] for line in (package.MATERIAL/'50-setup-ap-core.conf').read_text().splitlines() if line.startswith('ExecStart=') and line!='ExecStart=')
DEVICE_EXEC=next(line.split('=',1)[1] for line in (ROOT/'firmware/system/overlay/etc/systemd/system/rustsimhub-core.service').read_text().splitlines() if line.startswith('ExecStart='))
BASE_EXEC=DEVICE_EXEC.replace(' --serve-device ', ' --serve-device-forwarding-managed ',1)+' 983'
BOUND={
 'application-manifest.sha256':'manifest_sha256','application-ledger.sha256':'ledger_sha256',
 'core-unit.sha256':'core_unit_sha256','forwarding-unit.sha256':'forwarding_unit_sha256',
 'passwd.sha256':'passwd_sha256','group.sha256':'group_sha256','edge-config.sha256':'edge_config_sha256',
 'identity.sha256':'identity_sha256','setup-used.sha256':'setup_used_sha256',
}
require=package.require
sha=package.sha


def identities(passwd,group):
    def rows(value,length):
        require(isinstance(value,str) and len(value)<=262144,'identity inventory bound')
        result=[line.split(':') for line in value.splitlines() if line]
        require(all(len(row)==length for row in result) and len({row[0] for row in result})==len(result),'malformed/duplicate identity')
        require(all(row[2].isascii() and row[2].isdigit() for row in result),'identity number')
        if length==7:require(all(row[3].isascii() and row[3].isdigit() for row in result),'primary group number')
        require(all(str(int(row[2]))==row[2] and (length!=7 or str(int(row[3]))==row[3]) for row in result),'canonical identity numbers required')
        return result
    users=rows(passwd,7);groups=rows(group,4)
    for name,uid in [('core',986),('edge',985),('media',984),('forwarder',983)]:
        name='rustsimhub-'+name
        require([(u[0],u[2],u[3]) for u in users if u[0]==name or u[2]==str(uid)]==[(name,str(uid),str(uid))],'service UID/GID missing or aliased')
        require([(g[0],g[2]) for g in groups if g[0]==name or g[2]==str(uid)]==[(name,str(uid))],'service group missing or aliased')
        require(all(u[0]==name for u in users if u[3]==str(uid)),'foreign primary service group')
        require(all(g[2] in [str(uid),'987'] for g in groups if name in g[3].split(',')),'unexpected supplementary service group')
        require(all(not g[3] or set(g[3].split(','))<={name} for g in groups if g[2]==str(uid)),'foreign supplementary service member')
    ipc=[g for g in groups if g[0]=='rustsimhub-ipc' or g[2]=='987']
    require(len(ipc)==1 and ipc[0][0]=='rustsimhub-ipc' and ipc[0][2]=='987','IPC group identity')
    require(set(filter(None,ipc[0][3].split(',')))<={'rustsimhub-core','rustsimhub-edge','rustsimhub-media','rustsimhub-forwarder'},'foreign IPC group member')
    require(not any(u[3]=='987' for u in users),'IPC primary group refused')


def validate(inventory,manifest):
    i=inventory
    require(type(i.get('format')) is int and i['format']==1,'inventory format')
    for prefix in ['local','nss']:identities(i[prefix+'_passwd'],i[prefix+'_group'])
    require(i['passwd_sha256']==sha(i['local_passwd'].encode()) and i['group_sha256']==sha(i['local_group'].encode()),'local account bytes changed')
    app=i['application'];source=manifest['source_sha256']
    require(app['verified'] is True and type(app['epoch']) is int and 0<app['epoch']<2**64,'verified current application required')
    require(all(type(app[k]) is int for k in ['format','schema_min','schema_max','floor']) and app['format']==2 and app['schema_min']==6 and app['schema_max']==6,'full forwarding application/schema required')
    require(app['source_sha256']==source and app['core_sha256']==manifest['builds']['core']['sha256'] and app['edge_sha256']==manifest['builds']['edge']['sha256'],'selected Core/Edge do not match the signed AP source')
    require(type(app['confirmed'].get('epoch')) is int and app['confirmed']=={'epoch':app['epoch'],'hash':app['manifest_sha256']} and app['floor']==app['epoch'] and app['candidate'] is None and app['attempted'] is False,'unconfirmed/pending/recovered application')
    require(i['manifest_sha256']==app['manifest_sha256'],'manifest digest binding')
    require(i['core_exec_start']==BASE_EXEC,'existing managed forwarding command required')
    require(type(i['systemd_version']) is int and i['systemd_version']>=247,'systemd feature boundary')
    require(type(i['active_calls']) is int and i['active_calls']==0 and i['pcm_closed'] is True and i['ordinary_data_connected'] is False,'idle voice/PCM and ordinary data off required')
    require(i['backup_verified'] is True and i['recovery_used'] is False,'verified backup and unused recovery budget required')
    require(type(i['owner_count']) is int and i['owner_count']==1 and i['owner_verified'] is True,'existing Owner profile required; factory setup is separate')
    for key in [*BOUND.values(),'backup_sha256']:
        require(package.digest(i[key]),'invalid bound digest: '+key)
    require(i['setup_code_sha256']=='absent' or package.digest(i['setup_code_sha256']),'setup code snapshot')
    require(re.fullmatch('[a-f0-9]{8}(?:-[a-f0-9]{4}){3}-[a-f0-9]{12}',i['boot_id']) is not None,'boot binding')
    require(re.fullmatch('/org/freedesktop/ModemManager1/Modem/[0-9]{1,10}',i['modem_path']) is not None,'fixed modem path')
    edge=i['normal_edge'];require(isinstance(edge['additional_listen_ips'],list) and len(edge['additional_listen_ips'])<=3,'Edge bind list')
    address=edge['listen'];host,port=address.rsplit(':',1);host=host.strip('[]');require(port=='8443','management port')
    for raw in [host,*edge['additional_listen_ips']]:
        ip=ipaddress.ip_address(raw)
        if isinstance(ip,ipaddress.IPv6Address) and ip.ipv4_mapped:ip=ip.ipv4_mapped
        require(not ip.is_unspecified and str(ip) not in ['127.0.0.1','172.31.85.1'],'normal Edge conflicts with setup listener')
    expected={target for _,target,_ in package.FILES}|set(DIRECTORIES)
    require(set(i['destinations'])==expected and all(value=='absent' for value in i['destinations'].values()),'first-install destinations must all be absent')
    return i


def create_delivery(inventory,directory,public,source,backup,output):
    manifest=package.verify_bundle(directory,public,source);i=validate(inventory,manifest)
    backup=Path(backup);data=checked(backup,128*1024**2);require(sha(data)==i['backup_sha256'],'coherent backup readback differs')
    output=Path(output)
    require(output.is_absolute() and not output.exists() and output.resolve().is_relative_to((ROOT/'.private').resolve()),'fresh private delivery required')
    blobs={f'payload-{index}':checked(Path(directory)/name,24*1024**2) for index,(name,_,_) in enumerate(package.FILES)}
    for name,key in BOUND.items():blobs[name]=(i[key]+'\n').encode()
    for name,value in [('core.sha256',manifest['builds']['core']['sha256']),('edge.sha256',manifest['builds']['edge']['sha256']),('setup-code.sha256',i['setup_code_sha256']),('boot-id',i['boot_id']),('modem-path',i['modem_path'])]:blobs[name]=(value+'\n').encode()
    blobs.update({'business-before.tar':data,'signed-manifest.json':checked(Path(directory)/'manifest.json',16384),'signed-manifest.sig':checked(Path(directory)/'manifest.sig',64),'signer.public':checked(public,32),'apply.sh':(package.MATERIAL/'install.sh').read_bytes().replace(b'\r\n',b'\n')})
    plan={'format':1,'kind':'first-setup-ap-install','source_sha256':source,'existing_owner_preserved':True,'signature_verified_off_device':True,
        'signed_manifest_sha256':sha(blobs['signed-manifest.json']),'helper_sha256':sha(blobs['apply.sh']),
        'files':[{'payload':f'payload-{n}','target':target,'mode':mode,'sha256':sha(blobs[f'payload-{n}'])} for n,(_,target,mode) in enumerate(package.FILES)],
        'starts_services':False,'changes_network':False,'restores_business_database':False,'retained_on_rollback':['prepared private materials','binding metadata','journal directories'],
        'invocation_requirement':'Root over pinned maintenance transport must check the helper digest before execution and supply the expected SHA256SUMS digest'}
    blobs['plan.json']=(json.dumps(plan,sort_keys=True,indent=2)+'\n').encode()
    require(len(blobs)==28,'fixed delivery file count')
    output.mkdir(mode=0o700)
    for name,value in blobs.items():
        with (output/name).open('xb') as file:file.write(value)
        (output/name).chmod(0o600)
    sums=''.join(sha(value)+'  '+name+'\n' for name,value in sorted(blobs.items())).encode()
    (output/'SHA256SUMS').write_bytes(sums);(output/'SHA256SUMS').chmod(0o600)
    return {'prepared':True,'delivery_sha256':sha(sums),'helper_sha256':sha(blobs['apply.sh']),'plan':plan,'device_changed':False}


def main():
    p=argparse.ArgumentParser(description=__doc__)
    for name in ['inventory','package','public','backup','output']:p.add_argument('--'+name,required=True,type=Path)
    p.add_argument('--source',required=True);a=p.parse_args()
    result=create_delivery(json.loads(checked(a.inventory,1024**2),object_pairs_hook=unique_object),a.package,a.public,a.source,a.backup,a.output)
    print(json.dumps({k:v for k,v in result.items() if k!='plan'}))


if __name__=='__main__':main()
