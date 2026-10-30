#!/usr/bin/env python3
# SPDX-License-Identifier: AGPL-3.0-only
"""Compile the exact fixed upstream credentials declaration with std only.

This is a source-excerpt failure sample, not a TURN client or dependency install.
Only synthetic values are constructed. No config/debug/credential text is output.
"""
import argparse
import hashlib
import json
import os
from pathlib import Path
import stat
import subprocess
import tempfile

COMMIT='d8d127ab1e37aef14f95a9f6b529073c31ee04a5'
SOURCE_SHA='a74d0b21919ba67466c8204d6d640678a778de1341fe19fc1a01d33e270f7bcb'


def main():
    parser=argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--types-source',type=Path,required=True,help='fixed turn-types/src/lib.rs, checked before extraction/compilation')
    args=parser.parse_args()
    flags=os.O_RDONLY|getattr(os,'O_NOFOLLOW',0)|getattr(os,'O_NONBLOCK',0)
    with os.fdopen(os.open(args.types_source,flags),'rb') as source:
        if not stat.S_ISREG(os.fstat(source.fileno()).st_mode):raise SystemExit('fixed upstream source required')
        data=source.read(16385)
    if len(data)>16384 or hashlib.sha256(data).hexdigest()!=SOURCE_SHA:
        raise SystemExit('fixed upstream source required')
    text=data.decode('utf8')
    start=text.index('/// Credentials used for a TURN user.')
    end=text.index('impl From<TurnCredentials>',start)
    # Preserve the original copyright/license header and exact derive/struct.
    header=text[:text.index('#![')]
    declaration=text[start:end]
    main_source='''
fn main() {
    let value = TurnCredentials {
        username: "SYNTHETIC_USER".to_owned(),
        password: "SYNTHETIC_PASSWORD".to_owned(),
    };
    let debug = format!("{value:?}");
    assert!(debug.contains("SYNTHETIC_PASSWORD"));
    println!("DEBUG_CONTAINS_SYNTHETIC_PASSWORD");
}
'''
    with tempfile.TemporaryDirectory(prefix='turn-debug-excerpt-') as directory:
        path=Path(directory);rust=path/'excerpt.rs';binary=path/'excerpt'
        rust.write_text(header+declaration+main_source)
        subprocess.run(['rustc','--edition=2021','--crate-name','turn_debug_excerpt',str(rust),'-o',str(binary)],
                       capture_output=True,check=True,timeout=20)
        result=subprocess.run([str(binary)],capture_output=True,check=True,timeout=3,text=True)
        if result.stdout.strip()!='DEBUG_CONTAINS_SYNTHETIC_PASSWORD':raise SystemExit('excerpt assertion did not execute')
    print(json.dumps({'kind':'turn-credentials-debug-excerpt','source_commit':COMMIT,
        'source_sha256':SOURCE_SHA,'debug_contains_synthetic_password':True,
        'scope':'exact_upstream_type_excerpt_only','candidate_approved':False},sort_keys=True))


if __name__=='__main__':main()
