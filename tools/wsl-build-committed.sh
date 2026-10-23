#!/bin/sh
# SPDX-License-Identifier: AGPL-3.0-only
# Compile a verified committed archive on the x86_64 workstation; never touches hardware.
set -eu
umask 077
repo=$(CDPATH= cd -- "$(dirname -- "$0")/.." && pwd)
archive=${1:?absolute source archive path}
revision=${2:?full committed revision}
expected=${3:?archive SHA256}
case "$revision" in *[!0-9a-f]*|'') exit 10;; esac
test "${#revision}" -eq 40
case "$expected" in *[!0-9a-f]*|'') exit 10;; esac
test "${#expected}" -eq 64
mkdir -p "$repo/.private"
exec >"$repo/.private/wsl-build-$revision.log" 2>&1
trap 'code=$?; printf "%s\n" "$code" > "$repo/.private/wsl-build-$revision.exit"' EXIT
test -f "$archive"; test ! -L "$archive"
test "$(sha256sum "$archive" | cut -d' ' -f1)" = "$expected"
export RUSTSIMHUB_CROSS_HOME="$repo/.local/wsl-cross"
test -f "$RUSTSIMHUB_CROSS_HOME/env.sh"
. "$RUSTSIMHUB_CROSS_HOME/env.sh"
exec 9>"$RUSTSIMHUB_CROSS_HOME/build.lock"
flock -n 9
build_home=$(python3 - <<'PY'
import os,pathlib
p=pathlib.Path.home()/'.cache/rustsimhub'
p.mkdir(mode=0o700,parents=True,exist_ok=True)
if p.is_symlink() or p.stat().st_uid!=os.getuid() or p.stat().st_mode&0o077: raise SystemExit('Private build cache required')
print(p)
PY
)
work=$(mktemp -d "$build_home/build.XXXXXXXX")
printf '%s\n' "$work" > "$repo/.private/wsl-build-workdir.txt"
mkdir "$work/src" "$work/artifacts"
python3 - "$archive" "$work/src" <<'PY'
import pathlib,sys,tarfile
with tarfile.open(sys.argv[1]) as tar:
 for item in tar.getmembers():
  path=pathlib.PurePosixPath(item.name)
  if not item.isfile() or path.is_absolute() or '..' in path.parts:
   raise SystemExit('Unexpected source archive entry')
 tar.extractall(sys.argv[2],filter='data')
PY
# Git comparison prevents archives from being mislabeled by revision or source hash.
python3 - "$archive" "$repo" "$revision" <<'PY'
import subprocess,sys,tarfile
with tarfile.open(sys.argv[1]) as tar:
 for item in tar.getmembers():
  actual=tar.extractfile(item).read()
  committed=subprocess.check_output(['git','-C',sys.argv[2],'show',sys.argv[3]+':'+item.name])
  if actual!=committed: raise SystemExit('Source differs from the specified Git commit')
PY
find "$work/src" -type f -exec touch {} +
export CARGO_TARGET_DIR="$build_home/target"
export RUSTSIMHUB_BUILD_SOURCE_SHA256="$expected"
cd "$work/src"
rustc +1.98.0 --version
jobs=${RUSTSIMHUB_BUILD_JOBS:-8}
case "$jobs" in 1|2|4|8|12) ;; *) exit 10;; esac
printf 'HOST_TEST_START=%s\n' "$(date -u +%FT%TZ)"
cargo +1.98.0 test --workspace --all-features --locked -j "$jobs"
printf 'ARM_BUILD_START=%s\n' "$(date -u +%FT%TZ)"
cargo +1.98.0 build --release --workspace --no-default-features --locked --target aarch64-unknown-linux-gnu -j "$jobs"
for name in rustsimhub-core simhubctl rustsimhub-edge rustsimhub-update rustsimhub-recovery rustsimhub-migrate rustsimhub-status-led rustsimhub-network-agent; do
 install -m 0555 "$CARGO_TARGET_DIR/aarch64-unknown-linux-gnu/release/$name" "$work/artifacts/$name"
 file "$work/artifacts/$name"
 readelf -h "$work/artifacts/$name" | grep 'Machine:.*AArch64'
done
python3 - "$work" "$revision" "$expected" "$RUSTSIMHUB_CROSS_HOME/downloads/gnu-packages.json" <<'PY'
import hashlib,json,pathlib,re,subprocess,sys
work=pathlib.Path(sys.argv[1])
requirements={};hashes={}
for binary in (work/'artifacts').iterdir():
 hashes[binary.name]=hashlib.sha256(binary.read_bytes()).hexdigest()
 text=subprocess.check_output(['readelf','--version-info',str(binary)],text=True)
 versions=set(re.findall(r'GLIBC_([0-9]+\.[0-9]+)',text))
 if any(tuple(map(int,v.split('.')))>(2,41) for v in versions): raise SystemExit('Device glibc boundary exceeded')
 requirements[binary.name]=sorted(versions,key=lambda v:tuple(map(int,v.split('.'))))
(work/'artifacts'/'SHA256SUMS').write_text(''.join(hashes[name]+'  '+name+'\n' for name in sorted(hashes)))
schema=int(re.search(r'const SCHEMA: i64 = (\d+);',(work/'src/crates/storage/src/lib.rs').read_text())[1])
assert schema==int(re.search(r'const SCHEMA: u32 = (\d+);',(work/'src/crates/update/src/main.rs').read_text())[1])
assert schema==int(re.search(r'"database_schema":(\d+)',(work/'src/crates/core/src/main.rs').read_text())[1])
(work/'artifacts'/'build.json').write_text(json.dumps({'revision':sys.argv[2],'source_archive_sha256':sys.argv[3],'host':'x86_64-unknown-linux-gnu','target':'aarch64-unknown-linux-gnu','rust':'1.98.0','c_compiler':'GNU aarch64-linux-gnu-gcc','toolchain_manifest_sha256':hashlib.sha256(pathlib.Path(sys.argv[4]).read_bytes()).hexdigest(),'hardware_tested':False,'development_fixtures':False,'database_schema':schema,'boot_runtime_selection_supported':'"boot_runtime_selection_supported":true' in (work/'src/crates/update/src/main.rs').read_text(),'local_installation_preparation_supported':'"local_installation_preparation_supported":true' in (work/'src/crates/edge/src/linux.rs').read_text(),'media_installation_preparation_supported':'"media_installation_preparation_supported":true' in (work/'src/crates/edge/src/linux.rs').read_text(),'glibc_requirements':requirements,'artifact_sha256':hashes},indent=2)+'\n')
PY
out="$repo/.local/wsl-artifacts-$revision"
test ! -e "$out"
mkdir "$out"
cp "$work/artifacts/"* "$out/"
printf 'ARM_BUILD_END=%s\n' "$(date -u +%FT%TZ)"
printf 'LOCAL_ARM64_BUILD_COMPLETE=%s\n' "$out"
