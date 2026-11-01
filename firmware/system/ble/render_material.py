#!/usr/bin/env python3
# SPDX-License-Identifier: AGPL-3.0-only
"""Render a private review archive. Never install/reserve IDs/reload/start units."""
import argparse
import hashlib
import io
import json
import os
from pathlib import Path
import tarfile
import material_plan

POLICY = {'format': 1, 'core_uid': 986, 'ble_uid': 982, 'ble_gid': 982}

def render(output):
    report = material_plan.inspect()  # Revalidate every fixed permission/material.
    content = {name: material_plan.read_regular(material_plan.MATERIAL/name)
               for name in material_plan.FILES}
    content['bridge-policy.proposal.json'] = (json.dumps(POLICY, sort_keys=True)+'\n').encode()
    manifest = {'format': 1, 'kind': 'ble-offline-review', 'install_permitted': False,
                'default_enabled': False, 'production_ready': False, 'ids_reserved': False,
                'requires': ['mainline NSS allocation', 'actual A/Core provider and listener',
                             'target systemd/cgroup and BlueZ permission verification',
                             'reviewed C assembly and installation'],
                'sha256': {name: hashlib.sha256(data).hexdigest() for name,data in content.items()},
                'material_plan': report}
    content['review-manifest.json'] = (json.dumps(manifest, sort_keys=True, indent=2)+'\n').encode()
    # Exclusive ordinary output only; no paths from JSON and no extraction.
    fd = os.open(output, os.O_WRONLY|os.O_CREAT|os.O_EXCL|os.O_NOFOLLOW, 0o600)
    with os.fdopen(fd,'wb') as destination:
        with tarfile.open(fileobj=destination,mode='w') as archive:
            for name,data in sorted(content.items()):
                item = tarfile.TarInfo(name)
                item.size, item.mode, item.mtime = len(data), 0o600, 0
                archive.addfile(item,io.BytesIO(data))
        destination.flush()
        os.fsync(destination.fileno())
    return manifest

def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--output', type=Path, required=True)
    args = parser.parse_args()
    try:
        render(args.output)
    except (OSError, ValueError) as error:
        parser.exit(1, 'BLE review material refused: '+str(error)+'\n')
    print('BLE review archive written; installation not permitted')

if __name__ == '__main__':
    main()
