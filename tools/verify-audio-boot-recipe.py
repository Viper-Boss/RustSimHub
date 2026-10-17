#!/usr/bin/env python3
# SPDX-License-Identifier: GPL-2.0-only
"""Hash the fixed UFI003 boot reconstruction in memory. Writes only a JSON report."""
import argparse
import copy
import hashlib
import importlib.util
import json
import pathlib
import re
import sys
import tomllib

ROOT = pathlib.Path(__file__).resolve().parents[1]
sys.dont_write_bytecode = True
spec = importlib.util.spec_from_file_location('upstream_boot_format', ROOT/'tools/rootfs/kernel/audio/replace-appended-dtb.py')
boot = importlib.util.module_from_spec(spec)
spec.loader.exec_module(boot)


def checked_bytes(path, expected, limit):
    if path.is_symlink() or not path.is_file() or path.stat().st_size > limit:
        raise ValueError('Input must be a bounded regular upstream file')
    with path.open('rb') as handle:
        data = handle.read(limit+1)
    if len(data) > limit or hashlib.sha256(data).hexdigest() != expected:
        raise ValueError('Input differs from the fixed public source lock')
    return data


def root_label(fields):
    fields = copy.deepcopy(fields)
    # This board uses the filesystem LABEL, not the GPT PARTLABEL or a cloned UUID.
    main = fields[12].split(b'\0',1)[0]
    extra = fields[14].split(b'\0',1)[0]
    pattern = rb'(?<!\S)root=\S+'
    if len(re.findall(pattern, main)) != 1 or re.search(pattern, extra):
        raise ValueError('Expected one unambiguous root selector in the main command line')
    main = re.sub(pattern,b'root=LABEL=rootfs',main)
    if len(main) >= 512:
        raise ValueError('Root command line exceeds Android v0 field')
    fields[12] = main.ljust(512,b'\0')
    return fields


def verify(upstream_path, dtb_path, report_path):
    if report_path.exists() or report_path.is_symlink():
        raise FileExistsError('Previous evidence is retained; report must be new')
    lock = json.loads((ROOT/'firmware/kernel/upstream-source.lock.json').read_text())
    inputs = {row['name']:row for row in lock['assets']}
    audio = json.loads((ROOT/'firmware/kernel/audio/source.json').read_text())
    board = tomllib.loads((ROOT/'firmware/board.toml').read_text())
    upstream = checked_bytes(upstream_path, inputs['boot.img']['sha256'],32*1024*1024)
    dtb = checked_bytes(dtb_path, audio['expected_dtb_sha256'],128*1024)
    parsed = boot.parse(upstream)
    image, _ = boot.split_kernel(parsed['kernel'])
    parsed['fields'] = root_label(parsed['fields'])
    minimal = boot.build(parsed,image+dtb)
    size = 64*1024*1024
    if len(minimal) > size:
        raise ValueError('Reconstruction exceeds the recorded boot partition')
    digest = hashlib.sha256(minimal)
    remaining = size-len(minimal)
    while remaining:
        amount = min(remaining,1024*1024)
        digest.update(bytes(amount));remaining -= amount
    passed = digest.hexdigest() == board['immutable_content']['boot_sha256']
    report = {'passed':passed,'upstream_boot_sha256':inputs['boot.img']['sha256'],
              'audio_dtb_sha256':audio['expected_dtb_sha256'],'root_selector':'root=LABEL=rootfs',
              'minimal_bytes':len(minimal),'minimal_sha256':hashlib.sha256(minimal).hexdigest(),
              'zero_padded_bytes':size,'zero_padded_sha256':digest.hexdigest(),
              'matches_recorded_board_boot':passed,'boot_image_written':False,'device_access':False,
              'partition_writes':0,'independent_complete_kernel_rebuild':False,
              'board_boot_verified':False,'recovery_verified':False,'flash_allowed':False,'release_ready':False}
    with report_path.open('x',encoding='utf8') as handle:
        handle.write(json.dumps(report,indent=2)+'\n')
    if not passed:
        raise ValueError('Reconstructed boot differs; failed report retained')
    return report


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--upstream-boot', type=pathlib.Path, required=True)
    parser.add_argument('--audio-dtb', type=pathlib.Path, required=True)
    parser.add_argument('--report', type=pathlib.Path, required=True)
    args = parser.parse_args()
    verify(args.upstream_boot,args.audio_dtb,args.report)
    print('64 MiB boot reconstruction hash matches the board record; JSON report only, no boot image or device writes')


if __name__=='__main__':
    main()
