#!/usr/bin/env python3
# SPDX-License-Identifier: AGPL-3.0-only
import argparse
import json
from pathlib import Path
import firstboot_ap
p=argparse.ArgumentParser(description='Assemble unsigned generic firstboot AP material; no service or network changes')
for name in ['core','edge','ap','launcher','application','public','output']:p.add_argument('--'+name,type=Path,required=True)
p.add_argument('--qemu',type=Path);p.add_argument('--sysroot',type=Path)
a=p.parse_args();firstboot_ap.require(bool(a.qemu)==bool(a.sysroot),'qemu/sysroot pair required')
runner=[str(a.qemu.resolve()),'-L',str(a.sysroot.resolve())] if a.qemu else []
m=firstboot_ap.build(a.core,a.edge,a.ap,a.launcher,a.application,a.public,a.output,runner)
print(json.dumps({'format':1,'source_sha256':m['source_sha256'],'files':len(m['files']),'public_installable':False,'credentials_included':False}))
