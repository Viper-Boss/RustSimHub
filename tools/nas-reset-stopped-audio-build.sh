#!/bin/sh
set -eu
base=/mnt/docker-data/rustsimhub/p0
container=rustsimhub-audio-build-p3
test "$(docker inspect "$container" --format '{{.State.Status}}')" = exited
test "$(docker inspect "$container" --format '{{index .Config.Labels "org.rustsimhub.purpose"}}')" = audio-development-build
test "$(docker inspect "$container" --format '{{.Image}}')" = "$(cat "$base/audio-builder-image.txt")"
mounts=$(docker inspect "$container" --format '{{range .Mounts}}{{.Destination}}={{.Source}};{{end}}')
case "$mounts" in
    "/work=$base/src;/audio-target=$base/audio-target;/audio-cargo=$base/audio-cargo;"|"/work=$base/audio-src;/audio-target=$base/audio-target;/audio-cargo=$base/audio-cargo;") ;;
    *) echo 'Unexpected audio container mounts' >&2; exit 1;;
esac
history=$(mktemp -d "$base/history/audio-build-XXXXXXXX")
cp "$base/audio-build.log" "$base/audio-builder-image.txt" "$base/AudioBuilder.Dockerfile" "$history/"
if test -f "$base/audio-source.tar.gz"; then cp "$base/audio-source.tar.gz" "$history/"; else cp "$base/source.tar.gz" "$history/"; fi
docker inspect "$container" --format '{{.State.ExitCode}}' > "$history/build.exit"
if test "$(cat "$history/build.exit")" = 0; then cp "$base/audio-target/release/rustsimhub-audio-alsa" "$history/"; fi
docker rm "$container"
echo 'Stopped audio build evidence preserved; container removed'
