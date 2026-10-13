#!/bin/sh
# No dial/send/ALSA open: independent random identity, explicit authorizer, denied media.
set -eu
umask 077
core=/opt/rustsimhub/dev-media-p3b/rustsimhub-core
ctl=/opt/rustsimhub/dev-media-p3b/simhubctl
audio=/opt/rustsimhub/dev-media-p3b/rustsimhub-audio-alsa
for path in /proc/asound/card0/pcm*/sub*/status; do test "$(cat "$path")" = closed; done
base=$(mktemp -d /var/tmp/rustsimhub-media-denial-p3b-XXXXXXXX)
chown nassimhub:nassimhub "$base"
runuser -u nassimhub -- "$core" --inbox-prepare-fixture "$base"
uid=$(id -u nassimhub)
socket=$base/runtime/core.sock
media=$base/runtime/media.sock
unit=rustsimhub-media-denial-p3b
test "$(systemctl is-active "$unit" 2>/dev/null || true)" != active
cleanup(){ systemctl stop "$unit" >/dev/null 2>&1 || true; }
trap cleanup EXIT HUP INT TERM
systemd-run --unit "$unit" -p User=nassimhub -p Group=nassimhub \
 -p NoNewPrivileges=yes -p ProtectSystem=strict -p "ReadWritePaths=$base" \
 -p RuntimeMaxSec=90 -p TimeoutStopSec=35 \
 "$core" --serve-media-authorizing "$socket" "$uid" "$base/state" "$base/identity" "$media" "$uid"
count=0
while ! test -S "$socket" || ! test -S "$media"; do count=$((count+1));test "$count" -lt 15;sleep 1;done
pin=$(cat "$base/device.pubhex")
runuser -u nassimhub -- "$ctl" login "$socket" "$base/owner-key" "$pin" "$base/sessions/owner.json"
count=0
while :;do
 runuser -u nassimhub -- "$ctl" call-status "$socket" "$base/sessions/owner.json" > "$base/status.json"
 if grep -q '"available":true' "$base/status.json";then break;fi
 count=$((count+1));test "$count" -lt 20;sleep 1
done
grep -q '"active_calls":0' "$base/status.json"
grep -q '"media_authorization_api":true' "$base/status.json"
runuser -u nassimhub -- "$ctl" line "$socket" "$base/sessions/owner.json" > "$base/line.json"
line=$(sed -n 's/.*"line":\([0-9]*\).*/\1/p' "$base/line.json")
boot=$(printf 'a%.0s' $(seq 1 64))
peer=$(printf '07%.0s' $(seq 1 32))
printf '{"call_id":"00000000-0000-0000-0000-000000000000","boot":"%s","line":%s,"generation":1,"expires_in_seconds":30}\n' "$boot" "$line" > "$base/lease.json"
chown nassimhub:nassimhub "$base/lease.json"
if runuser -u nassimhub -- "$ctl" issue-media "$socket" "$base/sessions/owner.json" "$base/lease.json" "$peer" "$base/ticket-issued.json" > "$base/issue.log" 2>&1; then exit 1;fi
test ! -e "$base/ticket-issued.json"
nonce=$(awk 'BEGIN{for(i=0;i<32;i++)printf "%s0",i?",":""}')
key=$(awk 'BEGIN{for(i=0;i<32;i++)printf "%s7",i?",":""}')
printf '{"ticket":[%s],"peer_key":[%s],"boot":"%s"}\n' "$nonce" "$key" "$boot" > "$base/ticket.json"
chown nassimhub:nassimhub "$base/ticket.json"
if systemd-run --unit=rustsimhub-media-native-denial-p3b --collect --wait --pipe \
 -p User=nassimhub -p Group=nassimhub -p NoNewPrivileges=yes -p ProtectSystem=strict \
 -p PrivateDevices=yes -p RuntimeMaxSec=10 -p TimeoutStopSec=3 \
 "$audio" --duplex-authorized "$media" "$uid" "$base/ticket.json" 3 > "$base/native-denied.log" 2>&1;then exit 1;fi
grep -q 'media authorization denied' "$base/native-denied.log"
systemctl stop "$unit"
test ! -S "$socket";test ! -S "$media"
for path in /proc/asound/card0/pcm*/sub*/status; do test "$(cat "$path")" = closed; done
runuser -u nassimhub -- "$core" --inbox-verify-fixture "$base"
systemctl is-active nassimhub-agent ModemManager NetworkManager rmtfs
echo 'Idle media ticket denied; native rejected before PCM; both sockets cleaned; old services active'
