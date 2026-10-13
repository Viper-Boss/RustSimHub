#!/bin/sh
set -eu
base=/mnt/docker-data/rustsimhub/p0
test -f "$base/source.tar.gz"
if docker container inspect rustsimhub-build-p0 >/dev/null 2>&1; then
    echo 'Named build container already exists. Inspect/save its logs and remove only this stopped build container before restarting.' >&2
    exit 1
fi
install -d -o 1000 -g 1000 -m 0750 "$base/src" "$base/src/.cargo"
# Current extraction time prevents old archive mtimes from hiding changes made
# while a previous long Cargo build was still running.
tar --touch --no-same-owner -xzf "$base/source.tar.gz" -C "$base/src"
chown -R 1000:1000 "$base/src"
image=rust@sha256:2c3a22f0a5533ea2dd5a16627bc841228151faa2d4de2644ac9987e4a2f1f2fa
test "$(docker image inspect "$image" --format '{{.Architecture}}')" = arm64
printf '%s\n' "$image" > "$base/builder-image.txt"
rm -f "$base/build.exit"
docker run -d --name rustsimhub-build-p0 --label org.rustsimhub.purpose=development-build --user 1000:1000 --cpus 2 --memory 2g --pids-limit 256 \
    --mount "type=bind,src=$base/src,dst=/work" -w /work -e CARGO_HOME=/work/.cargo -e "RUSTSIMHUB_BUILD_SOURCE_SHA256=$(sha256sum "$base/source.tar.gz" | cut -d' ' -f1)" "$image" \
    sh -c 'set -e; rustc --version; cargo --version; cargo check --workspace --no-default-features --locked -j 2; cargo test --workspace --all-features --locked -j 2; cargo build --release --workspace --features rustsimhub-core/dev-fixtures --locked -j 2; ./target/release/rustsimhub-core --self-check; ./target/release/simhubctl --help; mkdir -p target/development target/production; for name in rustsimhub-core simhubctl rustsimhub-edge rustsimhub-update rustsimhub-recovery; do cp target/release/$name target/development/$name; done; cargo build --release -p rustsimhub-core --no-default-features --locked -j 2; ./target/release/rustsimhub-core --build-info | grep -q "\"development_fixtures\":false"; for name in rustsimhub-core simhubctl rustsimhub-edge rustsimhub-update rustsimhub-recovery; do cp target/release/$name target/production/$name; done' \
    > "$base/container-id.txt"
nohup sh -c 'docker logs -f rustsimhub-build-p0 > /mnt/docker-data/rustsimhub/p0/build.log 2>&1; docker inspect rustsimhub-build-p0 --format "{{.State.ExitCode}}" > /mnt/docker-data/rustsimhub/p0/build.exit' </dev/null >/dev/null 2>&1 &
echo 'Isolated ARM64 build started'
