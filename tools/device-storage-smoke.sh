#!/bin/sh
set -eu
upload=/var/tmp/rustsimhub-core-p1
binary=/opt/rustsimhub/dev-p1/rustsimhub-core
test -f "$upload"
install -d -m 0755 /opt/rustsimhub/dev-p1
install -m 0755 "$upload" "$binary"
base=$(mktemp -d /var/tmp/rustsimhub-storage-p1-XXXXXXXX)
chown nassimhub:nassimhub "$base"
install -d -m 0700 -o nassimhub -g nassimhub "$base/check" "$base/crash"
runuser -u nassimhub -- "$binary" --storage-check "$base/check"
set +e
runuser -u nassimhub -- "$binary" --storage-crash-fixture "$base/crash"
code=$?
set -e
test "$code" = 73
runuser -u nassimhub -- "$binary" --storage-recover-fixture "$base/crash"
printf '\nPRIVATE DATABASE MODES\n'
find "$base" -name '*.sqlite3' -printf '%m %u %s bytes\n'
printf '\nARTIFACT\n'; sha256sum "$binary"
printf '\nOLD SERVICES\n'; systemctl is-active nassimhub-agent ModemManager NetworkManager rmtfs
printf '\nNETWORK TYPES\n'; nmcli -t -f TYPE,STATE device status
echo 'Synthetic fixture databases retained in isolated private temporary directory'
