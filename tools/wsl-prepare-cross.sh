#!/bin/sh
# SPDX-License-Identifier: AGPL-3.0-only
set -eu
umask 077
repo=$(CDPATH= cd -- "$(dirname -- "$0")/.." && pwd)
base="$repo/.local/wsl-cross"
mkdir -p "$base/downloads" "$base/bin" "$base/cargo" "$base/rustup"
exec 9>"$base/build.lock"
flock -n 9
export CARGO_HOME="$base/cargo" RUSTUP_HOME="$base/rustup"
export PATH="$base/cargo/bin:$base/bin:$PATH"
cd "$base/downloads"
if ! test -x "$base/cargo/bin/rustup"; then
 curl --silent --show-error --fail --location --proto '=https' --tlsv1.2 --connect-timeout 15 --max-time 180 --retry 2 -o rustup-init https://static.rust-lang.org/rustup/dist/x86_64-unknown-linux-gnu/rustup-init
 curl --silent --show-error --fail --location --proto '=https' --tlsv1.2 --connect-timeout 15 --max-time 60 -o rustup-init.sha256 https://static.rust-lang.org/rustup/dist/x86_64-unknown-linux-gnu/rustup-init.sha256
 expected=$(cut -d' ' -f1 rustup-init.sha256)
 printf '%s  rustup-init\n' "$expected" | sha256sum -c -
 chmod 0700 rustup-init
 ./rustup-init -y --no-modify-path --profile minimal --default-toolchain 1.98.0 --target aarch64-unknown-linux-gnu
fi
rustup toolchain install 1.98.0 --profile minimal --component rustfmt --component clippy
rustup target add --toolchain 1.98.0 aarch64-unknown-linux-gnu
python3 "$repo/tools/wsl-prepare-gnu.py"
printf '#include <stdio.h>\nint main(void) { puts("rustsimhub-c-arm64-toolchain-ok"); return 0; }\n' > "$base/smoke.c"
"$base/bin/arm64-cc" -O2 -Wl,--fix-cortex-a53-843419 "$base/smoke.c" -o "$base/smoke-c-arm64"
file "$base/smoke-c-arm64"
printf 'fn main() { println!("rustsimhub-local-arm64-toolchain-ok"); }\n' > "$base/smoke.rs"
rustc +1.98.0 --target aarch64-unknown-linux-gnu -C "linker=$base/bin/arm64-cc" "$base/smoke.rs" -o "$base/smoke-arm64"
file "$base/smoke-arm64"
readelf --version-info "$base/smoke-arm64" | sed -n '/Name: GLIBC_/p'
rustc +1.98.0 --version
"$base/bin/arm64-cc" --version | head -n 1
printf 'WSL_CROSS_PREPARED=1\n'
