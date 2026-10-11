#!/bin/sh
set -eu
base=/mnt/docker-data/rustsimhub/private-backups/p0
chmod 0600 "$base/boot.img"
printf '%s  %s\n' d5790e07a8ee67c6d80ec24ea9b0cd4b5a78ab4440a3de2b8767fea58a03eaee "$base/boot.img" | sha256sum -c -
stat -c 'Private backup size=%s mode=%a' "$base/boot.img"
