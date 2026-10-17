# SPDX-License-Identifier: AGPL-3.0-only
"""Pinned UFI003 audio layer for a NEW offline system tree. Never loads a module."""
import hashlib
import json
import os
import pathlib
import shutil
import stat
import struct
import subprocess

ROOT = pathlib.Path(__file__).resolve().parents[1]
SOURCE = ROOT / 'firmware/kernel/audio/modules'
RELEASE = '6.12.49-msm8916-g93a71ee9468d'
PRELOAD = ('q6afe-dai', 'q6asm-dai', 'q6voice-dai', 'snd-soc-apq8016-sbc')
ALIASES = {
    'of:N*T*Cqcom,q6afe-dais': 'q6afe_dai',
    'of:N*T*Cqcom,q6asm': 'q6asm',
    'of:N*T*Cqcom,q6voice-dais': 'q6voice_dai',
    'of:N*T*Cqcom,apq8016-sbc-sndcard': 'snd_soc_apq8016_sbc',
}
ENV = {'PATH': '/usr/sbin:/usr/bin:/sbin:/bin', 'LC_ALL': 'C'}


def require(condition, reason):
    if not condition:
        raise ValueError(reason)


def digest(data):
    return hashlib.sha256(data).hexdigest()


def regular_bytes(path, limit):
    facts = path.lstat()
    require(stat.S_ISREG(facts.st_mode) and facts.st_nlink == 1 and facts.st_size <= limit,
            'Expected a bounded regular file with one link: '+path.name)
    with path.open('rb') as handle:
        data = handle.read(limit+1)
    require(len(data) == facts.st_size and len(data) <= limit, 'Input changed or exceeded limit')
    return data


def profile(source=SOURCE):
    require(not source.is_symlink(), 'Source directory symlink refused')
    raw = regular_bytes(source/'source.json', 256*1024)
    value = json.loads(raw)
    require(value['format'] == 1 and value['kernel_release'] == RELEASE, 'Wrong audio source profile')
    require(value['upstream_kernel_commit'] == '93a71ee9468dd8d47a9e082e5d818608f08712e3'
            and value['revision'] == 'ea019485e5d890a96acae357c52b94c746de837f', 'Unpinned audio source')
    require(set(value['files']) == {'Makefile.outoftree', 'nassimhub-incall-audio.patch'}, 'Unexpected source set')
    for name, expected in value['files'].items():
        data = regular_bytes(source/name, 128*1024)
        require(len(data) == expected['bytes'] and digest(data) == expected['sha256'], 'Audio source differs')
        blob = hashlib.sha1(b'blob '+str(len(data)).encode()+b'\0'+data).hexdigest()
        require(blob == expected['git_blob'], 'Audio source Git blob differs')
    rows = value['expected_modules']
    names = [r['name'] for r in rows]
    require(len(rows) == len(set(names)) == 22, 'Expected exactly 22 distinct audio modules')
    for row in rows:
        name = row['name']
        require(pathlib.PurePosixPath(name).name == name and name.endswith('.ko') and name != '.ko',
                'Unsafe module filename')
    return value, digest(raw)


def modinfo(data):
    require(len(data) >= 64 and data[:6] == b'\x7fELF\x02\x01', 'Expected little-endian ELF64')
    require(struct.unpack_from('<HH', data, 16) == (1, 183), 'Expected ARM64 relocatable module')
    offset = struct.unpack_from('<Q', data, 40)[0]
    size, count, names = struct.unpack_from('<HHH', data, 58)
    require(size == 64 and 0 < count < 4096 and names < count and offset+count*size <= len(data),
            'Invalid ELF section table')
    headers = [struct.unpack_from('<IIQQQQIIQQ', data, offset+i*size) for i in range(count)]
    def content(h):
        require(h[4]+h[5] <= len(data), 'ELF section outside file')
        return data[h[4]:h[4]+h[5]]
    strings = content(headers[names])
    found = []
    for h in headers:
        require(h[0] < len(strings), 'ELF section name outside table')
        if strings[h[0]:].split(b'\0', 1)[0] == b'.modinfo':
            found.append(content(h))
    require(len(found) == 1, 'Missing/duplicate modinfo')
    result = {}
    for item in found[0].split(b'\0'):
        key, separator, value = item.partition(b'=')
        if separator and key in (b'name', b'srcversion', b'vermagic', b'depends'):
            require(key.decode() not in result, 'Duplicate module identity')
            result[key.decode()] = value.decode('ascii')
    return result


def verify_artifacts(artifacts, value):
    require(artifacts.is_dir() and not artifacts.is_symlink(), 'Module input must be a real directory')
    expected = {r['name']: r for r in value['expected_modules']}
    require(set(p.name for p in artifacts.iterdir()) == set(expected), 'Missing/extra module input')
    result = {}
    for name, row in expected.items():
        data = regular_bytes(artifacts/name, 2*1024*1024)
        require(len(data) == row['bytes'] and digest(data) == row['sha256'], 'Module bytes differ: '+name)
        require(modinfo(data) == row['modinfo'], 'Module identity differs: '+name)
        result[name] = data
    return result


def loaded_rows(value):
    """The 16 audio modules reachable from the four fixed preload targets."""
    rows = {r['modinfo']['name'].replace('_','-'): r for r in value['expected_modules']}
    pending = list(PRELOAD)
    names = set()
    while pending:
        name = pending.pop().replace('_','-')
        if name in names or name not in rows:
            continue  # APR and other stock kernel dependencies are outside this audio layer.
        names.add(name)
        pending.extend(filter(None, rows[name]['modinfo']['depends'].split(',')))
    require(len(names) == 16, 'Unexpected pinned audio dependency closure')
    return sorted((rows[n] for n in names), key=lambda r: r['name'])


def target(root, relative, new=False):
    current = root
    path = pathlib.PurePosixPath(relative)
    require(not path.is_absolute() and '..' not in path.parts, 'Unsafe destination')
    for part in path.parts:
        current /= part
        require(not current.is_symlink(), 'Destination symlink refused')
        if current != root.joinpath(*path.parts):
            if not current.exists():
                current.mkdir(mode=0o755)
                current.chmod(0o755)
            require(current.is_dir(), 'Non-directory destination parent')
    if new:
        require(not current.exists(), 'Existing audio output refused')
    return current


def run(*args):
    return subprocess.run(args, check=True, capture_output=True, text=True, env=ENV, timeout=40).stdout


def dependencies(root, value):
    """Query all 22 modules/aliases with target indexes and NO host modprobe config."""
    prefix = root/'lib/modules'/RELEASE
    expected = {r['name']: r for r in value['expected_modules']}
    command = ['/usr/sbin/modprobe', '-C', '/dev/null', '-d', str(root), '-S', RELEASE]
    rows = {}
    for name in expected:
        output = run(*command, '--show-depends', '--ignore-install', name[:-3])
        paths = []
        for line in output.splitlines():
            fields = line.split()
            require(len(fields) == 2 and fields[0] == 'insmod', 'Unexpected modprobe action/options')
            path = pathlib.Path(fields[1])
            relative = path.relative_to(prefix).as_posix()
            require('..' not in pathlib.PurePosixPath(relative).parts, 'Module dependency escaped tree')
            require(path.is_file() and not path.is_symlink(), 'Missing dependency')
            if path.name in expected:
                require(relative == 'updates/rustsimhub-audio/'+path.name, 'Stock audio dependency selected')
                require(digest(regular_bytes(path, 2*1024*1024)) == expected[path.name]['sha256'], 'Installed audio differs')
            paths.append(relative)
        require(paths and paths[-1] == 'updates/rustsimhub-audio/'+name, 'Wrong selected audio module')
        rows[name] = paths
    for alias, name in ALIASES.items():
        require(run(*command, '--resolve-alias', alias).splitlines() == [name], 'Wrong/multiple device-tree alias')
    return rows


def install(root, artifacts, source=SOURCE):
    """Caller owns a fresh composition. On failure retain it; never alter the base."""
    require(root.is_dir() and not root.is_symlink() and root == root.resolve(), 'Noncanonical system tree')
    value, source_sha = profile(source)
    payload = verify_artifacts(artifacts, value)  # Validate all external bytes BEFORE mutations.
    require((root/'lib').is_symlink() and os.readlink(root/'lib') == 'usr/lib', 'Expected merged-/usr tree')
    module_root = target(root, 'usr/lib/modules/'+RELEASE)
    require(module_root.is_dir() and (module_root/'modules.dep').is_file(), 'Missing pinned base modules')
    require({p.name for p in module_root.parent.iterdir()} == {RELEASE}, 'Foreign kernel module tree')
    out = target(root, 'usr/lib/modules/'+RELEASE+'/updates/rustsimhub-audio', new=True)
    require(not (root/'usr/share/rustsimhub/audio-modules.json').exists(), 'Audio manifest already present')
    out.mkdir(mode=0o755);out.chmod(0o755)
    for name, data in payload.items():
        path = out/name
        path.write_bytes(data);path.chmod(0o444);os.chown(path, 0, 0)
    run('/usr/sbin/depmod', '-b', str(root), '-a', RELEASE)
    graph = dependencies(root, value)
    loaded = set()
    for name in PRELOAD:
        loaded.update(pathlib.PurePosixPath(p).name for p in graph[name+'.ko'] if p.startswith('updates/'))
    require(loaded == {r['name'] for r in loaded_rows(value)}, 'Unexpected audio preload closure')
    rows = {r['name']: r for r in value['expected_modules']}
    check_profile = RELEASE+'\n'+''.join(rows[n]['modinfo']['name']+' '+rows[n]['modinfo']['srcversion']+'\n' for n in sorted(loaded))
    path = target(root, 'usr/share/rustsimhub/audio-driver-profile', new=True)
    path.write_text(check_profile);path.chmod(0o444);os.chown(path, 0, 0)
    manifest = {'format': 1, 'kernel_release': RELEASE, 'source_lock_sha256': source_sha,
                'kernel_source_commit': value['upstream_kernel_commit'], 'audio_patch_revision': value['revision'],
                'module_count': len(payload), 'expected_modules': value['expected_modules'],
                'preload': PRELOAD, 'loaded_module_count': len(loaded), 'dependencies': graph,
                'device_tree_aliases': ALIASES, 'driver_profile_sha256': digest(check_profile.encode()),
                'offline_resolution_verified': True, 'modules_loaded_by_composer': False,
                'board_boot_verified': False, 'flash_allowed': False}
    path = target(root, 'usr/share/rustsimhub/audio-modules.json', new=True)
    path.write_text(json.dumps(manifest, indent=2)+'\n');path.chmod(0o444);os.chown(path, 0, 0)
    return manifest
