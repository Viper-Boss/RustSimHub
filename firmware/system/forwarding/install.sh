#!/bin/sh
# SPDX-License-Identifier: AGPL-3.0-only
# Finite first-install helper. Operator invokes it from a checked Root bundle.
# Never starts a service, enables forwarding, restores data or repairs accounts.
set -eu
umask 077
PATH=/usr/sbin:/usr/bin:/sbin:/bin
export PATH
test "$#" = 2
bundle=$1
action=$2
case "$action" in --check|--apply|--rollback) ;; *) exit 2;; esac
test "$(id -u)" = 0
test ! -L "$bundle"
test "$(stat -c '%u:%g:%a' "$bundle")" = 0:0:700
if test "$action" != --check;then
 test ! -L /run/lock/rustsimhub-forwarding-install.lock
 exec 9>/run/lock/rustsimhub-forwarding-install.lock
 flock -n 9
fi
cd "$bundle"
for file in ./*;do
 case "${file#./}" in payload-[0-8]|ledger.sha256|passwd.sha256|group.sha256|core-unit.sha256|confirmed.sha256|boot-id|business-before.tar|apply.sh|plan.json|SHA256SUMS|operation.log) ;; *) exit 1;; esac
 test -f "$file"
 test ! -L "$file"
 test "$(stat -c '%u:%g:%a:%h' "$file")" = 0:0:600:1
done
test "$(wc -l < SHA256SUMS)" = 18
test "$(cut -c 67- SHA256SUMS | sort -u | wc -l)" = 18
test "$(grep -Ec '^[0-9a-f]{64}  (payload-[0-8]|ledger.sha256|passwd.sha256|group.sha256|core-unit.sha256|confirmed.sha256|boot-id|business-before.tar|apply.sh|plan.json)$' SHA256SUMS)" = 18
sha256sum -c SHA256SUMS >/dev/null
test "$(cat boot-id)" = "$(cat /proc/sys/kernel/random/boot_id)"
digest() { sha256sum "$1" | cut -d ' ' -f 1; }
test "$(digest /run/rustsimhub-app/selected/manifest.json)" = "$(cat confirmed.sha256)"
test "$(digest /var/lib/rustsimhub/rustsimhub-v1/system/app/state.json)" = "$(cat ledger.sha256)"
test "$(digest /etc/systemd/system/rustsimhub-core.service)" = "$(cat core-unit.sha256)"
test ! -e /run/rustsimhub-app/recovery.used
ps -eo uid= | awk '$1 == 983 { bad=1 } END { exit bad ? 1 : 0 }'

mapping() {
 cat <<'FILES'
payload-0 644 /etc/systemd/system/rustsimhub-forwarder.service
payload-1 644 /etc/systemd/system/rustsimhub-forwarder-reconcile.service
payload-2 644 /etc/systemd/system/rustsimhub-forwarder-status.service
payload-3 644 /etc/systemd/system/rustsimhub-forwarder-boot.service
payload-4 644 /etc/polkit-1/rules.d/46-rustsimhub-forwarding.rules
payload-5 600 /etc/rustsimhub/forwarder.json
payload-6 644 /etc/tmpfiles.d/rustsimhub-forwarding.conf
payload-7 644 /etc/systemd/system/rustsimhub-app.target.d/20-forwarding.conf
payload-8 644 /etc/systemd/system/rustsimhub-core.service.d/30-forwarding.conf
FILES
}
safe_parent() (
 parent=$(dirname "$1")
 while test "$parent" != /;do
  test ! -L "$parent"
  if test -e "$parent";then
   test -d "$parent"
   test "$(stat -c %u "$parent")" = 0
   mode=$(stat -c %a "$parent")
   test "$((0$mode & 022))" = 0
  fi
  parent=$(dirname "$parent")
 done
)
if test "$action" = --rollback;then
 for unit in rustsimhub-app.target rustsimhub-core rustsimhub-edge rustsimhub-media rustsimhub-forwarder rustsimhub-forwarder-reconcile rustsimhub-forwarder-status rustsimhub-forwarder-boot;do
  main=$(systemctl show --value -p MainPID "$unit")
  test -z "$main" || test "$main" = 0
  case "$(systemctl show --value -p ActiveState "$unit")" in inactive|failed) ;; *) exit 1;; esac
 done
 test ! -e /var/lib/rustsimhub/rustsimhub-v1/system/forwarding/enabled.json
 test ! -e /run/rustsimhub-forwarding/settings.json
 # Check ALL targets before removing ANY. A changed file needs human review.
 mapping | while read -r name mode target;do
  safe_parent "$target"
  test ! -L "$target"
  if test -e "$target";then
   test "$(stat -c '%u:%g:%a:%h' "$target")" = "0:0:$mode:1"
   test "$(digest "$target")" = "$(digest "$name")"
  fi
 done
 mapping | while read -r name mode target;do rm -f -- "$target";done
 printf 'rolled-back-files-account-and-business-data-retained\n' >> operation.log
 sync -f operation.log
 printf '{"files_removed":true,"accounts_retained":true,"business_data_restored":false,"services_started":false}\n'
 exit 0
fi

test "$(digest /etc/passwd)" = "$(cat passwd.sha256)"
test "$(digest /etc/group)" = "$(cat group.sha256)"
for name in 983 rustsimhub-forwarder;do
 if getent passwd "$name" >/dev/null;then exit 1;else test "$?" = 2;fi
 if getent group "$name" >/dev/null;then exit 1;else test "$?" = 2;fi
done
for pair in rustsimhub-core:986 rustsimhub-edge:985 rustsimhub-media:984;do
 name=${pair%:*};expected=${pair#*:}
 test "$(id -u "$name")" = "$expected"
 test "$(id -g "$name")" = "$expected"
done
test "$(getent group rustsimhub-ipc | cut -d: -f3)" = 987
mapping | while read -r name mode target;do
 safe_parent "$target"
 test ! -e "$target"
 test ! -L "$target"
 test ! -e "$target.rsh-forwarding-new"
 test ! -L "$target.rsh-forwarding-new"
done
for directory in /run/rustsimhub-forwarding /var/lib/rustsimhub/rustsimhub-v1/system/forwarding;do
 safe_parent "$directory"
 test ! -e "$directory"
 test ! -L "$directory"
done
if test "$action" = --check;then
 printf '{"preflight":true,"device_changed":false,"default_enabled":false}\n'
 exit 0
fi
for unit in rustsimhub-app.target rustsimhub-app-select rustsimhub-core rustsimhub-edge rustsimhub-media;do
 test "$(systemctl show --value -p ActiveState "$unit")" = inactive
done
# Account creation is deliberately after all conflict checks. A partial failure
# leaves a locked unused identity; never delete/reassign it automatically.
printf 'begin-first-install\n' >> operation.log
sync -f operation.log
groupadd --system --gid 983 rustsimhub-forwarder
useradd --system --uid 983 --gid 983 --groups rustsimhub-ipc --home-dir /nonexistent --no-create-home --shell /usr/sbin/nologin --no-log-init rustsimhub-forwarder
test "$(id -u rustsimhub-forwarder)" = 983
test "$(id -g rustsimhub-forwarder)" = 983
test "$(id -G rustsimhub-forwarder)" = '983 987'
test "$(passwd -S rustsimhub-forwarder | cut -d ' ' -f 2)" = L
printf 'locked-account-created\n' >> operation.log
install -d -o 0 -g 0 -m 0750 /run/rustsimhub-forwarding
chown 0:986 /run/rustsimhub-forwarding
chmod 2750 /run/rustsimhub-forwarding
install -d -o 0 -g 0 -m 0700 /var/lib/rustsimhub/rustsimhub-v1/system/forwarding
mapping | while read -r name mode target;do
 install -D -o 0 -g 0 -m "$mode" "$name" "$target.rsh-forwarding-new"
 sync -f "$target.rsh-forwarding-new"
 mv -T "$target.rsh-forwarding-new" "$target"
 sync -f "$(dirname "$target")"
 printf 'installed %s\n' "$target" >> operation.log
done
sync
test ! -e /var/lib/rustsimhub/rustsimhub-v1/system/forwarding/enabled.json
test ! -e /run/rustsimhub-forwarding/settings.json
printf 'prepared-services-not-started\n' >> operation.log
sync -f operation.log
printf '{"prepared":true,"default_enabled":false,"services_started":false,"business_data_restored":false}\n'
