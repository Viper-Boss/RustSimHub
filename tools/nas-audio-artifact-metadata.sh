#!/bin/sh
set -eu
base=/mnt/docker-data/rustsimhub/p0
container=rustsimhub-audio-build-p3
test "$(docker inspect "$container" --format '{{.State.Status}}')" = exited
test "$(docker inspect "$container" --format '{{.State.ExitCode}}')" = 0
test "$(docker inspect "$container" --format '{{index .Config.Labels "org.rustsimhub.purpose"}}')" = audio-development-build
test "$(docker inspect "$container" --format '{{.Image}}')" = "$(cat "$base/audio-builder-image.txt")"
head -n 2 "$base/audio-build.log"
grep '^test result:' "$base/audio-build.log"
cat "$base/audio-builder-image.txt"
sha256sum "$base/audio-source.tar.gz" "$base/audio-target/release/rustsimhub-audio-alsa"
stat -c 'size_bytes=%s' "$base/audio-target/release/rustsimhub-audio-alsa"
file "$base/audio-target/release/rustsimhub-audio-alsa" || true
