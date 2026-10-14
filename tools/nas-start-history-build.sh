#!/bin/sh
# Offline ARM64 history builder; inspect/save the stopped named container before reusing.
set -eu
archive=${1:?exact committed source archive}
revision=${2:?verified Git revision}
case "$archive" in /mnt/docker-data/rustsimhub/p0/history-rust-*.tar.gz) ;; *) exit 10;; esac
case "$revision" in *[!a-f0-9]*|'') exit 10;; esac
test "${#revision}" -ge 7;test "${#revision}" -le 40
test -f "$archive";test ! -L "$archive"
test "$(docker inspect rustsimhub-build-p0 --format '{{.State.Status}}')" = exited
if docker inspect rustsimhub-history-build-p5 >/dev/null 2>&1;then exit 10;fi
base=$(mktemp -d /mnt/docker-data/rustsimhub/p0/history-build-XXXXXXXX)
chmod 0755 "$base"
install -d -m 0755 "$base/src"
install -d -o 1000 -g 1000 -m 0750 "$base/target"
# Deterministic archive mtime=0 is not valid for a reused Cargo workspace cache.
# Touch extracted sources, keep archive bytes and their hash unchanged.
tar --touch --no-same-owner -xzf "$archive" -C "$base/src"
source_hash=$(sha256sum "$archive" | cut -d' ' -f1)
printf '%s\n' "$source_hash" > "$base/source.sha256"
printf '%s\n' "$revision" > "$base/revision"
image=rust@sha256:2c3a22f0a5533ea2dd5a16627bc841228151faa2d4de2644ac9987e4a2f1f2fa
docker run -d --name rustsimhub-history-build-p5 --label org.rustsimhub.purpose=offline-history-build --label "org.rustsimhub.revision=$revision" \
 --network none --read-only --cap-drop ALL --security-opt no-new-privileges --user 1000:1000 --cpus 2 --memory 1g --pids-limit 128 \
 --tmpfs /tmp:rw,nosuid,size=128m,mode=1777 --mount "type=bind,src=$base/src,dst=/work,readonly" \
 --mount type=bind,src=/mnt/docker-data/rustsimhub/p0/src/.cargo,dst=/cache --mount "type=bind,src=$base/target,dst=/target" \
 -w /work -e CARGO_HOME=/cache -e CARGO_TARGET_DIR=/target -e "RUSTSIMHUB_BUILD_SOURCE_SHA256=$source_hash" "$image" \
 sh -c 'set -e; cargo test -p rustsimhub-compat --locked --offline -j 2; cargo build --release -p rustsimhub-compat --bin rustsimhub-migrate --locked --offline -j 2; /target/release/rustsimhub-migrate --build-info' > "$base/container-id"
printf 'History build dispatched: %s\n' "$base"
