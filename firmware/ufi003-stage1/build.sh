#!/usr/bin/env bash
# SPDX-License-Identifier: AGPL-3.0-only
set -euo pipefail
export LC_ALL=C

if (( $# != 2 )); then
    echo "Usage: $0 <lk2nd checkout> <output directory>" >&2
    exit 2
fi

recipe=$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")" && pwd)
upstream=$1
output=$(realpath -m -- "$2")
revision=c7a0aa2fc316f8284b7bcefe14aaef0dd11f04cb
version=rustsimhub-ufi003-1
toolchain=${TOOLCHAIN_PREFIX:-arm-none-eabi-}
jobs=${JOBS:-4}

git -C "$upstream" cat-file -e "$revision^{commit}"
work=$(mktemp -d)
trap 'rm -rf -- "$work"' EXIT
git -C "$upstream" archive "$revision" | tar -xf - -C "$work"
git -C "$work" apply --unidiff-zero "$recipe/ufi003-lk1st.patch"
make -C "$work" -j"$jobs" TOOLCHAIN_PREFIX="$toolchain" \
    LK2ND_BUNDLE_DTB=msm8916-512mb-mtp.dtb LK2ND_COMPATIBLE=thwc,ufi003 \
    LK2ND_VERSION="$version" RUSTSIMHUB_UFI003=1 lk1st-msm8916

image="$work/build-lk1st-msm8916/emmc_appsboot.mbn"
"${toolchain}readelf" -h "$image" > "$work/elf-header.txt"
grep -Eq 'Class:[[:space:]]+ELF32' "$work/elf-header.txt"
grep -Eq 'Machine:[[:space:]]+ARM$' "$work/elf-header.txt"
"${toolchain}strings" "$image" > "$work/strings.txt"
grep -Fxq "$version" "$work/strings.txt"
grep -Fxq 'rustsimhub:storage-layout' "$work/strings.txt"

mkdir -p -- "$output"
cp -- "$image" "$output/RustSimHub-UFI003-stage1.elf"
(cd -- "$output" && sha256sum RustSimHub-UFI003-stage1.elf > SHA256SUMS.txt)
