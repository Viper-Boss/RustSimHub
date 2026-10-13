#!/bin/sh
set -eu
base=/mnt/docker-data/rustsimhub/p0
container=rustsimhub-audio-build-p3
if docker inspect "$container" >/dev/null 2>&1; then echo 'Audio builder already exists; preserve and inspect it first' >&2; exit 1; fi
install -d -o 1000 -g 1000 -m 0750 "$base/audio-src" "$base/audio-image-context" "$base/audio-target" "$base/audio-cargo"
test -f "$base/audio-upload.tar.gz"
cp "$base/audio-upload.tar.gz" "$base/audio-source.tar.gz"
tar --touch --no-same-owner -xzf "$base/audio-source.tar.gz" -C "$base/audio-src"
chown -R 1000:1000 "$base/audio-src"
# Native dependency installation occurs in this derived image, never on NAS or stick.
docker build --network=none --label org.rustsimhub.purpose=audio-development-build -t rustsimhub-audio-builder:p3 -f "$base/AudioBuilder.Dockerfile" "$base/audio-image-context" > "$base/audio-image.log" 2>&1
image=$(docker image inspect rustsimhub-audio-builder:p3 --format '{{.Id}}')
test "$(docker image inspect "$image" --format '{{.Architecture}}')" = arm64
printf '%s\n' "$image" > "$base/audio-builder-image.txt"
docker run -d --name "$container" --label org.rustsimhub.purpose=audio-development-build --user 1000:1000 --cpus 2 --memory 1g --pids-limit 256 \
 --mount "type=bind,src=$base/audio-src,dst=/work,readonly" \
 --mount "type=bind,src=$base/audio-target,dst=/audio-target" \
 --mount "type=bind,src=$base/audio-cargo,dst=/audio-cargo" \
 -w /work -e CARGO_HOME=/audio-cargo -e CARGO_TARGET_DIR=/audio-target -e "RUSTSIMHUB_BUILD_SOURCE_SHA256=$(sha256sum "$base/audio-source.tar.gz" | cut -d' ' -f1)" "$image" \
 sh -c 'set -e; rustc --version; pkg-config --modversion alsa; cargo test --manifest-path native/audio-alsa/Cargo.toml --locked -j 2; cargo build --manifest-path native/audio-alsa/Cargo.toml --release --locked -j 2' > "$base/audio-container-id.txt"
nohup sh -c 'docker logs -f rustsimhub-audio-build-p3 > /mnt/docker-data/rustsimhub/p0/audio-build.log 2>&1; docker inspect rustsimhub-audio-build-p3 --format "{{.State.ExitCode}}" > /mnt/docker-data/rustsimhub/p0/audio-build.exit' </dev/null >/dev/null 2>&1 &
echo 'Isolated native ARM64 audio build started'
