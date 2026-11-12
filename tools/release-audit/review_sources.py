#!/usr/bin/env python3
# SPDX-License-Identifier: AGPL-3.0-only
"""Read pinned package commits and scope; never builds, imports or executes their code."""
import argparse
import hashlib
import json
from pathlib import Path
import subprocess
from safeio import Refused,Tree,need,parse,read_file,hex_id

PREFIXES={
 'console':('crates/edge/src/web/console.js','crates/edge/src/web/console.css',
            'crates/storage/src/inbox.rs','crates/storage/src/inbox_management.rs',
            'crates/storage/src/calls.rs','crates/storage/src/calls_tests.rs',
            'tools/test-web-console','docs/deliveries/'),
 'firstboot':('tools/firstboot-ap/','firmware/system/firstboot-ap/','tools/compose-system-tree.py',
              'tools/build-system-image.py','firmware/system/overlay/usr/local/libexec/rustsimhub-firstboot',
              'firmware/system/overlay/usr/local/libexec/rustsimhub-generic-install','docs/deliveries/'),
 'matrix':('tools/product-integration/matrix/','tools/product-integration/fixtures/','tools/product-integration/production_chain.py',
           'crates/update/src/product_admission/','crates/update/src/admission/','crates/update/src/runtime_observation',
           'crates/core/src/runtime_compatibility','crates/core/src/update_health','docs/deliveries/')}

def review(repository,index):
    with Tree(repository):pass
    need(type(index) is dict and set(index)==set(PREFIXES),'three_package_inventory')
    def git(*args):
        result=subprocess.run(['git','--no-replace-objects','-C',str(repository),*args],stdin=subprocess.DEVNULL,capture_output=True,timeout=20)
        need(result.returncode==0 and len(result.stdout)<=64*1024*1024,'package_git_refused')
        return result.stdout
    rows={}
    for name,row in index.items():
        need(type(row) is dict and set(row)=={'base','product','head','archive_sha256'},'package_source_fields')
        need(all(hex_id(row[k],40) for k in ('base','product','head')) and hex_id(row['archive_sha256'],64),'package_full_identifiers')
        git('merge-base','--is-ancestor',row['base'],row['product'])
        git('merge-base','--is-ancestor',row['product'],row['head'])
        archive=git('archive','--format=tar',row['product'])
        need(hashlib.sha256(archive).hexdigest()==row['archive_sha256'],'package_archive_digest')
        changed=git('diff','--name-only','-z',row['base'],row['product']).decode().strip('\0').split('\0')
        need(changed and all(any(p.startswith(prefix) if prefix.endswith('/') or prefix=='tools/test-web-console' else p==prefix for prefix in PREFIXES[name]) for p in changed),'package_source_scope')
        documents=git('diff','--name-only','-z',row['product'],row['head']).decode().strip('\0')
        need(not documents or all(p.startswith('docs/deliveries/') for p in documents.split('\0')),'package_final_head_not_docs_only')
        rows[name]={**row,'source_files_changed':len(changed),'final_head_docs_only':True,
                    'runtime_reexecuted':False,'combined_image_built':False}
    return {'format':1,'kind':'web5-immutable-package-source-review','packages':rows,
            'read_only':True,'authorization_verified':False,'release_ready':False,
            'installed':False,'health_verified':False,'may_confirm':False}

def main():
    p=argparse.ArgumentParser(description=__doc__);p.add_argument('--repository',type=Path,required=True);p.add_argument('--index',type=Path,required=True);args=p.parse_args()
    try:result=review(args.repository,parse(read_file(args.index,16384)))
    except (Refused,ValueError,OSError,subprocess.SubprocessError,TypeError,KeyError):
        print(json.dumps({'passed':False,'code':'source_review_refused','release_ready':False}));return 2
    print(json.dumps(result,sort_keys=True));return 0
if __name__=='__main__':raise SystemExit(main())
