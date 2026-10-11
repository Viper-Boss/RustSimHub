#!/bin/sh
set -u
base=/mnt/docker-data/rustsimhub/p0
if [ -f "$base/image-pull.exit" ]; then printf 'PULL EXIT='; cat "$base/image-pull.exit"; fi
tail -n 6 "$base/image-pull.log" 2>/dev/null || true
docker image inspect rust:1-slim-bookworm --format '{{.Architecture}} {{index .RepoDigests 0}}' 2>/dev/null || true
docker ps -a --filter name=rustsimhub-build-p0 --format '{{.Names}} {{.Status}}'
if [ -f "$base/build.exit" ]; then printf 'BUILD EXIT='; cat "$base/build.exit"; fi
tail -n 12 "$base/build.log" 2>/dev/null || true
