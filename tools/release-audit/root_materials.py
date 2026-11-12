# SPDX-License-Identifier: AGPL-3.0-only
"""Read and verify the existing finite ProductMaintenance v1 input; never install."""
import ast
import os
from pathlib import Path
import subprocess
import tempfile
from safeio import Tree,need,parse,read_file,sha,hex_id,same

SOURCE_FILES={'profile_config':('materials','product-profile.json'), 'core_dropin':('materials','50-product-profile.conf'),
 'launcher':('programs','rustsimhub-core-launcher'), 'nkn_root':('programs','nkn-root'),
 'nkn_sdk':('programs','nkn-sdk-engine'),'nkn_unit':('materials','rustsimhub-nkn-root.service'),
 'product_tmpfiles':('materials','product.tmpfiles'),'ble_host':('programs','rustsimhub-ble-recovery'),
 'ble_unit':('materials','rustsimhub-ble-recovery.service'),'ble_policy':('materials','ble-product.policy'),
 'health_dropin':('materials','55-product-health.conf'),'health_tmpfiles':('materials','health.tmpfiles'),
 'health_service':('materials','rustsimhub-update-health.service'),'health_timer':('materials','rustsimhub-update-health.timer')}

def verify(args,component,app,git_blob,flags):
    need(args.root_materials is not None and hex_id(args.root_manifest_sha256,64)
         and args.roles_policy is not None,'root_materials_external_inputs_required')
    policy=read_file(args.roles_policy,8192)
    declaration=parse(policy)
    need(type(declaration) is dict and set(declaration)=={'format','roles'} and
         type(declaration['format']) is int and declaration['format']==1 and
         type(declaration['roles']) is dict and set(declaration['roles'])=={'nkn','ble'},'root_roles_policy')
    approved=[]
    for role in ('nkn','ble'):
        row=declaration['roles'][role]
        need(type(row) is dict and set(row)=={'uid','gid'} and type(row['uid']) is int and
             type(row['gid']) is int and row['uid']==row['gid'] and 0<row['uid']<2**32-1 and
             row['uid'] not in (29,983,984,985,986,987,65534),'root_roles_policy')
        approved.append(row['uid'])
    need(approved[0]!=approved[1],'root_roles_policy')
    ids=['profile_config','core_dropin','launcher']
    if flags[0]:ids+=['nkn_root','nkn_sdk','nkn_unit','product_tmpfiles']
    if flags[2]:ids+=['ble_host','ble_unit','ble_policy']
    if flags[3]:ids+=['health_dropin','health_tmpfiles','health_service','health_timer']
    parsed=ast.parse(git_blob(args.repository,args.source,'tools/product-install/package.py'))
    definitions=[ast.literal_eval(n.value) for n in parsed.body if isinstance(n,ast.Assign)
                 and any(isinstance(t,ast.Name) and t.id=='SOURCE_FILES' for t in n.targets)]
    need(len(definitions)==1 and all(definitions[0].get(n)==SOURCE_FILES[n] for n in ids),'root_source_contract')
    key=read_file(args.release_public,32);need(len(key)==32,'release_public_length')
    need(not args.roles_policy.resolve().is_relative_to(args.root_materials.resolve()) and
         not args.roles_policy.resolve().is_relative_to(args.build_root.resolve()),'root_cannot_approve_roles')
    with Tree(args.root_materials) as tree, Tree(args.build_root) as build:
        need(set(os.listdir(tree.fd))=={'manifest.json','manifest.sig'}|{n+'.payload' for n in ids},'root_material_inventory')
        raw=tree.read('manifest.json',32768);need(sha(raw)==args.root_manifest_sha256,'root_external_manifest_digest')
        value=parse(raw);need(type(value) is dict and set(value)=={'format','kind','board','arch','bindings','roles','files'},'root_manifest_fields')
        need(type(value['format']) is int and value['format']==1 and value['kind']=='product-maintenance'
             and value['board']=='ufi003-v02' and value['arch']=='aarch64-linux','root_manifest_contract')
        expected={'profile':args.profile,'source_commit':args.source,'source_archive_sha256':component['archive_sha256'],
                  'application_manifest_sha256':app['manifest_sha256'], 'core_sha256':component['programs']['rustsimhub-core']['sha256'],
                  'approved_role_policy_sha256':sha(policy)}
        need(same(value['bindings'],expected),'root_application_source_binding')
        need(same(value['roles'],['core','edge','media','forwarder']+(['nkn','ble'] if flags[0] else [])), 'root_roles')
        need(type(value['files']) is list and len(value['files'])==len(ids),'root_target_inventory')
        total=0
        for name,row in zip(ids,value['files']):
            payload=tree.read(name+'.payload',32*1024*1024)
            need(same(row,{'id':name,'bytes':len(payload),'sha256':sha(payload)}),'root_payload_entry')
            folder,file=SOURCE_FILES[name]
            need(payload==build.read(folder+'/'+file,32*1024*1024),'root_build_cross_binding')
            total+=len(payload)
        need(total<=64*1024*1024,'root_payload_budget')
        signature=tree.read('manifest.sig',64);need(len(signature)==64,'root_signature_length')
        with tempfile.TemporaryDirectory(prefix='rsh-root-audit-') as directory:
            scratch=Path(directory)
            (scratch/'public.der').write_bytes(bytes.fromhex('302a300506032b6570032100')+key)
            (scratch/'message').write_bytes(b'RustSimHub.ProductMaintenance.v1\0'+raw)
            (scratch/'signature').write_bytes(signature)
            subprocess.run(['openssl','pkeyutl','-verify','-pubin','-keyform','DER','-inkey',str(scratch/'public.der'),'-rawin','-in',str(scratch/'message'),'-sigfile',str(scratch/'signature')],stdin=subprocess.DEVNULL,stdout=subprocess.DEVNULL,stderr=subprocess.DEVNULL,check=True,timeout=10)
    return {'signature_verified':True,'fixed_targets_checked':len(ids),'manifest_sha256':sha(raw),
            'installed_nss_verified':False,'authorization_verified':False,'applied':False}
