#!/usr/bin/env python3
# SPDX-License-Identifier: AGPL-3.0-only
"""Observe locally built ARM programs and export the existing build.json contract.

Linux workstation tooling only: no signing, services, hardware, or release
admission. Inputs must be the operator's own build outputs, not uploaded code.
"""
import argparse
import hashlib
import io
import json
import os
import platform
from pathlib import Path, PurePosixPath
import re
import selectors
import signal
import subprocess
import tarfile
import tempfile
import time

PROGRAMS = (
    'rustsimhub-core', 'simhubctl', 'rustsimhub-edge', 'rustsimhub-update',
    'rustsimhub-recovery', 'rustsimhub-migrate', 'rustsimhub-status-led',
    'rustsimhub-network-agent', 'rustsimhub-forwarder', 'rustsimhub-setup-ap',
    'rustsimhub-core-launcher', 'rustsimhub-mdns',
)
COMPONENTS = ('core', 'edge', 'update', 'setup-ap', 'core-launcher')
LIMIT = 24 * 1024**2


def require(ok, message):
    if not ok:
        raise ValueError(message)


def sha(data):
    return hashlib.sha256(data).hexdigest()


def regular(path, limit=LIMIT, *, single_link=True):
    import stat
    with os.fdopen(os.open(path, os.O_RDONLY | os.O_NOFOLLOW | os.O_NONBLOCK), 'rb') as stream:
        meta = os.fstat(stream.fileno())
        require(stat.S_ISREG(meta.st_mode) and meta.st_nlink > 0
                and (not single_link or meta.st_nlink == 1) and 0 < meta.st_size <= limit,
                'bounded regular file with permitted link count required')
        data = stream.read(limit + 1)
        require(len(data) == meta.st_size, 'input changed while reading')
        return data


def unique(pairs):
    result = {}
    for key, value in pairs:
        require(key not in result, 'duplicate metadata field')
        result[key] = value
    return result


def bounded_run(arguments, timeout=15, limit=16384, *, stdin_data=None, result_codes=(0,), with_status=False):
    require(result_codes and all(type(v) is int and v in (0, 3, 4) for v in result_codes), 'fixed comparison exit codes')
    if stdin_data is None:
        return _bounded_run(arguments, timeout, limit, subprocess.DEVNULL, result_codes, with_status)
    require(isinstance(stdin_data, bytes) and len(stdin_data) <= 128 * 1024, 'bounded stdin required')
    with tempfile.TemporaryFile() as source:
        source.write(stdin_data)
        source.seek(0)
        return _bounded_run(arguments, timeout, limit, source, result_codes, with_status)


def _bounded_run(arguments, timeout, limit, stdin, result_codes, with_status):
    """Bound both pipes and the process group; never invoke a shell.

    waitid(WNOWAIT) pins the leader PID until group cleanup, preventing a
    successful early exit from turning cleanup into a reused-PID signal.
    """
    process = subprocess.Popen(arguments, stdin=stdin, stdout=subprocess.PIPE,
                               stderr=subprocess.PIPE, start_new_session=True)
    streams = [process.stdout, process.stderr]
    values = [bytearray(), bytearray()]
    deadline = time.monotonic() + timeout
    reaped = False
    try:
        with selectors.DefaultSelector() as selector:
            for index, stream in enumerate(streams):
                os.set_blocking(stream.fileno(), False)
                selector.register(stream, selectors.EVENT_READ, index)
            while selector.get_map():
                remaining = deadline - time.monotonic()
                require(remaining > 0, 'inspection deadline exceeded')
                for key, _ in selector.select(min(remaining, .1)):
                    chunk = os.read(key.fileobj.fileno(), 4096)
                    if not chunk:
                        selector.unregister(key.fileobj)
                    else:
                        values[key.data].extend(chunk)
                        require(sum(map(len, values)) <= limit, 'inspection output exceeded limit')
        while os.waitid(os.P_PID, process.pid, os.WEXITED | os.WNOHANG | os.WNOWAIT) is None:
            require(time.monotonic() < deadline, 'inspection deadline exceeded')
            time.sleep(.01)
        try:
            os.killpg(process.pid, signal.SIGKILL)
        except ProcessLookupError:
            pass
        code = process.wait(timeout=2)
        reaped = True
        require(code in result_codes and not values[1], 'inspection command failed (exit ' + str(code) + ')')
        return (code, bytes(values[0])) if with_status else bytes(values[0])
    finally:
        if not reaped:
            try:
                os.killpg(process.pid, signal.SIGKILL)
            except ProcessLookupError:
                pass
            process.wait(timeout=2)
        for stream in streams:
            stream.close()


def verify_source(archive, repository, revision, expected, extract_to=None):
    require(re.fullmatch('[0-9a-f]{40}', revision) and re.fullmatch('[0-9a-f]{64}', expected),
            'fixed revision and source digest required')
    repository = Path(repository).resolve(strict=True)
    raw = regular(archive, 64 * 1024**2)
    require(sha(raw) == expected, 'archive digest mismatch')
    tree = bounded_run(['git', '-c', 'safe.directory=' + str(repository), '-C', str(repository),
                        'ls-tree', '-rz', revision], limit=2 * 1024**2)
    expected_files = {}
    for row in tree.split(b'\0'):
        if not row:
            continue
        meta, name = row.split(b'\t', 1)
        mode, kind, oid = meta.decode('ascii').split()
        require(mode in ('100644', '100755') and kind == 'blob', 'unsupported source entry')
        expected_files[name.decode('utf8')] = oid
    require(expected_files, 'empty committed source')
    expected_dirs = {str(parent) for name in expected_files for parent in PurePosixPath(name).parents if str(parent) != '.'}
    seen_dirs = set()
    seen = set()
    total = 0
    with tarfile.open(fileobj=io.BytesIO(raw), mode='r:*') as tar:
        for index, member in enumerate(tar):
            require(index < 20000, 'source archive entry limit')
            path = PurePosixPath(member.name)
            require(not path.is_absolute() and '..' not in path.parts and '\\' not in member.name,
                    'unsafe source path')
            if member.isdir():
                require(str(path) in expected_dirs and str(path) not in seen_dirs,
                        'uncommitted source directory')
                seen_dirs.add(str(path))
                continue
            require(member.isfile() and member.name in expected_files and member.name not in seen,
                    'unexpected, duplicate, or nonregular source')
            total += member.size
            require(member.size <= 8 * 1024**2 and total <= 128 * 1024**2, 'source archive size limit')
            data = tar.extractfile(member).read()
            oid = expected_files[member.name]
            algorithm = hashlib.sha1 if len(oid) == 40 else hashlib.sha256
            actual = algorithm(b'blob ' + str(len(data)).encode() + b'\0' + data).hexdigest()
            require(actual == oid, 'source differs from committed blob')
            seen.add(member.name)
    require(seen == set(expected_files), 'source archive omits committed files')
    if extract_to is not None:
        destination = Path(extract_to)
        require(destination.is_absolute() and not destination.exists() and not destination.is_symlink(),
                'fresh absolute source directory required')
        destination.mkdir(mode=0o700)
        # Extract the same already-validated bytes, never reopen a mutable input.
        with tarfile.open(fileobj=io.BytesIO(raw), mode='r:*') as tar:
            tar.extractall(destination, filter='data')
    return len(seen)


def elf(data):
    require(len(data) >= 64 and data[:7] == b'\x7fELF\x02\x01\x01'
            and int.from_bytes(data[16:18], 'little') in (2, 3)
            and int.from_bytes(data[18:20], 'little') == 183
            and int.from_bytes(data[52:54], 'little') == 64, 'ARM64 executable required')


def metadata(path, runner):
    raw = bounded_run([*runner, str(path), '--build-info'])
    def invalid_constant(_):
        raise ValueError('nonfinite metadata number')
    return json.loads(raw.decode('utf8'), object_pairs_hook=unique, parse_constant=invalid_constant)


def versions(path):
    raw = bounded_run(['readelf', '--version-info', str(path)], limit=512 * 1024)
    values = set(re.findall(r'GLIBC_([0-9]+\.[0-9]+)', raw.decode('utf8')))
    require(all(tuple(map(int, v.split('.'))) <= (2, 41) for v in values), 'device glibc boundary exceeded')
    return sorted(values, key=lambda value: tuple(map(int, value.split('.'))))


def contracts(infos, source):
    require(set(infos) == set(COMPONENTS), 'fixed build-info component set required')
    for name, info in infos.items():
        require(isinstance(info, dict) and info.get('source_archive_sha256') == source, 'mixed build source')
        if name != 'update':
            require(info.get('component') == name and info.get('target_arch') == 'aarch64'
                    and info.get('development_fixtures') is False, 'component identity or fixture mismatch')
    schema = infos['core'].get('database_schema')
    require(type(schema) is int and 0 <= schema < 2**32
            and type(infos['update'].get('database_schema')) is int
            and infos['update']['database_schema'] == schema, 'schema mismatch')
    contract = infos['core'].get('setup_ap_installation_contract')
    require(isinstance(contract, dict) and bool(contract), 'setup contract unavailable')
    for name in ('edge', 'setup-ap', 'core-launcher'):
        require(json.dumps(infos[name].get('setup_ap_installation_contract'), sort_keys=True)
                == json.dumps(contract, sort_keys=True), 'setup contract mismatch')
    required = {
        'forwarding_application_format_supported': 'update',
        'boot_runtime_selection_supported': 'update',
        'local_installation_preparation_supported': 'edge',
        'media_installation_preparation_supported': 'edge',
    }
    require(all(infos[owner].get(field) is True for field, owner in required.items()),
            'required production capability unavailable')
    return schema, {field: True for field in required}


def collect(binary_dir, output, revision, source, toolchain_manifest, runner,
            info_reader=metadata, version_reader=versions, *, cargo_output=False):
    require(platform.system() == 'Linux' and platform.machine() == 'x86_64',
            'x86_64 Linux workstation required')
    require(re.fullmatch('[0-9a-f]{40}', revision) and re.fullmatch('[0-9a-f]{64}', source),
            'fixed revision and source digest required')
    binary_dir, output = Path(binary_dir), Path(output)
    require(output.is_absolute() and not output.exists() and not output.is_symlink(), 'fresh absolute output required')
    toolchain_hash = sha(regular(toolchain_manifest, 1024**2))
    # Snapshot only the finite production program set; never copy old target
    # build.json, arbitrary Cargo files, local paths, logs or credentials.
    # Cargo hardlinks its own release names to hashed deps artifacts. Only this
    # explicit, operator-built input mode accepts them. Inspection still runs
    # against bounded byte snapshots with independent single-link files below.
    blobs = {name: regular(binary_dir / name, single_link=not cargo_output) for name in PROGRAMS}
    require(sum(map(len, blobs.values())) <= 128 * 1024**2, 'program group size exceeded')
    for data in blobs.values():
        elf(data)
    hashes = {name: sha(data) for name, data in blobs.items()}
    with tempfile.TemporaryDirectory(prefix='.rsh-arm-inspect-', dir=output.parent) as temp:
        staging = Path(temp)
        for name, data in blobs.items():
            path = staging / name
            path.write_bytes(data)
            path.chmod(0o555)
        infos = {name: info_reader(staging / ('rustsimhub-' + name), runner) for name in COMPONENTS}
        schema, caps = contracts(infos, source)
        requirements = {name: version_reader(staging / name) for name in PROGRAMS}
        manifest = {
            'format': 1, 'kind': 'rustsimhub-arm-build', 'revision': revision,
            'source_archive_sha256': source, 'host': 'x86_64-unknown-linux-gnu',
            'target': 'aarch64-unknown-linux-gnu', 'rust': '1.98.0',
            'c_compiler': 'GNU aarch64-linux-gnu-gcc', 'toolchain_manifest_sha256': toolchain_hash,
            'hardware_tested': False, 'development_fixtures': False, 'database_schema': schema,
            **caps, 'glibc_requirements': requirements, 'artifact_sha256': hashes,
        }
        payload = json.dumps(manifest, indent=2, sort_keys=True).encode() + b'\n'
        output.mkdir(mode=0o700)
        try:
            files = {**blobs, 'SHA256SUMS': ''.join(hashes[n] + '  ' + n + '\n' for n in sorted(hashes)).encode(),
                     'build.json': payload}
            for name, data in files.items():
                with (output / name).open('xb') as stream:
                    stream.write(data)
                    stream.flush()
                    os.fsync(stream.fileno())
                (output / name).chmod(0o555 if name in PROGRAMS else 0o444)
        except BaseException:
            with (output / 'BUILD-MANIFEST.FAILED').open('xb') as stream:
                stream.write(b'Incomplete build export; retain for inspection.\n')
            raise
        return manifest


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    sub = parser.add_subparsers(dest='action', required=True)
    verify = sub.add_parser('verify-source')
    for name in ('archive', 'repository', 'revision', 'source'):
        verify.add_argument('--' + name, required=True)
    verify.add_argument('--extract-to')
    export = sub.add_parser('collect')
    for name in ('binary-dir', 'output', 'revision', 'source', 'toolchain-manifest', 'sysroot'):
        export.add_argument('--' + name, required=True)
    export.add_argument('--cargo-output', action='store_true',
                        help='snapshot operator-built Cargo hardlinks; exported files remain independent')
    args = parser.parse_args()
    if args.action == 'verify-source':
        count = verify_source(args.archive, args.repository, args.revision, args.source, args.extract_to)
        print(json.dumps({'git_exact_source_files': count}))
    else:
        value = collect(args.binary_dir, args.output, args.revision, args.source,
                        args.toolchain_manifest, ['qemu-aarch64', '-L', args.sysroot],
                        cargo_output=args.cargo_output)
        print(json.dumps({'programs': len(value['artifact_sha256']), 'database_schema': value['database_schema'],
                          'source_archive_sha256': value['source_archive_sha256'], 'hardware_tested': False}))


if __name__ == '__main__':
    main()
