#!/bin/sh
# SPDX-License-Identifier: AGPL-3.0-only
# Manual, fixed first-install preparation. No service/network activation.
# Caller verifies THIS helper before execution, then supplies the trusted sums
# digest. External AP signature verification happens on the maintenance host.
set -eu
umask 077
PATH=/usr/sbin:/usr/bin:/sbin:/bin
export PATH
test "$#" = 3
bundle=$1; action=$2; expected=$3
case "$action" in --check|--apply|--rollback) ;; *) exit 2;; esac
test "$(id -u)" = 0
printf '%s\n' "$expected" | grep -Eq '^[0-9a-f]{64}$'
test ! -L "$bundle"
test "$(stat -c '%u:%g:%a' "$bundle")" = 0:0:700
cd "$bundle"
digest() { sha256sum "$1" | cut -d ' ' -f 1; }
test "$(digest SHA256SUMS)" = "$expected"
names='payload-[0-7]|application-manifest.sha256|application-ledger.sha256|core-unit.sha256|forwarding-unit.sha256|passwd.sha256|group.sha256|edge-config.sha256|identity.sha256|setup-used.sha256|core.sha256|edge.sha256|setup-code.sha256|boot-id|modem-path|business-before.tar|signed-manifest.json|signed-manifest.sig|signer.public|apply.sh|plan.json'
# Include dotfiles in the rejection set, including dangling links.
for file in ./* ./.[!.]* ./..?*;do
 test -e "$file" || test -L "$file" || continue
 printf '%s\n' "${file#./}" | grep -Eq "^($names|SHA256SUMS|operation.log)$"
 test -f "$file"; test ! -L "$file"
 test "$(stat -c '%u:%g:%a:%h' "$file")" = 0:0:600:1
done
test "$(wc -l < SHA256SUMS)" = 28
test "$(cut -c 67- SHA256SUMS | sort -u | wc -l)" = 28
test "$(grep -Ec "^[0-9a-f]{64}  ($names)$" SHA256SUMS)" = 28
sha256sum -c SHA256SUMS >/dev/null
# Do not allow a different script with the same unchecked command line.
test "$(digest "$0")" = "$(digest apply.sh)"
core=/var/lib/rustsimhub/rustsimhub-v1/core
persistent=/var/lib/rustsimhub/rustsimhub-v1/system/setup-ap
edge=/var/lib/rustsimhub/rustsimhub-v1/setup-edge
selected=/run/rustsimhub-app/selected
absent() { test ! -e "$1" && test ! -L "$1"; }
bound() {
 test -f "$1"; test ! -L "$1"
 test "$(stat -c %h "$1")" = 1
 test "$(digest "$1")" = "$(cat "$2")"
}
snapshot() {
 bound "$core/identity/identity.seed" identity.sha256
 bound "$core/setup/setup.used" setup-used.sha256
 if test "$(cat setup-code.sha256)" = absent;then absent "$core/setup/setup.code"
 else bound "$core/setup/setup.code" setup-code.sha256;fi
}
test "$(cat boot-id)" = "$(cat /proc/sys/kernel/random/boot_id)"
bound "$selected/manifest.json" application-manifest.sha256
bound /var/lib/rustsimhub/rustsimhub-v1/system/app/state.json application-ledger.sha256
bound "$selected/rustsimhub-core" core.sha256
bound "$selected/rustsimhub-edge" edge.sha256
bound /etc/systemd/system/rustsimhub-core.service core-unit.sha256
bound /etc/systemd/system/rustsimhub-core.service.d/30-forwarding.conf forwarding-unit.sha256
bound /etc/passwd passwd.sha256; bound /etc/group group.sha256
bound /var/lib/rustsimhub/rustsimhub-v1/edge/installation/config.json edge-config.sha256
snapshot
absent /run/rustsimhub-app/recovery.used
for pair in core:986 edge:985 media:984 forwarder:983;do
 name=rustsimhub-${pair%:*}; uid=${pair#*:}
 test "$(id -u "$name")" = "$uid"; test "$(id -g "$name")" = "$uid"
 test "$(getent passwd "$uid" | cut -d: -f1)" = "$name"
 test "$(getent group "$uid" | cut -d: -f1)" = "$name"
 for gid in $(id -G "$name");do
  if test "$name:$gid" = rustsimhub-media:29;then
   test "$(getent group 29 | cut -d: -f1)" = audio
   test "$(getent group audio | cut -d: -f3)" = 29
  else test "$gid" = "$uid" || test "$gid" = 987;fi
 done
done
test "$(getent group 987 | cut -d: -f1)" = rustsimhub-ipc
for unit in rustsimhub-setup-ap.service rustsimhub-setup-edge.service;do
 case "$(systemctl show --value -p ActiveState "$unit")" in inactive|failed) ;; *) exit 1;; esac
 for property in MainPID ControlPID Job;do
  value=$(systemctl show --value -p "$property" "$unit");test -z "$value" || test "$value" = 0
 done
done
test "$(timeout 5 nmcli -g GENERAL.STATE device show wwan0qmi0 | cut -d ' ' -f1)" = 30
modem=$(cat modem-path)
printf '%s\n' "$modem" | grep -Eq '^/org/freedesktop/ModemManager1/Modem/[0-9]{1,10}$'
# Parse data, never eval it or use shell expansion supplied by the modem.
calls=$(busctl --timeout=3 get-property org.freedesktop.ModemManager1 "$modem" org.freedesktop.ModemManager1.Modem.Voice Calls)
printf '%s\n' "$calls" | awk 'NF>=2 && $1=="ao" && $2~/^[0-9]+$/ && $2<=64 && NF==$2+2 { for(i=3;i<=NF;i++) if($i!~/^"\/org\/freedesktop\/ModemManager1\/Call\/[0-9]+"$/) exit 1; ok=1 } END {if(!ok)exit 1}'
for call in $(printf '%s\n' "$calls" | cut -d ' ' -f3-);do
 call=$(printf '%s' "$call" | tr -d '"')
 test "$(busctl --timeout=3 get-property org.freedesktop.ModemManager1 "$call" org.freedesktop.ModemManager1.Call State)" = 'i 7'
done
absent "$persistent/journal/active.json"
for file in pending.json first-setup.pending.json station.pending.json commit.pending.json;do absent "$core/setup-ap-journal/$file";done
for directory in /run/rustsimhub-setup-ap-core /run/rustsimhub-setup-ap-root;do
 if test -d "$directory";then test -z "$(find "$directory" -mindepth 1 -maxdepth 1 -print -quit)";fi
done
mapping() {
 cat <<'FILES'
payload-0 755 /usr/libexec/rustsimhub/rustsimhub-setup-ap
payload-1 644 /etc/systemd/system/rustsimhub-setup-ap.service
payload-2 644 /etc/systemd/system/rustsimhub-setup-edge.service
payload-3 644 /etc/tmpfiles.d/rustsimhub-setup-ap.conf
payload-6 644 /etc/rustsimhub/setup-ap-contract.json
payload-7 755 /usr/libexec/rustsimhub/rustsimhub-core-launcher
payload-4 644 /etc/systemd/system/rustsimhub-core.service.d/50-setup-ap.conf
payload-5 644 /etc/systemd/system/rustsimhub-app.target.d/50-setup-ap.conf
FILES
}
directories() {
 printf '%s\n' "0 0 700 $persistent" "985 985 700 $edge" "986 986 700 $core/setup-ap-journal" "0 0 700 $persistent/journal" '986 0 2750 /run/rustsimhub-setup-ap-core' '0 986 2750 /run/rustsimhub-setup-ap-root'
}
safe_parent() (
 parent=$(dirname "$1")
 while test "$parent" != /;do
  test ! -L "$parent"
  if test -e "$parent";then
   test -d "$parent"; owner=$(stat -c %u "$parent")
   case "$parent" in "$core"|"$core"/*) test "$owner" = 0 || test "$owner" = 986;; *) test "$owner" = 0;; esac
   mode=$(stat -c %a "$parent");test "$((0$mode & 022))" = 0
  fi
  parent=$(dirname "$parent")
 done
)
exact() {
 test -f "$3";test ! -L "$3"
 test "$(stat -c '%u:%g:%a:%h' "$3")" = "0:0:$2:1"
 test "$(digest "$3")" = "$(digest "$1")"
}
# Before the lock, --check is completely read-only. Mutating actions acquire a
# single Root lock, then repeat all file checks below under that lock.
if test "$action" != --check;then
 safe_parent /run/rustsimhub-setup-ap-install.lock
 absent /run/rustsimhub-setup-ap-install.lock || { test ! -L /run/rustsimhub-setup-ap-install.lock;test "$(stat -c '%u:%g:%a:%h' /run/rustsimhub-setup-ap-install.lock)" = 0:0:600:1; }
 exec 9>/run/rustsimhub-setup-ap-install.lock
 flock -n 9
 for unit in rustsimhub-app.target rustsimhub-app-select.service rustsimhub-core.service rustsimhub-edge.service rustsimhub-media.service rustsimhub-forwarder.service rustsimhub-forwarder-reconcile.service rustsimhub-forwarder-status.service rustsimhub-forwarder-boot.service;do
  case "$(systemctl show --value -p ActiveState "$unit")" in inactive|failed) ;; *) exit 1;; esac
  for property in MainPID ControlPID Job;do
   value=$(systemctl show --value -p "$property" "$unit");test -z "$value" || test "$value" = 0
  done
 done
 ps -eo uid= | awk '$1>=983 && $1<=986 {bad=1} END {exit bad?1:0}'
fi
resume=false
if test -e operation.log;then
 test "$(head -n 1 operation.log)" = begin-first-setup-ap-install
 test "$(stat -c %s operation.log)" -le 32768
 resume=true
fi
mapping | while read -r name mode target;do
 safe_parent "$target"
 for path in "$target" "$target.rsh-ap-new";do
  if ! absent "$path";then test "$resume" = true;exact "$name" "$mode" "$path";fi
 done
done
directories | while read -r uid gid mode target;do
 safe_parent "$target"
 if ! absent "$target";then
  test "$resume" = true;test -d "$target";test ! -L "$target"
  test "$(stat -c '%u:%g:%a' "$target")" = "$uid:$gid:$mode"
 fi
done
if test "$action" = --check;then
 printf '{"preflight":true,"device_changed":false,"services_started":false}\n';exit 0
fi
if test "$action" = --rollback;then
 test "$resume" = true
 # All eight destinations, staging files and journals were checked first.
 mapping | while read -r name mode target;do
  rm -f -- "$target" "$target.rsh-ap-new"
  parent=$(dirname "$target");while test ! -d "$parent";do parent=$(dirname "$parent");done
  sync -f "$parent"
 done
 printf 'rolled-back-files-materials-retained\n' >> operation.log;sync -f operation.log
 printf '{"files_removed":true,"materials_retained":true,"business_data_restored":false,"services_started":false}\n';exit 0
fi
if test "$resume" = false;then printf 'begin-first-setup-ap-install\n' > operation.log;sync -f operation.log;fi
directories | while read -r uid gid mode target;do
 if absent "$target";then install -d -o "$uid" -g "$gid" -m "$mode" "$target";sync -f "$(dirname "$target")";fi
done
put() {
 name=$1;mode=$2;target=$3
 if absent "$target";then
  if absent "$target.rsh-ap-new";then install -D -o 0 -g 0 -m "$mode" "$name" "$target.rsh-ap-new";fi
  exact "$name" "$mode" "$target.rsh-ap-new";sync -f "$target.rsh-ap-new"
  mv -T "$target.rsh-ap-new" "$target";sync -f "$(dirname "$target")"
 fi
}
mapping | head -n 6 | while read -r name mode target;do put "$name" "$mode" "$target";done
# Fixed offline CLI arguments only. Do not reopen setup, export credentials or
# restore databases. Activation drop-ins are published only after preparation.
prepare() {
 case "$1" in
  core) user=rustsimhub-core; source=$core; alias=/var/lib/rustsimhub-core; program=rustsimhub-core
   set -- --prepare-device-ap-binding "$alias/state" "$alias/identity" "$alias/setup";;
  edge) user=rustsimhub-edge; source=$edge; alias=/var/lib/rustsimhub-setup-edge; program=rustsimhub-edge
   set -- --prepare-setup-local "$alias";;
  *) return 1;;
 esac
 # The persistent state and version-store parents are deliberately Root-only.
 # PID1 binds just the selected program and this role's directory before
 # dropping privileges, exactly as the normal service does. Never chmod the
 # parents or run preparation as Root. Lost callers cannot leave a lasting job.
 timeout 20 systemd-run --quiet --wait --pipe --collect \
  --property=Type=exec --property="User=$user" --property="Group=$user" \
  --property="BindReadOnlyPaths=$selected:/opt/rustsimhub/selected" \
  --property="BindPaths=$source:$alias" --property="ReadWritePaths=$alias" \
  --property=ProtectSystem=strict --property=ProtectHome=yes \
  --property=InaccessiblePaths=/var/lib/rustsimhub \
  --property=PrivateTmp=yes --property=PrivateDevices=yes --property=PrivateNetwork=yes \
  --property=NoNewPrivileges=yes --property=CapabilityBoundingSet= \
  --property=RestrictAddressFamilies=AF_UNIX --property=UMask=0077 \
  --property=RuntimeMaxSec=15 --property=TimeoutStopSec=3 \
  --property=MemoryMax=96M --property=TasksMax=8 \
  /usr/bin/env -i "/opt/rustsimhub/selected/$program" "$@" >/dev/null
}
prepare core
prepare edge
timeout 20 /usr/libexec/rustsimhub/rustsimhub-setup-ap --prepare-from-core "$persistent" "$core/setup" "$edge/installation" 985 986 >/dev/null
snapshot
mapping | tail -n 2 | while read -r name mode target;do put "$name" "$mode" "$target";done
printf 'prepared-services-not-started\n' >> operation.log;sync -f operation.log
printf '{"prepared":true,"services_started":false,"preparation_jobs_started":true,"network_changed":false,"business_data_restored":false}\n'
