#!/bin/sh
set -eu
printf 'BOOT RESOLVED PATH\n'; readlink -f /dev/disk/by-partlabel/boot
printf 'BOOT SHA256\n'; sha256sum /dev/disk/by-partlabel/boot
printf 'MODULE MANIFEST SHA256\n'
kernel=$(uname -r)
cd "/lib/modules/$kernel"
find . -type f -print0 | sort -z | xargs -0 sha256sum | sha256sum
printf 'BOOT LOADER PACKAGE\n'
dpkg-query -W -f='${Package} ${Version}\n' 'linux-image*' 'rmtfs' 'qrtr-tools' 'modemmanager' 'network-manager' 2>/dev/null || true
