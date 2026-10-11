#!/bin/sh
set -eu
base=/mnt/docker-data/rustsimhub/p0
container=rustsimhub-build-p0
test "$(docker inspect "$container" --format '{{.State.Status}} {{.State.ExitCode}}')" = 'exited 0'
test "$(docker inspect "$container" --format '{{range .Mounts}}{{.Source}}{{end}}')" = "$base/src"
history="$base/history/p0-first"
test ! -e "$history"
install -d -o 1000 -g 1000 -m 0750 "$history"
cp "$base/source.tar.gz" "$base/build.log" "$base/builder-image.txt" "$base/src/target/release/rustsimhub-core" "$history/"
chown -R 1000:1000 "$history"
docker rm "$container"
echo 'Verified completed project build archived; only its container removed'
