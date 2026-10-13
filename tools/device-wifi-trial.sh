#!/bin/sh
# Explicit device acceptance: existing Wi-Fi, USB maintenance; no calls/SMS/WWAN methods.
exec 2>&1
set -eu
umask 077
root=/opt/rustsimhub/dev-wifi-p4
core=$root/rustsimhub-core
fixture_core=$root/rustsimhub-core-development
ctl=$root/simhubctl
expected=${EXPECTED_BUILD_SOURCE:?expected source required}
case "$expected" in *[!0-9a-f]*|'') exit 10;;esac
test "${#expected}" -eq 64
"$core" --build-info | grep -Fq "\"source_archive_sha256\":\"$expected\""
"$core" --build-info | grep -q '"development_fixtures":false'
"$fixture_core" --build-info | grep -q '"development_fixtures":true'
"$fixture_core" --build-info | grep -Fq "\"source_archive_sha256\":\"$expected\""
"$ctl" --build-info | grep -Fq "\"source_archive_sha256\":\"$expected\""
units='rustsimhub-wifi-trial-p4 rustsimhub-wifi-fault-p4 rustsimhub-wifi-reopen-p4'
for unit in $units;do if systemctl is-active --quiet "$unit";then exit 10;fi;done
base=$(mktemp -d /var/tmp/rustsimhub-wifi-trial-XXXXXXXX)
"$fixture_core" --inbox-prepare-fixture "$base"
install -d -o rustsimhub-core -g rustsimhub-core -m 0700 "$base/journal"
chown -R rustsimhub-core:rustsimhub-core "$base/state" "$base/identity"
chown rustsimhub-core:rustsimhub-ipc "$base/runtime";chmod 2750 "$base/runtime"
chmod 0711 "$base"
started=''
cleanup(){ for unit in $started;do systemctl stop "$unit" >/dev/null 2>&1 || true;done; }
trap cleanup EXIT HUP INT TERM
socket=$base/runtime/core.sock
printf 'BASE=%s\n' "$base"
# Board profile discovery must be exactly the expected interface types, never WWAN.
get(){ busctl get-property org.freedesktop.NetworkManager "$1" "$2" "$3"; }
connections(){ busctl call org.freedesktop.NetworkManager /org/freedesktop/NetworkManager/Settings org.freedesktop.NetworkManager.Settings ListConnections | tr '"' ' ' | tr ' ' '\n' | sed -n '\|^/org/freedesktop/NetworkManager/Settings/|p' | sort; }
wifi='';usb=''
for path in $(busctl call org.freedesktop.NetworkManager /org/freedesktop/NetworkManager org.freedesktop.NetworkManager GetDevices | tr '"' ' ');do
 case "$path" in /org/freedesktop/NetworkManager/Devices/*)
  iface=$(get "$path" org.freedesktop.NetworkManager.Device Interface)
  kind=$(get "$path" org.freedesktop.NetworkManager.Device DeviceType)
  if test "$iface" = 's "wlan0"' && test "$kind" = 'u 2';then test -z "$wifi";wifi=$path;fi
  if test "$iface" = 's "usb0"' && test "$kind" = 'u 1';then test -z "$usb";usb=$path;fi
  if test "$kind" = 'u 8';then test "$(get "$path" org.freedesktop.NetworkManager.Device ActiveConnection)" = 'o "/"';fi
 ;;esac
done
test -n "$wifi";test -n "$usb"
test "$(get "$wifi" org.freedesktop.NetworkManager.Device State)" = 'u 100'
test "$(get "$usb" org.freedesktop.NetworkManager.Device State)" = 'u 100'
wwan_before=$(get /org/freedesktop/NetworkManager org.freedesktop.NetworkManager WwanEnabled)
active=$(get "$wifi" org.freedesktop.NetworkManager.Device ActiveConnection | cut -d '"' -f2)
profile=$(get "$active" org.freedesktop.NetworkManager.Connection.Active Connection | cut -d '"' -f2)
connections > "$base/profiles-before.txt"
core_start(){
 binary=$1;mode=$2;unit=$3
 systemd-run --unit "$unit" --collect -p User=rustsimhub-core -p Group=rustsimhub-core -p SupplementaryGroups=rustsimhub-ipc -p CapabilityBoundingSet= -p AmbientCapabilities= -p NoNewPrivileges=yes -p PrivateDevices=yes -p ProtectSystem=strict -p ProtectHome=yes -p "ReadWritePaths=$base/state $base/identity $base/runtime $base/journal" -p RuntimeMaxSec=120 -p TimeoutStopSec=5 -p MemoryMax=96M -p TasksMax=8 "$binary" "$mode" "$socket" 0 "$base/state" "$base/identity" "$base/journal"
 started="$started $unit"
 n=0;while ! test -S "$socket";do n=$((n+1));test "$n" -lt 15;sleep 1;done
}
login(){
 label=$1;key=$2
 "$ctl" login "$socket" "$base/$key" "$(cat "$base/device.pubhex")" "$base/sessions/$label.json" > "$base/$label-login.json"
}
core_start "$core" --serve-network-trials rustsimhub-wifi-trial-p4
login readonly status-key
if "$ctl" wifi-trial "$socket" "$base/sessions/readonly.json" readonly-denied > "$base/readonly.log" 2>&1;then exit 10;fi
test "$(find "$base/journal" -maxdepth 1 -type f | wc -l)" = 0
login owner owner-key
"$ctl" wifi-trial "$socket" "$base/sessions/owner.json" normal-once > "$base/normal.json"
grep -q '"rollback_verified":true' "$base/normal.json"
test "$(get /org/freedesktop/NetworkManager org.freedesktop.NetworkManager Checkpoints)" = 'ao 0'
"$ctl" wifi-trial "$socket" "$base/sessions/owner.json" normal-once > "$base/cached.json"
cmp "$base/normal.json" "$base/cached.json"
test "$(find "$base/journal" -maxdepth 1 -name '*.started' | wc -l)" = 1
systemctl stop rustsimhub-wifi-trial-p4
core_start "$core" --serve-network-trials rustsimhub-wifi-reopen-p4
login reopen owner-key
"$ctl" wifi-trial "$socket" "$base/sessions/reopen.json" normal-once > "$base/reopen-cached.json"
cmp "$base/normal.json" "$base/reopen-cached.json"
systemctl stop rustsimhub-wifi-reopen-p4
core_start "$fixture_core" --serve-network-fault-trials rustsimhub-wifi-fault-p4
login fault owner-key
"$ctl" wifi-trial "$socket" "$base/sessions/fault.json" fault-once > "$base/fault-reply.json" 2> "$base/fault-error.log" &
client_pid=$!
n=0
while test "$(find "$base/journal" -maxdepth 1 -name '*.activated' | wc -l)" = 0;do
 if ! kill -0 "$client_pid" 2>/dev/null;then wait "$client_pid" || true;echo 'Fault activation unavailable; no retry';exit 10;fi
 n=$((n+1));test "$n" -lt 20;sleep .1
done
get /org/freedesktop/NetworkManager org.freedesktop.NetworkManager Checkpoints > "$base/fault-checkpoint-before-kill.txt"
test "$(cat "$base/fault-checkpoint-before-kill.txt")" != 'ao 0'
systemctl kill --kill-whom=all --signal=KILL rustsimhub-wifi-fault-p4
wait "$client_pid" || true
# NM's finite checkpoint timer must restore the original profile without Core alive.
n=0
while :;do
 active=$(get "$wifi" org.freedesktop.NetworkManager.Device ActiveConnection | cut -d '"' -f2)
 current=$(get "$active" org.freedesktop.NetworkManager.Connection.Active Connection 2>/dev/null | cut -d '"' -f2 || true)
 if test "$(get "$wifi" org.freedesktop.NetworkManager.Device State)" = 'u 100' && test "$current" = "$profile" && test "$(get /org/freedesktop/NetworkManager org.freedesktop.NetworkManager Checkpoints)" = 'ao 0';then break;fi
 n=$((n+1));test "$n" -lt 15;sleep 1
done
connections > "$base/profiles-after.txt"
cmp "$base/profiles-before.txt" "$base/profiles-after.txt"
test "$(get "$usb" org.freedesktop.NetworkManager.Device State)" = 'u 100'
test "$(get /org/freedesktop/NetworkManager org.freedesktop.NetworkManager WwanEnabled)" = "$wwan_before"
# Reuse normal key should remain cached; uncertain fault key must stay denied after restart.
if test -S "$socket";then test ! -L "$socket";test "$(stat -c %u "$socket")" = "$(id -u rustsimhub-core)";rm -- "$socket";fi
core_start "$core" --serve-network-trials rustsimhub-wifi-reopen-p4
login recovered owner-key
if "$ctl" wifi-trial "$socket" "$base/sessions/recovered.json" fault-once > "$base/uncertain-replay.log" 2>&1;then exit 10;fi
"$ctl" wifi-trial "$socket" "$base/sessions/recovered.json" normal-once > "$base/recovered-cached.json"
cmp "$base/normal.json" "$base/recovered-cached.json"
test "$(find "$base/journal" -maxdepth 1 -name '*.started' | wc -l)" = 2
test "$(find "$base/journal" -maxdepth 1 -name '*.result' | wc -l)" = 1
cleanup
for unit in nassimhub-agent ModemManager NetworkManager rmtfs;do systemctl is-active "$unit";done
for s in /proc/asound/card0/pcm*/sub0/status;do test "$(cat "$s")" = closed;done
printf '%s\n' '{"saved_wifi_trial":true,"read_only_denied":true,"replay_cached_after_restart":true,"core_killed_after_fault_activation":true,"nm_timer_restored_wifi":true,"temporary_profile_removed":true,"usb_preserved":true,"wwan_radio_preserved":true,"uncertain_never_reapplied":true,"cellular_activated":false,"new_calls":0,"new_sms":0}'
