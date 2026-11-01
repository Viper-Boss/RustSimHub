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
RUSTSIMHUB_CROSS_HOME=${RUSTSIMHUB_CROSS_HOME:-"$repo/.local/wsl-cross"}
export RUSTSIMHUB_CROSS_HOME
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
# Compare the complete Git inventory and extract those same verified bytes.
# Normal git archive directory entries are allowed; missing/extra/duplicate
# files and symlinks are not. No host checkout or private data is copied.
python3 "$repo/tools/arm_build_manifest.py" verify-source \
 --archive "$archive" --repository "$repo" --revision "$revision" \
 --source "$expected" --extract-to "$work/src"
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
gnu_root=$(cat "$RUSTSIMHUB_CROSS_HOME/gnu-root.txt")
python3 "$repo/tools/arm_build_manifest.py" collect \
 --binary-dir "$CARGO_TARGET_DIR/aarch64-unknown-linux-gnu/release" \
 --output "$work/artifacts" --revision "$revision" --source "$expected" \
 --toolchain-manifest "$RUSTSIMHUB_CROSS_HOME/downloads/gnu-packages.json" \
 --sysroot "$gnu_root/usr/aarch64-linux-gnu"
out="$repo/.local/wsl-artifacts-$revision"
test ! -e "$out"
mkdir "$out"
cp "$work/artifacts/"* "$out/"
printf 'ARM_BUILD_END=%s\n' "$(date -u +%FT%TZ)"
printf 'LOCAL_ARM64_BUILD_COMPLETE=%s\n' "$out"
