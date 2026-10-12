#!/bin/sh
set -eu
umask 077
install -d -m 0755 /opt/rustsimhub/dev-inbox-p2
install -m 0755 /var/tmp/rustsimhub-core-inbox-p2 /opt/rustsimhub/dev-inbox-p2/rustsimhub-core
install -m 0755 /var/tmp/simhubctl-inbox-p2 /opt/rustsimhub/dev-inbox-p2/simhubctl
base=$(mktemp -d /var/tmp/rustsimhub-inbox-p2-XXXXXXXX)
chown nassimhub:nassimhub "$base"
core=/opt/rustsimhub/dev-inbox-p2/rustsimhub-core
ctl=/opt/rustsimhub/dev-inbox-p2/simhubctl
unit=rustsimhub-inbox-p2
test "$(systemctl is-active "$unit" 2>/dev/null || true)" != active
runuser -u nassimhub -- "$core" --inbox-prepare-fixture "$base"
uid=$(id -u nassimhub)
socket=$base/runtime/core.sock
cleanup() { systemctl stop "$unit" >/dev/null 2>&1 || true; }
trap cleanup EXIT HUP INT TERM
systemd-run --unit "$unit" --property=User=nassimhub --property=Group=nassimhub \
  --property=NoNewPrivileges=yes --property=ProtectSystem=strict --property="ReadWritePaths=$base" \
  --property=RuntimeMaxSec=90 --property=TimeoutStopSec=30 \
  "$core" --serve-observing "$socket" "$uid" "$base/state" "$base/identity"
count=0
while ! printf '%s' '{"protocol":2,"id":"ready","command":{"op":"identity"}}' | \
  runuser -u nassimhub -- "$core" --request-auth "$socket" > "$base/readiness.json" 2>/dev/null; do
    count=$((count+1)); test "$count" -lt 15; sleep 1
done
pin=$(cat "$base/device.pubhex")
for kind in owner status reader; do
    runuser -u nassimhub -- "$ctl" login "$socket" "$base/$kind-key" "$pin" "$base/sessions/$kind.json"
done
count=0
while :; do
    runuser -u nassimhub -- "$ctl" line "$socket" "$base/sessions/status.json" > "$base/line.json"
    if grep -q '"status":"ready"' "$base/line.json"; then break; fi
    count=$((count+1)); test "$count" -lt 20; sleep 1
done
echo 'Stable SIM identity observed without printing identifiers'
count=0
while :; do
    runuser -u nassimhub -- "$ctl" inbox "$socket" "$base/sessions/reader.json" > "$base/inbox.json"
    if grep -Fq '"messages":[{' "$base/inbox.json"; then break; fi
    count=$((count+1)); test "$count" -lt 20; sleep 1
done
# This hardware smoke requires a pre-existing complete incoming modem object.
# Its contents stay private; the test sends no new SMS.
sms_id=$(sed -n 's/.*"id":"\([^"]*\)".*/\1/p' "$base/inbox.json")
test -n "$sms_id"
runuser -u nassimhub -- "$ctl" sms "$socket" "$base/sessions/reader.json" "$sms_id" > "$base/detail.json"
test -s "$base/detail.json"
runuser -u nassimhub -- "$ctl" inbox-events "$socket" "$base/sessions/reader.json" > "$base/events.json"
if runuser -u nassimhub -- "$ctl" inbox "$socket" "$base/sessions/status.json" > "$base/denied.json" 2>/dev/null; then exit 1; fi
if runuser -u nassimhub -- "$ctl" line "$socket" "$base/sessions/reader.json" > "$base/denied-line.json" 2>/dev/null; then exit 1; fi
reader=$(cat "$base/reader-key.pubhex")
runuser -u nassimhub -- "$ctl" revoke "$socket" "$base/sessions/owner.json" "$reader"
if runuser -u nassimhub -- "$ctl" inbox "$socket" "$base/sessions/reader.json" > "$base/revoked.json" 2>/dev/null; then exit 1; fi
if runuser -u nassimhub -- "$ctl" sms "$socket" "$base/sessions/reader.json" "$sms_id" > "$base/revoked-detail.json" 2>/dev/null; then exit 1; fi
echo 'SMS metadata and events require sms_read; live revocation rejected old session'
systemctl stop "$unit"
test ! -S "$socket"
runuser -u nassimhub -- "$core" --inbox-verify-fixture "$base"
find "$base/state" "$base/identity" "$base/sessions" -type f -exec stat -c '%a' {} + | sort -u | grep -qx 600
sha256sum "$core" "$ctl"
systemctl is-active nassimhub-agent ModemManager NetworkManager rmtfs
nmcli -t -f TYPE,STATE device status
echo 'Independent inbox smoke complete; private evidence retained'
