#!/bin/sh
# Development application-slot acceptance only; no boot/rootfs writes, no network mutations.
# Caller sets existing root-owned PRIVATE_TRIAL_ROOT and a stopped private clone source.
exec 2>&1
set -eu
umask 077
base=${PRIVATE_TRIAL_ROOT:?private trial required}
source=${PRIVATE_FIXTURE_SOURCE:?stopped private fixture required}
case "$base" in /var/tmp/rustsimhub-app-trial-????????) ;; *) exit 10;; esac
case "$source" in /var/tmp/rustsimhub-carrier-rtc-????????) ;; *) exit 10;; esac
test "$(stat -c '%u:%a' "$base")" = 0:700
test "$(stat -c %u "$source")" = 0
test ! -S "$source/runtime/core.sock"
test ! -e "$base/store"
u=/opt/rustsimhub/dev-app-update-p6/rustsimhub-update
pub=$base/release.public
chmod 0600 "$pub" "$base/wrong.public" "$base/bad.sig"
for e in 1 2 3;do mkdir -m 0700 "$base/package$e";tar -C "$base/package$e" -xzf "$base/package$e.tar.gz";done
mkdir -m 0700 "$base/store"
cp -a "$source" "$base/data"
fixture=$base/data
socket=$fixture/runtime/core.sock
identity_before=$(sha256sum "$fixture/identity/identity.seed")
units='rustsimhub-app-factory-p6 rustsimhub-app-crash-p6 rustsimhub-app-fallback-p6 rustsimhub-app-confirm-p6'
for unit in $units;do if systemctl is-active --quiet "$unit";then exit 10;fi;done
started=''
cleanup(){ for unit in $started;do systemctl stop "$unit" >/dev/null 2>&1 || true;done; }
trap cleanup EXIT HUP INT TERM
core_start(){
 slot=$1;unit=$2
 systemd-run --unit "$unit" --collect -p User=root -p Group=root -p NoNewPrivileges=yes -p ProtectSystem=strict -p ProtectHome=yes -p PrivateTmp=yes -p PrivateDevices=yes -p InaccessiblePaths=/run/dbus -p "BindReadOnlyPaths=$base/store" -p "BindPaths=$fixture" -p "ReadWritePaths=$fixture" -p RuntimeMaxSec=45 -p TimeoutStopSec=5 -p MemoryMax=96M -p TasksMax=8 "$slot/rustsimhub-core" --serve-authenticated "$socket" 0 "$fixture/state" "$fixture/identity"
 started="$started $unit"
 n=0;while ! test -S "$socket";do n=$((n+1));test "$n" -lt 10;sleep 1;done
}
core_health(){
 slot=$1;label=$2
 "$slot/rustsimhub-core" --build-info | grep -q '"development_fixtures":false'
 "$slot/rustsimhub-core" --build-info | grep -q '"source_archive_sha256":"a78207e3f5eacc618a2e09ac1bc10cc7d70d9b752773a5bd5e44198203cbd56b"'
 pin=$(cat "$fixture/device.pubhex")
 "$slot/simhubctl" login "$socket" "$fixture/owner-key" "$pin" "$fixture/sessions/$label.json" > "$base/$label-login.json"
 "$slot/simhubctl" status "$socket" "$fixture/sessions/$label.json" > "$base/$label-health.json"
 grep -q '"ok":true' "$base/$label-health.json"
 "$slot/simhubctl" inbox "$socket" "$fixture/sessions/$label.json" > "$base/$label-inbox.json"
}
"$u" stage "$base/store" "$pub" "$base/package1"
factory=$("$u" choose "$base/store" "$pub")
case "$factory" in "$base"/store/versions/????????????????????????????????????????????????????????????????) ;; *) exit 10;; esac
core_start "$factory" rustsimhub-app-factory-p6
core_health "$factory" factory
systemctl stop rustsimhub-app-factory-p6
"$u" confirm "$base/store" "$pub" "${factory##*/}"
"$u" status "$base/store" "$pub" > "$base/factory-status.json"
# Invalid payload and signature must fail before any selector state changes.
cp -a "$base/package2" "$base/bad-payload"
printf 'X' >> "$base/bad-payload/Opus-COPYING.txt"
if "$u" stage "$base/store" "$pub" "$base/bad-payload" > "$base/bad-payload.log" 2>&1;then exit 10;fi
cp -a "$base/package2" "$base/bad-signature"
install -m 0600 "$base/bad.sig" "$base/bad-signature/manifest.sig"
if "$u" stage "$base/store" "$pub" "$base/bad-signature" > "$base/bad-signature.log" 2>&1;then exit 10;fi
"$u" status "$base/store" "$pub" > "$base/after-rejections.json"
cmp "$base/factory-status.json" "$base/after-rejections.json"
"$u" stage "$base/store" "$pub" "$base/package2"
candidate=$("$u" choose "$base/store" "$pub")
test "$candidate" != "$factory"
core_start "$candidate" rustsimhub-app-crash-p6
# A real candidate process crash before health confirmation; next choose must fall back.
systemctl kill --kill-whom=main --signal=KILL rustsimhub-app-crash-p6
sleep 1
if test -S "$socket";then test ! -L "$socket";test "$(stat -c %u "$socket")" = 0;rm -- "$socket";fi
recovered=$("$u" choose "$base/store" "$pub")
test "$recovered" = "$factory"
core_start "$recovered" rustsimhub-app-fallback-p6
core_health "$recovered" fallback
systemctl stop rustsimhub-app-fallback-p6
cmp "$base/factory-inbox.json" "$base/fallback-inbox.json"
if "$u" stage "$base/store" "$pub" "$base/package1" > "$base/rollback.log" 2>&1;then exit 10;fi
if "$u" status "$base/store" "$base/wrong.public" > "$base/wrong-key.log" 2>&1;then exit 10;fi
"$u" status "$base/store" "$pub" > "$base/recovered-status.json"
grep -q '"floor_epoch":2' "$base/recovered-status.json"
"$u" stage "$base/store" "$pub" "$base/package3"
third=$("$u" choose "$base/store" "$pub")
core_start "$third" rustsimhub-app-confirm-p6
core_health "$third" third
systemctl stop rustsimhub-app-confirm-p6
"$u" confirm "$base/store" "$pub" "${third##*/}"
test "$("$u" choose "$base/store" "$pub")" = "$third"
"$u" status "$base/store" "$pub" > "$base/final-status.json"
grep -q '"floor_epoch":3' "$base/final-status.json"
cmp "$base/factory-inbox.json" "$base/third-inbox.json"
identity_after=$(sha256sum "$fixture/identity/identity.seed")
test "$identity_after" = "$identity_before"
cleanup
for s in /proc/asound/card0/pcm*/sub0/status;do test "$(cat "$s")" = closed;done
for unit in nassimhub-agent ModemManager NetworkManager rmtfs;do systemctl is-active "$unit";done
printf '%s\n' '{"application_slots":true,"production_core":true,"candidate_crash_fallback":true,"payload_tamper_denied":true,"signature_tamper_denied":true,"rollback_denied":true,"key_change_denied":true,"identity_preserved":true,"inbox_preserved":true,"confirmed_epoch":3,"system_ota_supported":false,"firmware_partitions_written":false}'
