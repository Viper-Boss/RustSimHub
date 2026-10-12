#!/bin/sh
set -eu
upload=/var/tmp/rustsimhub-core-auth-p1
binary=/opt/rustsimhub/dev-auth-p1/rustsimhub-core
test -f "$upload"
install -d -m 0755 /opt/rustsimhub/dev-auth-p1
install -m 0755 "$upload" "$binary"
base=$(mktemp -d /var/tmp/rustsimhub-auth-p1-XXXXXXXX)
chown nassimhub:nassimhub "$base"
runuser -u nassimhub -- "$binary" --auth-check "$base"
runuser -u nassimhub -- "$binary" --auth-reopen-fixture "$base"
socket=$base/core.sock
uid=$(id -u nassimhub)
unit=rustsimhub-auth-p1
if systemctl is-active --quiet "$unit"; then
    echo 'Existing project auth test unit is running; refusing to replace it' >&2
    exit 1
fi
systemd-run --unit "$unit" --property=User=nassimhub --property=Group=nassimhub \
    --property=NoNewPrivileges=yes --property=ProtectSystem=strict \
    --property="ReadWritePaths=$base" --property=RuntimeMaxSec=180 \
    "$binary" --serve-authenticated "$socket" "$uid" "$base/state" "$base/identity"
trap 'systemctl stop "$unit" >/dev/null 2>&1 || true' EXIT HUP INT TERM
ready=0
while [ "$ready" -lt 10 ]; do
    test ! -S "$socket" || break
    sleep 1
    ready=$((ready+1))
done
test -S "$socket"
runuser -u nassimhub -- "$binary" --auth-ipc-fixture "$base" "$socket"
# root can traverse the private directory but still must fail SO_PEERCRED admission.
printf '%s' '{"protocol":2,"id":"uid-check","command":{"op":"identity"}}' > "$base/uid-request.json"
set +e
"$binary" --request-auth "$socket" < "$base/uid-request.json" >/dev/null 2>&1
code=$?
set -e
test "$code" != 0
echo 'Different UID denied on authenticated socket'
systemctl stop "$unit"
trap - EXIT HUP INT TERM
printf '\nPRIVATE MATERIAL MODES\n'
find "$base" -type f -printf '%m %u %s bytes\n'
printf '\nARTIFACT\n'
sha256sum "$binary"
printf '\nOLD SERVICES\n'
systemctl is-active nassimhub-agent ModemManager NetworkManager rmtfs
printf '\nNETWORK TYPES\n'
nmcli -t -f TYPE,STATE device status
echo 'Only synthetic identity and fixture records used; private temporary evidence retained'
