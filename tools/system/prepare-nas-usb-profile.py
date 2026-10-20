#!/usr/bin/env python3
# SPDX-License-Identifier: AGPL-3.0-only
"""Prepare an inactive NAS USB profile; never loads/activates it or changes networking."""
import argparse
import importlib.util
import json
import pathlib
import re
import uuid

spec=importlib.util.spec_from_file_location('maintenance_seed',pathlib.Path(__file__).with_name('prepare-maintenance-seed.py'))
seed=importlib.util.module_from_spec(spec);spec.loader.exec_module(seed)

def prepare(directory,expected_public_sha256,port,output):
    seed.require(seed.os.geteuid()==0,'Local Root operator required')
    seed.os.umask(0o077)
    seed.require(re.fullmatch(r'[0-9]+-[0-9]+(?:\.[0-9]+)*',port),'Recorded physical USB port required')
    manifest,_,_=seed.validate_seed(directory,expected_public_sha256)
    seed.private_dir(output.parent)
    seed.require(output.is_absolute() and output.resolve()==output,'Canonical new private output required')
    output.mkdir(mode=0o700)
    identity=str(uuid.uuid4())
    profile=f'''[connection]
id=RustSimHub USB maintenance
uuid={identity}
type=ethernet
autoconnect=false

[ethernet]
mac-address={manifest['usb_host_mac']}

[ipv4]
method=manual
address1=10.55.0.1/29
never-default=true
ignore-auto-dns=true
may-fail=false

[ipv6]
method=disabled
'''
    seed.write_file(output/'usb.nmconnection',profile.encode())
    plan={'format':1,'kind':'inactive-nas-usb-profile','profile_uuid':identity,'physical_usb_port':port,
          'usb_vendor':'1d6b','usb_product':'0104','expected_host_mac':manifest['usb_host_mac'],
          'expected_serial_file_sha256':manifest['usb_serial_sha256'],'host_address':'10.55.0.1/29',
          'device_address':'10.55.0.2','autoconnect':False,'never_default':True,
          'requires_exact_usb_serial_port_mac_before_activation':True,
          'network_profile_loaded':False,'network_changed':False,'device_installed':False}
    seed.write_file(output/'access-plan.json',json.dumps(plan,indent=2).encode())
    seed.sync_dir(output);seed.sync_dir(output.parent)
    return plan

def main():
    p=argparse.ArgumentParser(description=__doc__)
    p.add_argument('--seed-directory',type=pathlib.Path,required=True)
    p.add_argument('--expected-public-sha256',required=True)
    p.add_argument('--usb-port',required=True)
    p.add_argument('--output',type=pathlib.Path,required=True)
    a=p.parse_args()
    prepare(a.seed_directory,a.expected_public_sha256,a.usb_port,a.output)
    print('Inactive NAS USB profile prepared privately; no network manager, address or route changed')

if __name__=='__main__':main()
