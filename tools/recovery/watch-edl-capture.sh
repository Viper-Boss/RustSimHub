#!/bin/sh
# SPDX-License-Identifier: AGPL-3.0-only
# Operator-owned, fixed read-only capture and one return attempt on the USB host.
set -eu
test "$#" = 2
tools=$1
out=$2
case "$tools" in /mnt/docker-data/rustsimhub/p0/recovery-tools-????????) ;; *) exit 2;; esac
case "$out" in /mnt/docker-data/rustsimhub/p0/recovery-edl-????????) ;; *) exit 2;; esac
for directory in "$tools" "$out";do
 test "$(realpath -e "$directory")" = "$directory"
 test "$(stat -c '%u:%g:%a' "$directory")" = 0:0:700
done
umask 077
test ! -e "$out/started"
exec >"$out/watcher.log" 2>&1
date -u +%FT%TZ > "$out/started"
printf '%s  %s\n' 4a83be46cc8008e9da3cd872161a2cd3614cd64ecfa6e277de13e32f7f0eff93 "$tools/edl_readonly.py" | sha256sum -c -
test "$(stat -c '%u:%g:%a:%h' "$tools/expected-device.json")" = 0:0:600:1
"$tools/venv/bin/python" - "$tools/expected-device.json" > "$out/expected" <<'PY'
import json,re,sys
d=json.load(open(sys.argv[1]));assert re.fullmatch('[0-9]+-[0-9]+(?:\.[0-9]+)*',d['usb_port'])
assert re.fullmatch('[0-9a-f]{64}',d['gpt_sha256'])
print(d['usb_port'],d['gpt_sha256'])
PY
read -r port gpt < "$out/expected"
status() { printf '%s\n' "$1" > "$out/phase"; }
normal_return() {
 test -e "$out/edl-seen" || return 0
 test ! -e "$out/reset" || return 0
 mkdir -m 0700 "$out/reset"
 if "$tools/venv/bin/python" "$tools/edl_readonly.py" --tools "$tools" --output "$out/reset" --action reset --usb-port "$port" > "$out/reset.log" 2>&1;then
  date -u +%FT%TZ > "$out/reset-ack"
 else
  printf 'One return attempt lacked acknowledgement; inspect current mode, do not automatically retry\n' > "$out/return-unknown"
 fi
}
finish() {
 code=$?
 if test "$code" != 0;then status failed;fi
 normal_return
 printf '%s\n' "$code" > "$out/exit"
}
trap finish EXIT
trap 'exit 130' HUP INT TERM
status waiting-for-physical-entry
seen=0
for attempt in $(seq 1 600);do
 if test -r "/sys/bus/usb/devices/$port/idVendor" && test -r "/sys/bus/usb/devices/$port/idProduct" &&
    test "$(cat "/sys/bus/usb/devices/$port/idVendor")" = 05c6 && test "$(cat "/sys/bus/usb/devices/$port/idProduct")" = 9008;then seen=1;break;fi
 sleep 1
done
test "$seen" = 1
date -u +%FT%TZ > "$out/edl-seen"
status reading-gpt
mkdir -m 0700 "$out/gpt"
"$tools/venv/bin/python" "$tools/edl_readonly.py" --tools "$tools" --output "$out/gpt" --action gpt --usb-port "$port" --expected-gpt-sha256 "$gpt" > "$out/gpt.log" 2>&1
status reading-offline-user-area
mkdir -m 0700 "$out/capture"
"$tools/venv/bin/python" "$tools/edl_readonly.py" --tools "$tools" --output "$out/capture" --action read-emmc --usb-port "$port" --expected-gpt-sha256 "$gpt" > "$out/capture.log" 2>&1
status returning-to-normal-system
normal_return
test -e "$out/reset-ack"
status capture-finished-return-acknowledged
