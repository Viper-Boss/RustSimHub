#!/usr/bin/env python3
# SPDX-License-Identifier: AGPL-3.0-only
"""Offline verification only; the expected source/key are external inputs, never capabilities."""
import argparse
import json
from pathlib import Path
import firstboot_ap
p=argparse.ArgumentParser(description=__doc__)
for name in ['material','application','public']:p.add_argument('--'+name,type=Path,required=True)
p.add_argument('--source',required=True)
a=p.parse_args()
app=firstboot_ap.verify_package(a.application,a.public,a.source,schema=6)
firstboot_ap.require(app['format']==2,'format2 application required')
m=firstboot_ap.verify(a.material.resolve(strict=True),a.source,firstboot_ap.sha(firstboot_ap.regular(a.application/'manifest.json')))
for name in ['core','edge']:
    firstboot_ap.require(m['builds'][name]['sha256']==firstboot_ap.sha(firstboot_ap.regular(a.application/('rustsimhub-'+name))),'factory/program mismatch')
print(json.dumps({'verified':True,'source_sha256':a.source,'credentials_included':False,'public_installable':False},sort_keys=True))
