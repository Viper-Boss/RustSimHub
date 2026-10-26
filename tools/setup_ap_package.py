# SPDX-License-Identifier: AGPL-3.0-only
"""Separate development-signed privileged AP bundle. Never installs or starts it.

Core/Edge remain in the existing application group; this bundle binds their
exact bytes and source without extending the ordinary application trust domain.
"""
import argparse
import hashlib
import json
import pathlib
import re
import subprocess
import sys
import tempfile
from application_package import checked, unique_object

ROOT = pathlib.Path(__file__).resolve().parents[1]
MATERIAL = ROOT/'firmware/system/setup-ap'
CONTRACT = json.loads((MATERIAL/'contract.json').read_text(encoding='utf8'))
DOMAIN = b'RustSimHub.SetupApBundle.v1\0'
FILES = (
    ('rustsimhub-setup-ap', '/usr/libexec/rustsimhub/rustsimhub-setup-ap', 0o755),
    ('rustsimhub-setup-ap.service', '/etc/systemd/system/rustsimhub-setup-ap.service', 0o644),
    ('rustsimhub-setup-edge.service', '/etc/systemd/system/rustsimhub-setup-edge.service', 0o644),
    ('rustsimhub-setup-ap.tmpfiles', '/etc/tmpfiles.d/rustsimhub-setup-ap.conf', 0o644),
    ('50-setup-ap-core.conf', '/etc/systemd/system/rustsimhub-core.service.d/50-setup-ap.conf', 0o644),
    ('50-setup-ap-target.conf', '/etc/systemd/system/rustsimhub-app.target.d/50-setup-ap.conf', 0o644),
    ('contract.json', '/etc/rustsimhub/setup-ap-contract.json', 0o644),
    ('rustsimhub-core-launcher', '/usr/libexec/rustsimhub/rustsimhub-core-launcher', 0o755),
)
BINARIES = {'rustsimhub-setup-ap': 'setup-ap', 'rustsimhub-core-launcher': 'core-launcher'}


def require(value, message):
    if not value:
        raise ValueError(message)


def sha(value):
    return hashlib.sha256(value).hexdigest()


def digest(value):
    return isinstance(value,str) and re.fullmatch('[a-f0-9]{64}',value) is not None


def contract_valid(value):
    return isinstance(value,dict) and set(value)==set(CONTRACT) and all(type(value[k]) is type(v) and value[k]==v for k,v in CONTRACT.items())


def elf(value):
    require(len(value)>=64 and value[:7]==b'\x7fELF\x02\x01\x01', 'ELF64 LE required')
    require(int.from_bytes(value[16:18],'little') in (2,3) and int.from_bytes(value[18:20],'little')==183
        and int.from_bytes(value[52:54],'little')==64, 'AArch64 executable required')


def builds_valid(builds, source):
    require(digest(source) and isinstance(builds,dict) and set(builds)=={'core','edge','setup-ap','core-launcher'}, 'exact component/source set')
    for component,record in builds.items():
        require(isinstance(record,dict) and set(record)=={'sha256','info'} and digest(record['sha256']), 'component bytes binding')
        info=record['info']
        require(isinstance(info,dict),'component metadata object')
        require(info.get('component')==component and info.get('target_arch')=='aarch64', 'component identity/architecture')
        require(info.get('source_archive_sha256')==source and info.get('development_fixtures') is False, 'production same-source components required')
        require(contract_valid(info.get('setup_ap_installation_contract')), 'joint installation contract mismatch')
    require(type(builds['core']['info'].get('database_schema')) is int and builds['core']['info']['database_schema']==6, 'Core schema contract')
    caps=builds['core']['info'].get('service_launch')
    require(isinstance(caps,dict) and set(caps)=={'format','modes'} and type(caps['format']) is int and caps['format']==1 and caps['modes']==['device','forwarding_managed','setup_ap'], 'Core finite launch capabilities')
    protocol=builds['core-launcher']['info'].get('launcher_protocol')
    require(type(protocol) is int and protocol==1, 'launcher protocol contract')


def _assemble(blobs, builds, seed, output):
    """Internal byte assembly; production CLI obtains builds from actual programs."""
    source=builds['core']['info']['source_archive_sha256'];builds_valid(builds,source)
    require(set(blobs)=={name for name,_,_ in FILES}, 'fixed privileged payload set')
    for name,_,_ in FILES:
        if name in BINARIES:
            elf(blobs[name])
            require(sha(blobs[name])==builds[BINARIES[name]]['sha256'], 'binary differs from inspected program')
        else:
            require(blobs[name]==(MATERIAL/name).read_bytes().replace(b'\r\n',b'\n'), 'unreviewed service/contract bytes')
    output=pathlib.Path(output)
    require(output.is_absolute() and not output.exists() and output.resolve().is_relative_to((ROOT/'.private').resolve()), 'fresh private bundle directory required')
    require(len(seed)==32, 'Ed25519 seed length')
    if sys.platform=='win32':sys.path.insert(0,str(ROOT/'.local/python-deps'))
    from cryptography.hazmat.primitives.asymmetric.ed25519 import Ed25519PrivateKey
    manifest={'format':2,'kind':'setup-ap-maintenance','board':'ufi003-v02','arch':'aarch64-linux',
        'development_maintenance':True,'source_sha256':source,'contract':CONTRACT,'builds':builds,
        'application_format':2,'schema':6,'starts_services':False,'changes_owner':False,'activates_data':False,
        'files':[{'name':name,'target':target,'mode':mode,'bytes':len(blobs[name]),'sha256':sha(blobs[name])} for name,target,mode in FILES]}
    raw=json.dumps(manifest,separators=(',',':'),sort_keys=True).encode()
    require(len(raw)<=16384 and sum(map(len,blobs.values()))<=32*1024**2, 'bundle bounds')
    signature=Ed25519PrivateKey.from_private_bytes(seed).sign(DOMAIN+raw)
    output.mkdir(mode=0o700)
    for name,data in {**blobs,'manifest.json':raw,'manifest.sig':signature}.items():
        with (output/name).open('xb') as f:f.write(data)
        (output/name).chmod(0o600)
    return manifest


def create_bundle(core,edge,ap,launcher,seed,output,runner=()):
    builds={};blobs={}
    for component,path in [('core',core),('edge',edge),('setup-ap',ap),('core-launcher',launcher)]:
        path=pathlib.Path(path);require(not path.is_symlink(),'component link refused');path=path.resolve(strict=True);data=checked(path,24*1024**2);elf(data)
        result=subprocess.run([*runner,str(path),'--build-info'],stdin=subprocess.DEVNULL,capture_output=True,timeout=10)
        require(result.returncode==0 and not result.stderr and len(result.stdout)<=16384, 'bounded build metadata unavailable')
        info=json.loads(result.stdout,object_pairs_hook=unique_object)
        builds[component]={'sha256':sha(data),'info':info}
        for name,kind in BINARIES.items():
            if component==kind:blobs[name]=data
    for name,_,_ in FILES:
        if name not in BINARIES:blobs[name]=(MATERIAL/name).read_bytes().replace(b'\r\n',b'\n')
    return _assemble(blobs,builds,checked(seed,32),output)


def verify_bundle(directory,public,expected_source):
    directory=pathlib.Path(directory)
    require(not directory.is_symlink() and {p.name for p in directory.iterdir()}=={'manifest.json','manifest.sig',*(name for name,_,_ in FILES)}, 'unexpected/missing bundle entries')
    raw=checked(directory/'manifest.json',16384);signature=checked(directory/'manifest.sig',64);key=checked(public,32)
    require(len(signature)==64 and len(key)==32, 'external trust key/signature length')
    with tempfile.TemporaryDirectory(prefix='rsh-ap-signature-') as temporary:
        temp=pathlib.Path(temporary);(temp/'public.der').write_bytes(bytes.fromhex('302a300506032b6570032100')+key);(temp/'signed').write_bytes(DOMAIN+raw);(temp/'signature').write_bytes(signature)
        result=subprocess.run(['openssl','pkeyutl','-verify','-pubin','-keyform','DER','-inkey',str(temp/'public.der'),'-rawin','-in',str(temp/'signed'),'-sigfile',str(temp/'signature')],stdin=subprocess.DEVNULL,capture_output=True,timeout=5)
        require(result.returncode==0,'signature not trusted by external key')
    m=json.loads(raw,object_pairs_hook=unique_object)
    require(set(m)=={'format','kind','board','arch','development_maintenance','source_sha256','contract','builds','application_format','schema','starts_services','changes_owner','activates_data','files'},'manifest field set')
    require(type(m['format']) is int and m['format']==2 and m['kind']=='setup-ap-maintenance' and m['board']=='ufi003-v02' and m['arch']=='aarch64-linux','bundle identity')
    require(m['development_maintenance'] is True and m['starts_services'] is False and m['changes_owner'] is False and m['activates_data'] is False,'maintenance behavior contract')
    require(contract_valid(m['contract']) and type(m['schema']) is int and m['schema']==6 and type(m['application_format']) is int and m['application_format']==2,'installation/schema contract')
    require(m['source_sha256']==expected_source,'unexpected source');builds_valid(m['builds'],expected_source)
    require(isinstance(m['files'],list) and len(m['files'])==len(FILES),'payload list bound')
    total=0
    for entry,(name,target,mode) in zip(m['files'],FILES):
        require(set(entry)=={'name','target','mode','bytes','sha256'} and entry['name']==name and entry['target']==target and type(entry['mode']) is int and entry['mode']==mode,'fixed payload destination/mode')
        data=checked(directory/name,24*1024**2 if name in BINARIES else 65536);total+=len(data)
        require(type(entry['bytes']) is int and 0<len(data)==entry['bytes'] and sha(data)==entry['sha256'],'payload hash/size mismatch')
        if name in BINARIES:elf(data);require(sha(data)==m['builds'][BINARIES[name]]['sha256'],'binary inspection binding')
        else:require(data==(MATERIAL/name).read_bytes().replace(b'\r\n',b'\n'),'unreviewed installed material')
    require(total<=32*1024**2,'bundle total bound')
    return m


def main():
    parser=argparse.ArgumentParser(description=__doc__);commands=parser.add_subparsers(dest='command',required=True)
    build=commands.add_parser('build')
    for name in ['core','edge','ap','launcher','seed','output']:build.add_argument('--'+name,type=pathlib.Path,required=True)
    build.add_argument('--qemu',type=pathlib.Path);build.add_argument('--sysroot',type=pathlib.Path)
    verify=commands.add_parser('verify')
    for name in ['directory','public']:verify.add_argument('--'+name,type=pathlib.Path,required=True)
    verify.add_argument('--source',required=True)
    args=parser.parse_args()
    if args.command=='build':
        require(bool(args.qemu)==bool(args.sysroot),'QEMU and sysroot must be supplied together')
        runner=[str(args.qemu.resolve(strict=True)),'-L',str(args.sysroot.resolve(strict=True))] if args.qemu else []
        m=create_bundle(args.core,args.edge,args.ap,args.launcher,args.seed,args.output,runner)
    else:m=verify_bundle(args.directory,args.public,args.source)
    print(json.dumps({'verified':True,'kind':m['kind'],'source_sha256':m['source_sha256'],'files':len(m['files']),'device_changed':False,'trust_installed':False}))


if __name__=='__main__':main()
