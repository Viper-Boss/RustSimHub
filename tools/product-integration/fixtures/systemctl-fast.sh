#!/bin/busybox sh
# SPDX-License-Identifier: AGPL-3.0-only
# ARM guest fixture only: same inactive PID1 facts without launching Python for
# every readonly show. Read tracked PIDs and actual /proc liveness; mutations
# use the original controller, which creates real processes and guest cgroups.
set -eu
show=no
last=
for arg do
 [ "$arg" != show ] || show=yes
 last=$arg
done
if [ "$show" != yes ]; then
 exec /usr/bin/python3 /fixture/systemctl-python.py "$@"
fi
case "$last" in
 rustsimhub-core.service|rustsimhub-edge.service|rustsimhub-media.service|rustsimhub-forwarder.service|rustsimhub-nkn-root.service|rustsimhub-ble-recovery.service|rustsimhub-forwarder.socket|rustsimhub-forwarder.timer|rustsimhub-forwarder.path) ;;
 *) exit 2;;
esac
unit=$last
printf 'readonly-show %s\n' "$unit" >> /fixture/systemctl-fast-trace.log
pid=0
if [ -f /fixture/pids.json ]; then
 # This private file is emitted only by the fixed Python controller. Read the
 # fixed unit's decimal PID; no supplied command or shell content is executed.
 found=$(/bin/busybox sed -n 's/.*"'"$unit"'": \([0-9][0-9]*\).*/\1/p' /fixture/pids.json)
 [ -z "$found" ] || pid=$found
fi
active=no
if [ "$pid" != 0 ] && [ -r "/proc/$pid/stat" ]; then
 stat=$(/bin/busybox cat "/proc/$pid/stat")
 state=${stat##*) }; state=${state%% *}
 [ "$state" = Z ] || active=yes
fi
if [ "$active" = yes ]; then
 activity=active; sub=running; group=/system.slice/$unit
 invocation=$(printf '%032x' "$pid")
else
 pid=0; activity=inactive; sub=dead; group=; invocation=
fi
role=${unit#rustsimhub-}; role=${role%.service}
drops=
for file in /etc/systemd/system/rustsimhub-core.service.d/*.conf; do
 [ -f "$file" ] || continue
 if [ -n "$drops" ]; then drops="$drops $file"; else drops=$file; fi
done
emit() {
 case "$1" in
  Id) value=$unit;; LoadState) value=loaded;; ActiveState) value=$activity;;
  SubState) value=$sub;; MainPID) value=$pid;; ControlPID|Job) value=0;;
  User) value=rustsimhub-$role;; NeedDaemonReload|Delegate|Restart) value=no;;
  ControlGroup) value=$group;; InvocationID) value=$invocation;;
  TriggeredBy|Triggers) value=;;
  UnitFileState) value=static;; KillMode) value=control-group;;
  FragmentPath) value=/etc/systemd/system/$unit;; DropInPaths) value=$drops;;
  *) exit 2;;
 esac
 printf '%s=%s\n' "$1" "$value"
}
next=no
for arg do
 if [ "$next" = yes ]; then emit "$arg"; next=no; continue; fi
 case "$arg" in
  -p) next=yes;;
  --property=*)
   properties=${arg#--property=}; saved=$IFS; IFS=,
   for property in $properties; do emit "$property"; done
   IFS=$saved;;
 esac
done
