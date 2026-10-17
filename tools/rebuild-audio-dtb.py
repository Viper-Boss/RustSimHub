#!/usr/bin/env python3
# SPDX-License-Identifier: AGPL-3.0-only
"""Rebuild the pinned UFI003 audio DTB offline. Never reads/writes a device or boot image."""
import argparse
import hashlib
import json
import pathlib
import struct
import subprocess

ROOT = pathlib.Path(__file__).resolve().parents[1]
FILES = {'ufi003-audio.dtsi', *('include/dt-bindings/sound/'+name for name in
         ('qcom,q6afe.h', 'qcom,q6asm.h', 'qcom,q6voice.h', 'qcom,q6dsp-lpass-ports.h'))}


def sha(data):
    return hashlib.sha256(data).hexdigest()


def verify_sources(source):
    profile = json.loads((source / 'source.json').read_text())
    if profile.get('format') != 1 or set(profile.get('files', {})) != FILES:
        raise ValueError('Unexpected DTB source profile')
    for name, expected in profile['files'].items():
        path = source
        for part in pathlib.PurePosixPath(name).parts:
            path /= part
            if path.is_symlink():
                raise ValueError('Source symlink refused')
        data = path.read_bytes()
        if len(data) != expected['bytes'] or sha(data) != expected['sha256']:
            raise ValueError('DTB source digest differs: '+name)
        blob = hashlib.sha1(b'blob '+str(len(data)).encode()+b'\0'+data).hexdigest()
        if blob != expected['git_blob']:
            raise ValueError('DTB source Git blob differs: '+name)
    return profile


def rebuild(source, input_dtb, out, dtc='dtc', cpp='cpp'):
    if out.exists() or out.is_symlink():
        raise FileExistsError('Output must be a new directory; previous evidence is retained')
    profile = verify_sources(source)
    if input_dtb.is_symlink() or not input_dtb.is_file():
        raise ValueError('Input must be the regular upstream DTB')
    with input_dtb.open('rb') as file:
        data = file.read(128*1024+1)
    if len(data) > 128*1024 or sha(data) != profile['input_dtb_sha256']:
        raise ValueError('Input is not the pinned upstream UFI003 DTB')
    if out.parent.resolve(strict=True) != out.absolute().parent:
        raise ValueError('Output parent must be canonical')
    out.mkdir(mode=0o700)
    report = {'passed': False, 'input_dtb_sha256': sha(data),
              'source_revision': profile['revision'], 'device_access': False,
              'boot_image_written': False, 'flash_allowed': False,
              'kernel_source_gate_resolved': False}
    def run(*args):
        return subprocess.run(args, check=True, capture_output=True, text=True, timeout=30).stdout
    try:
        report['dtc_version'] = run(dtc, '--version').strip()
        report['cpp_version'] = run(cpp, '--version').splitlines()[0]
        run(dtc, '-q', '-I', 'dtb', '-O', 'dts', '-o', str(out/'base.dts'), str(input_dtb))
        run(cpp, '-nostdinc', '-I', str(source/'include'), '-undef', '-x', 'assembler-with-cpp',
            '-P', str(source/'ufi003-audio.dtsi'), '-o', str(out/'audio.dtsi'))
        (out/'merged.dts').write_bytes((out/'base.dts').read_bytes()+b'\n'+(out/'audio.dtsi').read_bytes())
        run(dtc, '-q', '-I', 'dts', '-O', 'dtb', '-o', str(out/'audio.dtb'), str(out/'merged.dts'))
        result = (out/'audio.dtb').read_bytes()
        if len(result) < 40 or result[:4] != b'\xd0\x0d\xfe\xed' or struct.unpack_from('>I', result, 4)[0] != len(result):
            raise ValueError('Compiler did not produce a complete DTB')
        report['output_dtb_sha256'] = sha(result)
        report['output_bytes'] = len(result)
        if sha(result) != profile['expected_dtb_sha256']:
            raise ValueError('Rebuilt DTB differs from the verified profile')
        report['passed'] = True
        return report
    finally:
        (out/'DTB-REBUILD.json').write_text(json.dumps(report, indent=2)+'\n')


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--input-dtb', type=pathlib.Path, required=True)
    parser.add_argument('--out', type=pathlib.Path, required=True)
    parser.add_argument('--dtc', default='dtc')
    parser.add_argument('--cpp', default='cpp')
    args = parser.parse_args()
    report = rebuild(ROOT/'firmware/kernel/audio', args.input_dtb.resolve() if not args.input_dtb.is_symlink() else args.input_dtb,
                     args.out.absolute(), args.dtc, args.cpp)
    print('Pinned audio DTB rebuilt byte-for-byte:', report['output_dtb_sha256'], '; no boot/device writes')


if __name__ == '__main__':
    main()
