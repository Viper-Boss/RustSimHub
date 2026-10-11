#!/bin/sh
set -eu
base=/mnt/docker-data/rustsimhub/p0
test -f "$base/source.tar.gz"
install -d -o 1000 -g 1000 -m 0750 "$base/src" "$base/src/.cargo"
tar --no-same-owner -xzf "$base/source.tar.gz" -C "$base/src"
chown -R 1000:1000 "$base/src"
image=$(docker image inspect rust:1-slim-bookworm --format '{{index .RepoDigests 0}}')
printf '%s\n' "$image" > "$base/builder-image.txt"
docker run -d --name rustsimhub-build-p0 --user 1000:1000 --cpus 2 --memory 2g --pids-limit 256 \
    --mount "type=bind,src=$base/src,dst=/work" -w /work -e CARGO_HOME=/work/.cargo "$image" \
    sh -c 'set -e; rustc --version; cargo --version; cargo test --workspace --locked -j 2; cargo build --release --workspace --locked -j 2; ./target/release/rustsimhub-core --self-check' \
    > "$base/container-id.txt"
nohup sh -c 'docker logs -f rustsimhub-build-p0 > /mnt/docker-data/rustsimhub/p0/build.log 2>&1; docker inspect rustsimhub-build-p0 --format "{{.State.ExitCode}}" > /mnt/docker-data/rustsimhub/p0/build.exit' </dev/null >/dev/null 2>&1 &
echo 'Isolated ARM64 build started'
