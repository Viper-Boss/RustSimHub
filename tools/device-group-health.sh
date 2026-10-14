#!/bin/sh
# Root canary runner: finite local status-only client, no owner secret or NAS needed.
set -eu
umask 077
base=${1:?private group directory}
ctl=${2:?verified cli}
case "$base" in /var/tmp/rustsimhub-service-group-????????) ;; *) exit 10;; esac
case "$ctl" in /opt/rustsimhub/dev-state-p1-*/simhubctl) ;; *) exit 10;; esac
test "$(id -u)" = 0
test "$(stat -c '%u:%a:%h' "$base/group.marker")" = 0:600:1
test "$(cat "$base/group.marker")" = RustSimHub.private-service-group.v1
pin=$(cat "$base/device.pin")
test "${#pin}" = 64
case "$pin" in *[!a-f0-9]*) exit 10;; esac
session=$(mktemp -d "$base/health/session-XXXXXXXX")
chown rustsimhub-edge:rustsimhub-edge "$session"
health_unit=rustsimhub-service-group-health-p6
if systemctl is-active --quiet "$health_unit";then exit 10;fi
systemd-run --unit "$health_unit" --wait --pipe --collect \
 -p User=rustsimhub-edge -p Group=rustsimhub-edge -p SupplementaryGroups=rustsimhub-ipc \
 -p CapabilityBoundingSet= -p AmbientCapabilities= -p NoNewPrivileges=yes \
 -p ProtectSystem=strict -p ProtectHome=yes -p PrivateDevices=yes -p PrivateTmp=yes \
 -p "BindReadOnlyPaths=$base/health/key $base/runtime" -p "BindPaths=$session" \
 -p "InaccessiblePaths=/run/dbus -$base/state -$base/identity -$base/edge" \
 -p RuntimeMaxSec=25 -p TimeoutStopSec=3 -p MemoryMax=48M -p TasksMax=8 -p LimitCORE=0 \
 /bin/sh -s "$base/runtime/core.sock" "$base/health/key" "$pin" "$session" "$ctl" <<'HEALTH'
set -eu
socket=$1;key=$2;pin=$3;session=$4;ctl=$5
"$ctl" login "$socket" "$key" "$pin" "$session/login.json" >/dev/null
"$ctl" status "$socket" "$session/login.json" > "$session/health.json"
grep -Fq '"ok":true' "$session/health.json"
n=0;while :;do
 "$ctl" line "$socket" "$session/login.json" > "$session/line.json"
 if grep -Fq '"status":"ready"' "$session/line.json";then break;fi
 n=$((n+1));test "$n" -lt 12;sleep 1
done
if "$ctl" inbox "$socket" "$session/login.json" >/dev/null 2>&1;then exit 10;fi
if "$ctl" open-pairing "$socket" "$session/login.json" >/dev/null 2>&1;then exit 10;fi
printf '%s\n' '{"local_signed_health":true,"line_ready":true,"sms_read_denied":true,"owner_control_denied":true,"nas_required_for_health":false}'
HEALTH
