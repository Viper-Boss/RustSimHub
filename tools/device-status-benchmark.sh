#!/bin/sh
# Local status-only signed client; measures complete CLI+IPC operation, not network latency.
set -eu
umask 077
base=${1:?existing verified private application group}
ctl=${2:?verified CLI}
case "$base" in /var/tmp/rustsimhub-service-group-????????) ;; *) exit 10;; esac
case "$ctl" in /opt/rustsimhub/dev-state-p1-*/simhubctl) ;; *) exit 10;; esac
test "$(id -u)" = 0
test "$(stat -c '%u:%a:%h' "$base/group.marker")" = 0:600:1
test "$(cat "$base/group.marker")" = RustSimHub.private-service-group.v1
pin=$(cat "$base/device.pin")
test "${#pin}" = 64
case "$pin" in *[!a-f0-9]*) exit 10;; esac
session=$(mktemp -d "$base/health/benchmark-XXXXXXXX")
chown rustsimhub-edge:rustsimhub-edge "$session"
if systemctl is-active --quiet rustsimhub-status-benchmark-p7;then exit 10;fi
systemd-run --quiet --unit rustsimhub-status-benchmark-p7 --wait --pipe --collect \
 -p User=rustsimhub-edge -p Group=rustsimhub-edge -p SupplementaryGroups=rustsimhub-ipc \
 -p CapabilityBoundingSet= -p AmbientCapabilities= -p NoNewPrivileges=yes \
 -p ProtectSystem=strict -p ProtectHome=yes -p PrivateDevices=yes -p PrivateTmp=yes \
 -p "BindReadOnlyPaths=$base/health/key $base/runtime" -p "BindPaths=$session" \
 -p "InaccessiblePaths=/run/dbus -$base/state -$base/identity -$base/edge" \
 -p RuntimeMaxSec=80 -p TimeoutStopSec=3 -p MemoryMax=48M -p TasksMax=8 -p LimitCORE=0 \
 /bin/sh -s "$base/runtime/core.sock" "$base/health/key" "$pin" "$session" "$ctl" <<'BENCHMARK'
set -eu
socket=$1;key=$2;pin=$3;session=$4;ctl=$5
"$ctl" login "$socket" "$key" "$pin" "$session/login.json" >/dev/null
"$ctl" status "$socket" "$session/login.json" > "$session/status.json"
grep -Fq '"ok":true' "$session/status.json"
if "$ctl" inbox "$socket" "$session/login.json" >/dev/null 2>&1;then exit 10;fi
if "$ctl" open-pairing "$socket" "$session/login.json" >/dev/null 2>&1;then exit 10;fi
printf '%s\n' BEGIN_SAMPLES
i=0
while test "$i" -lt 200;do
 before=$(date +%s%N)
 "$ctl" status "$socket" "$session/login.json" > "$session/status.json"
 after=$(date +%s%N)
 grep -Fq '"ok":true' "$session/status.json"
 case "$before$after" in *[!0-9]*) exit 10;; esac
 duration=$((after-before));test "$duration" -ge 0
 printf '%s\n' "$duration"
 i=$((i+1))
done
printf '%s\n' END_SAMPLES
BENCHMARK
