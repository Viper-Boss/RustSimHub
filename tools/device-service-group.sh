#!/bin/sh
# Private, reversible application group acceptance. Does not install boot units.
# Root selects verified app payloads; services receive a read-only bind of that slot.
exec 2>&1
set -eu
umask 077
action=${1:?start or stop}
base=${2:?private group directory}
updater=${3:?verified updater}
case "$action" in start|stop) ;; *) exit 10;; esac
case "$base" in /var/tmp/rustsimhub-service-group-????????) ;; *) exit 10;; esac
case "$updater" in /opt/rustsimhub/dev-state-p1-*/rustsimhub-update) ;; *) exit 10;; esac
test "$(id -u)" = 0
test ! -L "$base"
test "$(stat -c '%u:%a' "$base")" = 0:711
test ! -L "$base/group.marker"
test "$(stat -c '%u:%a:%h' "$base/group.marker")" = 0:600:1
test "$(cat "$base/group.marker")" = RustSimHub.private-service-group.v1
core_unit=rustsimhub-service-group-core-p6
edge_unit=rustsimhub-service-group-edge-p6
stop_group(){
 systemctl stop "$edge_unit" "$core_unit" >/dev/null 2>&1 || true
 for unit in "$core_unit" "$edge_unit";do
  if systemctl is-active --quiet "$unit";then return 1;fi
 done
 if test -e "$base/runtime/core.sock";then
  test ! -L "$base/runtime/core.sock"
  test -S "$base/runtime/core.sock"
  test "$(stat -c %u "$base/runtime/core.sock")" = "$(id -u rustsimhub-core)"
  rm -- "$base/runtime/core.sock"
 fi
}
if test "$action" = stop;then stop_group;exit 0;fi
for unit in "$core_unit" "$edge_unit";do
 if systemctl is-active --quiet "$unit";then exit 10;fi
done
for part in state identity;do
 test ! -L "$base/$part"
 test "$(stat -c '%u:%a' "$base/$part")" = "$(id -u rustsimhub-core):700"
done
test "$(stat -c '%u:%a' "$base/edge")" = "$(id -u rustsimhub-edge):700"
test "$(stat -c '%u:%a' "$base/runtime")" = "$(id -u rustsimhub-core):2750"
test ! -e "$base/runtime/core.sock"
slot=$("$updater" choose "$base/store" "$base/release.public")
suffix=${slot##*/}
test "${#suffix}" = 64
case "$suffix" in *[!a-f0-9]*) exit 10;; esac
test "$slot" = "$base/store/versions/$suffix"
test ! -L "$slot"
test "$(stat -c %u "$slot")" = 0
# Public signed executables must be traversable inside each private mount namespace.
# Store ancestors and selector state remain root-private; no group gets write access.
chmod 0755 "$slot"
for name in rustsimhub-core simhubctl rustsimhub-edge rustsimhub-audio-alsa;do
 test ! -L "$slot/$name"
 test "$(stat -c '%u:%h' "$slot/$name")" = 0:1
 # stage runs with a private umask: make only the verified public executables
 # readable/executable for the isolated service users, never writable by them.
 chmod 0555 "$slot/$name"
done
"$slot/rustsimhub-core" --build-info | grep -Fq '"development_fixtures":false'
printf '%s\n' "$suffix" > "$base/selected.hash"
started=no
cleanup(){ if test "$started" = yes;then stop_group;fi; }
trap cleanup EXIT HUP INT TERM
started=yes
systemd-run --unit "$core_unit" --collect \
 -p User=rustsimhub-core -p Group=rustsimhub-core -p SupplementaryGroups=rustsimhub-ipc \
 -p CapabilityBoundingSet= -p AmbientCapabilities= -p NoNewPrivileges=yes \
 -p ProtectSystem=strict -p ProtectHome=yes -p PrivateDevices=yes -p PrivateTmp=yes \
 -p "BindReadOnlyPaths=$slot:/opt/rustsimhub/selected" \
 -p "BindPaths=$base/state $base/identity $base/runtime" \
 -p "InaccessiblePaths=-$base/edge -$base/health" \
 -p "ReadWritePaths=$base/state $base/identity $base/runtime" \
 -p RuntimeMaxSec=240 -p TimeoutStopSec=6 -p MemoryMax=96M -p TasksMax=16 -p LimitCORE=0 \
 /usr/bin/env -i /opt/rustsimhub/selected/rustsimhub-core --serve-call-observing \
 "$base/runtime/core.sock" "$(id -u rustsimhub-edge)" "$base/state" "$base/identity"
n=0;while ! test -S "$base/runtime/core.sock";do
 n=$((n+1));test "$n" -lt 15;sleep 1
done
systemd-run --unit "$edge_unit" --collect \
 -p User=rustsimhub-edge -p Group=rustsimhub-edge -p SupplementaryGroups=rustsimhub-ipc \
 -p CapabilityBoundingSet= -p AmbientCapabilities= -p NoNewPrivileges=yes \
 -p ProtectSystem=strict -p ProtectHome=yes -p PrivateDevices=yes -p PrivateTmp=yes \
 -p "BindReadOnlyPaths=$slot:/opt/rustsimhub/selected $base/edge $base/runtime" \
 -p "InaccessiblePaths=/run/dbus -$base/state -$base/identity -$base/health" \
 -p RuntimeMaxSec=220 -p TimeoutStopSec=5 -p MemoryMax=64M -p TasksMax=8 -p LimitCORE=0 \
 /usr/bin/env -i /opt/rustsimhub/selected/rustsimhub-edge --serve "$base/edge/config.json"
started=no
trap - EXIT HUP INT TERM
printf '%s\n' 'Private signed application group started; no boot installation or network changes'
