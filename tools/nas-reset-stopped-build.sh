#!/bin/sh
# Preserve evidence and remove only our stopped, labeled, isolated build container.
set -eu
base=/mnt/docker-data/rustsimhub/p0
container=rustsimhub-build-p0
test "$(docker inspect "$container" --format '{{.State.Status}}')" = exited
test "$(docker inspect "$container" --format '{{index .Config.Labels "org.rustsimhub.purpose"}}')" = development-build
test "$(docker inspect "$container" --format '{{range .Mounts}}{{.Source}}{{end}}')" = "$base/src"
test "$(docker inspect "$container" --format '{{.Config.Image}}')" = rust@sha256:2c3a22f0a5533ea2dd5a16627bc841228151faa2d4de2644ac9987e4a2f1f2fa
history=$(mktemp -d "$base/history/build-XXXXXXXX")
cp "$base/source.tar.gz" "$base/build.log" "$base/builder-image.txt" "$history/"
docker inspect "$container" --format '{{.State.ExitCode}}' > "$history/build.exit"
if test "$(cat "$history/build.exit")" = 0; then cp "$base/src/target/release/rustsimhub-core" "$history/"; fi
chown -R 1000:1000 "$history"
docker rm "$container"
echo 'Stopped project build evidence preserved; container removed'
