#!/bin/sh
set -eu
base=/mnt/docker-data/rustsimhub/p0
printf '\nBUILDER DIGEST\n'; cat "$base/builder-image.txt"
printf '\nBUILD TOOLCHAIN\n'; head -n 2 "$base/build.log"
printf '\nSOURCE ARCHIVE HASH\n'; sha256sum "$base/source.tar.gz"
printf '\nARTIFACT\n'; sha256sum "$base/src/target/release/rustsimhub-core"
stat -c 'size_bytes=%s' "$base/src/target/release/rustsimhub-core"
file "$base/src/target/release/rustsimhub-core" 2>/dev/null || true
