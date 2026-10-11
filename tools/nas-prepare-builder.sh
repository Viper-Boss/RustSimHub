#!/bin/sh
set -eu
base=/mnt/docker-data/rustsimhub/p0
install -d -o 1000 -g 1000 -m 0750 "$base"
if docker image inspect rust:1-slim-bookworm >/dev/null 2>&1; then
    echo 'Rust builder image exists'
else
    nohup sh -c 'docker pull rust:1-slim-bookworm > /mnt/docker-data/rustsimhub/p0/image-pull.log 2>&1; echo $? > /mnt/docker-data/rustsimhub/p0/image-pull.exit' </dev/null >/dev/null 2>&1 &
    echo 'Official Rust builder image pull started'
fi
