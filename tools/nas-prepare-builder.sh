#!/bin/sh
set -eu
base=/mnt/docker-data/rustsimhub/p0
install -d -o 1000 -g 1000 -m 0750 "$base"
image=rust@sha256:2c3a22f0a5533ea2dd5a16627bc841228151faa2d4de2644ac9987e4a2f1f2fa
if docker image inspect "$image" >/dev/null 2>&1; then
    echo 'Rust builder image exists'
else
    nohup sh -c 'docker pull rust@sha256:2c3a22f0a5533ea2dd5a16627bc841228151faa2d4de2644ac9987e4a2f1f2fa > /mnt/docker-data/rustsimhub/p0/image-pull.log 2>&1; echo $? > /mnt/docker-data/rustsimhub/p0/image-pull.exit' </dev/null >/dev/null 2>&1 &
    echo 'Official Rust builder image pull started'
fi
