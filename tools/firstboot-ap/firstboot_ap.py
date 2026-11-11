# SPDX-License-Identifier: AGPL-3.0-only
"""Separate generic firstboot v1 layer; consumes the original AP table/contract unchanged."""
from pathlib import Path
import hashlib
import json
import os
import re
import subprocess
import sys

ROOT=Path(__file__).resolve().parents[2]
sys.path.insert(0,str(ROOT/'tools'))
import setup_ap_package as ap
from application_package import checked, unique_object, verify_package
MATERIAL=ROOT/'firmware/system/firstboot-ap'
AUXILIARY=(('rustsimhub-firstboot-ap-activation.service','etc/systemd/system/rustsimhub-firstboot-ap-activation.service'),('10-firstboot-prepared.conf','etc/systemd/system/rustsimhub-setup-edge.service.d/10-firstboot-prepared.conf'),('10-firstboot-prepared.conf','etc/systemd/system/rustsimhub-setup-ap.service.d/10-firstboot-prepared.conf'))

def require(value,message):
    if not value:raise ValueError(message)
def sha(data):return hashlib.sha256(data).hexdigest()
def regular(path,maximum=32*1024*1024):
    p=Path(path);m=p.lstat();require(not p.is_symlink() and p.is_file() and m.st_nlink==1 and m.st_size<=maximum,'regular bounded single-link file required')
    return checked(p,maximum)
def load(path):return json.loads(regular(path,131072),object_pairs_hook=unique_object)

def build(core,edge,program,launcher,application,public,out,runner=()):
    out=Path(out);require(not out.exists() and not out.is_symlink(),'fresh output required')
    builds={};blobs={}
    for name,path in [('core',core),('edge',edge),('setup-ap',program),('core-launcher',launcher)]:
        raw=regular(path);ap.elf(raw)
        result=subprocess.run([*runner,str(Path(path).resolve(strict=True)),'--build-info'],stdin=subprocess.DEVNULL,capture_output=True,timeout=10)
        require(result.returncode==0 and not result.stderr and len(result.stdout)<=16384,'actual bounded program metadata required')
        info=json.loads(result.stdout,object_pairs_hook=unique_object);builds[name]={'sha256':sha(raw),'info':info}
        if name in ('setup-ap','core-launcher'):blobs['rustsimhub-'+name]=raw
    source=builds['core']['info']['source_archive_sha256'];ap.builds_valid(builds,source)
    app=verify_package(Path(application),Path(public),source,schema=6);require(app['format']==2,'format2 factory group required')
    for name in ('core','edge'):
        require(builds[name]['sha256']==sha(regular(Path(application)/('rustsimhub-'+name))),'factory differs from inspected program')
    for name,_,_ in ap.FILES:
        if name not in blobs:blobs[name]=(ap.MATERIAL/name).read_bytes().replace(b'\r\n',b'\n')
    require(not subprocess.check_output(['git','-c','safe.directory='+str(ROOT),'-C',str(ROOT),'status','--porcelain','--','tools/firstboot-ap','firmware/system/firstboot-ap','firmware/system/setup-ap','firmware/system/overlay/usr/local/libexec/rustsimhub-firstboot','firmware/system/overlay/usr/local/libexec/rustsimhub-generic-install','tools/compose-system-tree.py','tools/build-system-image.py']).strip(),'commit firstboot/AP composition inputs first')
    revision=subprocess.check_output(['git','-c','safe.directory='+str(ROOT),'-C',str(ROOT),'rev-parse','HEAD'],text=True).strip()
    manifest={'format':1,'kind':'generic-firstboot-ap','source_sha256':source,'factory_manifest_sha256':sha(regular(Path(application)/'manifest.json')),'builds':builds,
      'files':[{'name':name,'target':target,'mode':mode,'bytes':len(blobs[name]),'sha256':sha(blobs[name])} for name,target,mode in ap.FILES],
      'generic_contract_sha256':sha((MATERIAL/'contract.json').read_bytes()),'installer_revision':revision,'public_installable':False}
    out.mkdir(mode=0o755)
    for name,raw in {**blobs,'manifest.json':json.dumps(manifest,sort_keys=True,separators=(',',':')).encode()+b'\n','generic-contract.json':(MATERIAL/'contract.json').read_bytes()}.items():
        with (out/name).open('xb') as f:f.write(raw)
        (out/name).chmod(0o644)
    return manifest

def verify(bundle,application_source,factory_digest):
    bundle=Path(bundle);require(bundle.resolve(strict=True)==bundle and bundle.is_dir() and not bundle.is_symlink(),'canonical bundle required')
    require({p.name for p in bundle.iterdir()}=={name for name,_,_ in ap.FILES}|{'manifest.json','generic-contract.json'},'exact firstboot material set')
    m=load(bundle/'manifest.json');require(set(m)=={'format','kind','source_sha256','factory_manifest_sha256','builds','files','generic_contract_sha256','installer_revision','public_installable'},'firstboot fields')
    require(m['format']==1 and m['kind']=='generic-firstboot-ap' and m['public_installable'] is False,'firstboot v1 identity')
    require(re.fullmatch('[0-9a-f]{40}',m['installer_revision']) and m['source_sha256']==application_source and m['factory_manifest_sha256']==factory_digest,'source/factory mismatch')
    ap.builds_valid(m['builds'],application_source)
    require(regular(bundle/'generic-contract.json')==(MATERIAL/'contract.json').read_bytes() and m['generic_contract_sha256']==sha(regular(bundle/'generic-contract.json')),'unknown generic contract')
    require(len(m['files'])==8,'eight original AP targets')
    for r,(name,target,mode) in zip(m['files'],ap.FILES):
        raw=regular(bundle/name);require(set(r)=={'name','target','mode','bytes','sha256'} and (r['name'],r['target'],r['mode'],r['bytes'],r['sha256'])==(name,target,mode,len(raw),sha(raw)),'fixed AP payload mismatch')
        if name in ap.BINARIES:
            ap.elf(raw);require(sha(raw)==m['builds'][ap.BINARIES[name]]['sha256'],'binary metadata binding')
        else:require(raw==(ap.MATERIAL/name).read_bytes().replace(b'\r\n',b'\n'),'unreviewed AP template')
    return m

def install(tree,bundle,application_source,factory_digest):
    """Offline only. It installs no identity, credential or activation drop-in."""
    tree=Path(tree);m=verify(bundle,application_source,factory_digest)
    require(os.geteuid()==0,'numeric Root owners required')
    require(not (tree/'etc/rustsimhub/product-profile.json').exists(),'generic AP and product profile must be separately reviewed')
    def put(relative,raw,mode):
        p=tree/relative;parent=tree
        for part in Path(relative).parts[:-1]:
            parent=parent/part
            if not parent.exists():parent.mkdir(mode=0o755);parent.chmod(0o755);os.chown(parent,0,0)
            require(parent.is_dir() and not parent.is_symlink() and parent.stat().st_uid==0 and not parent.stat().st_mode&0o022,'unsafe layer parent')
        require(not p.exists() and not p.is_symlink(),'firstboot layer target already exists')
        with p.open('xb') as f:f.write(raw)
        p.chmod(mode);os.chown(p,0,0)
    for i,(name,target,mode) in enumerate(ap.FILES):
        put('usr/share/rustsimhub/firstboot-ap/'+name,regular(Path(bundle)/name),0o644)
        if i not in (3,4,5):put(target.lstrip('/'),regular(Path(bundle)/name),mode)
        else:
            parent=tree/Path(target.lstrip('/')).parent;parent.mkdir(parents=True,exist_ok=True);parent.chmod(0o755);os.chown(parent,0,0)
    for name in ['manifest.json','generic-contract.json']:put('usr/share/rustsimhub/firstboot-ap/'+name,regular(Path(bundle)/name),0o644)
    put('usr/local/libexec/rustsimhub-firstboot-ap',(MATERIAL/'runtime.py').read_bytes(),0o555)
    put('etc/systemd/system/rustsimhub-firstboot-ap.service',(MATERIAL/'rustsimhub-firstboot-ap.service').read_bytes(),0o644)
    for name,target in AUXILIARY:put(target,(MATERIAL/name).read_bytes(),0o644)
    # Mount destinations are empty public aliases; only firstboot creates private role directories.
    alias=tree/'var/lib/rustsimhub-setup-edge';alias.mkdir(mode=0o755);alias.chmod(0o755);os.chown(alias,0,0)
    return {'format':1,'source_sha256':application_source,'manifest_sha256':sha(regular(Path(bundle)/'manifest.json')),'installer_revision':m['installer_revision'],'runtime_sha256':sha((MATERIAL/'runtime.py').read_bytes()),'request_created_on_device':True,'identity_required':True,'credentials_included':False,'activation_dropins_preinstalled':False,'public_installable':False}

def audit_tree(tree,layer):
    tree=Path(tree);bundle=(tree/'usr/share/rustsimhub/firstboot-ap').resolve(strict=True)
    m=verify(bundle,layer['source_sha256'],sha(regular(tree/'usr/share/rustsimhub/factory-app/manifest.json')))
    require(layer['manifest_sha256']==sha(regular(bundle/'manifest.json')) and layer['credentials_included'] is False and layer['activation_dropins_preinstalled'] is False and layer['public_installable'] is False,'firstboot layer gates differ')
    for i,(name,target,mode) in enumerate(ap.FILES):
        p=tree/target.lstrip('/')
        if i in (3,4,5):require(not p.exists() and not p.is_symlink(),'activation must wait for device identity')
        else:
            require(regular(p)==regular(bundle/name),'installed firstboot material differs')
            info=p.lstat();require((info.st_uid,info.st_gid,info.st_mode&0o777)==(0,0,mode),'installed firstboot ownership/mode')
    for target,name,mode in [('usr/local/libexec/rustsimhub-firstboot-ap','runtime.py',0o555),('etc/systemd/system/rustsimhub-firstboot-ap.service','rustsimhub-firstboot-ap.service',0o644)]:
        p=tree/target;require(regular(p)==(MATERIAL/name).read_bytes(),'firstboot helper/unit differs');info=p.lstat();require((info.st_uid,info.st_gid,info.st_mode&0o777)==(0,0,mode),'firstboot helper/unit ownership')
    for name,target in AUXILIARY:
        p=tree/target;require(regular(p)==(MATERIAL/name).read_bytes(),'firstboot activation material differs');info=p.lstat();require((info.st_uid,info.st_gid,info.st_mode&0o777)==(0,0,0o644),'firstboot activation ownership')
    # The application revision chooses compose's overlay. Installing only this
    # layer beside an older overlay would never create the on-device request.
    # Refuse that real production wiring gap before an image can be accepted.
    for name in ['rustsimhub-firstboot','rustsimhub-generic-install']:
        target=Path('usr/local/libexec')/name;p=tree/target
        require(regular(p)==(ROOT/'firmware/system/overlay'/target).read_bytes(),'firstboot overlay hook differs')
        info=p.lstat();require((info.st_uid,info.st_gid,info.st_mode&0o777)==(0,0,0o755),'firstboot hook ownership/mode')
    require(not any((tree/'var/lib/rustsimhub').iterdir()),'device state must remain absent')
    return m
