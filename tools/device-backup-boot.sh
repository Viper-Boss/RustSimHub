#!/bin/sh
# Read immutable boot into private backup. Does not write to a block device.
set -eu
base=/var/tmp/rustsimhub-p0-private
install -d -m 0700 "$base"
test ! -e "$base/boot.img"
test "$(readlink -f /dev/disk/by-partlabel/boot)" = /dev/mmcblk0p12
dd if=/dev/disk/by-partlabel/boot of="$base/boot.img" bs=1M status=none
chmod 0600 "$base/boot.img"
printf '%s  %s\n' d5790e07a8ee67c6d80ec24ea9b0cd4b5a78ab4440a3de2b8767fea58a03eaee "$base/boot.img" | sha256sum -c -
stat -c 'Backup size=%s bytes' "$base/boot.img"
