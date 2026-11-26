#!/usr/bin/env python3
# SPDX-License-Identifier: AGPL-3.0-only
"""Verify that MSM8916 development packaging preserved the compiled LK program."""

import argparse
import hashlib
from pathlib import Path
import struct

ELF_HEADER = struct.Struct("<16sHHIIIIIHHHHHH")
PROGRAM_HEADER = struct.Struct("<8I")
MBN_V3_HEADER = struct.Struct("<10I")
SEGMENT_TYPE_MASK = 0x07000000
HASH_SEGMENT_TYPE = 0x02000000
LOAD_SEGMENT = 1
ARM_MACHINE = 40
LK_ENTRY = 0x8F600000
ABOOT_PARTITION_BYTES = 1 << 20


def read_elf(path):
    payload = path.read_bytes()
    header = ELF_HEADER.unpack_from(payload)
    magic, elf_type, machine, _, entry, offset = header[:6]
    entry_size, entry_count = header[9:11]
    if (magic[:7] != b"\x7fELF\x01\x01\x01" or elf_type != 2 or machine != ARM_MACHINE
            or entry != LK_ENTRY or entry_size != PROGRAM_HEADER.size or not entry_count):
        raise ValueError(f"unexpected UFI003 ELF header in {path}")
    segments = [
        PROGRAM_HEADER.unpack_from(payload, offset + index * entry_size)
        for index in range(entry_count)
    ]
    for _, start, _, _, length, _, _, _ in segments:
        if start + length > len(payload):
            raise ValueError(f"segment extends past the end of {path}")
    return payload, segments


def segment_bytes(payload, segment):
    start, length = segment[1], segment[4]
    return payload[start:start + length]


def load_segments(payload, segments):
    return [(seg[2], seg[3], seg[5], seg[6], segment_bytes(payload, seg))
            for seg in segments if seg[0] == LOAD_SEGMENT]


def verify(source, candidate):
    elf, elf_segments = read_elf(source)
    image, segments = read_elf(candidate)
    if len(image) > ABOOT_PARTITION_BYTES:
        raise ValueError("image exceeds the UFI003 aboot partition")
    expected_program = load_segments(elf, elf_segments)
    if not expected_program or expected_program != load_segments(image, segments):
        raise ValueError("packaging changed the loadable bootloader")
    hashes = [seg for seg in segments if seg[6] & SEGMENT_TYPE_MASK == HASH_SEGMENT_TYPE]
    if len(hashes) != 1:
        raise ValueError("expected exactly one Qualcomm hash segment")
    hash_segment = hashes[0]
    hash_payload = segment_bytes(image, hash_segment)
    header = MBN_V3_HEADER.unpack_from(hash_payload)
    hash_start = MBN_V3_HEADER.size
    hash_length = header[5]
    if (header[1] != 3 or hash_length != len(segments) * hashlib.sha256().digest_size
            or len(hash_payload) != hash_start + header[4]):
        raise ValueError("invalid MSM8916 MBN v3 hash table")
    expected_hashes = b"".join(
        bytes(32) if seg == hash_segment else hashlib.sha256(segment_bytes(image, seg)).digest()
        for seg in segments
    )
    if hash_payload[hash_start:hash_start + hash_length] != expected_hashes:
        raise ValueError("MBN segment hashes do not match the image")
    return hashlib.sha256(image).hexdigest()


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("source_elf", type=Path)
    parser.add_argument("candidate_mbn", type=Path)
    args = parser.parse_args()
    try:
        digest = verify(args.source_elf, args.candidate_mbn)
    except (OSError, ValueError, struct.error) as error:
        parser.exit(1, f"Verification failed: {error}\n")
    print(f"MSM8916 development image verified. SHA-256 {digest}")


if __name__ == "__main__":
    main()
