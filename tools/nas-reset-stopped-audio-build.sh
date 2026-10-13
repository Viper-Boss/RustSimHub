#!/bin/sh
set -eu
base=/mnt/docker-data/rustsimhub/p0
container=rustsimhub-audio-build-p3
test "$(docker inspect "$container" --format '{{.State.Status}}')" = exited
test "$(docker inspect "$container" --format '{{index .Config.Labels "org.rustsimhub.purpose"}}')" = audio-development-build
test "$(docker inspect "$container" --format '{{.Image}}')" = "$(cat "$base/audio-builder-image.txt")"
test "$(docker inspect "$container" --format '{{len .Mounts}}')" = 3
source=$(docker inspect "$container" --format '{{range .Mounts}}{{if eq .Destination "/work"}}{{.Source}}{{end}}{{end}}')
case "$source" in "$base/src"|"$base/audio-src") ;; *) echo 'Unexpected audio source mount' >&2; exit 1;; esac
test "$(docker inspect "$container" --format '{{range .Mounts}}{{if eq .Destination "/work"}}{{.RW}}{{end}}{{end}}')" = false
test "$(docker inspect "$container" --format '{{range .Mounts}}{{if eq .Destination "/audio-target"}}{{.Source}}{{end}}{{end}}')" = "$base/audio-target"
test "$(docker inspect "$container" --format '{{range .Mounts}}{{if eq .Destination "/audio-cargo"}}{{.Source}}{{end}}{{end}}')" = "$base/audio-cargo"
history=$(mktemp -d "$base/history/audio-build-XXXXXXXX")
cp "$base/audio-build.log" "$base/audio-builder-image.txt" "$base/AudioBuilder.Dockerfile" "$history/"
if test -f "$base/audio-source.tar.gz"; then cp "$base/audio-source.tar.gz" "$history/"; else cp "$base/source.tar.gz" "$history/"; fi
docker inspect "$container" --format '{{.State.ExitCode}}' > "$history/build.exit"
if test "$(cat "$history/build.exit")" = 0; then cp "$base/audio-target/release/rustsimhub-audio-alsa" "$history/"; fi
docker rm "$container"
echo 'Stopped audio build evidence preserved; container removed'
