#!/bin/sh
# Recovery entry exercise only. Never accepts an image or a flash command.
# Run on the USB host, with a private output directory and known physical port.
set -eu
test "$#" = 2
out=$1
port=$2
case "$out" in /mnt/docker-data/rustsimhub/p0/recovery-entry-*) ;; *) exit 2;; esac
case "$port" in /sys/bus/usb/devices/[0-9]*-[0-9]*) ;; *) exit 2;; esac
test -d "$out" && test ! -L "$out"
test "$(stat -c %a "$out")" = 700
test ! -e "$out/started"
umask 077
exec >"$out/fastboot.log" 2>&1
command -v timeout
command -v fastboot
test -z "$(timeout 5 fastboot devices)"
serial=
returned=0
cleanup() {
 if test -n "$serial" && test "$returned" = 0; then
  timeout 15 fastboot -s "$serial" reboot || true
 fi
}
trap cleanup EXIT
trap 'exit 130' HUP INT TERM
date -u +%FT%TZ > "$out/started"
seen=0
for attempt in $(seq 1 120); do
 timeout 3 fastboot devices > "$out/devices.tmp"
 if test -s "$out/devices.tmp"; then seen=1; break; fi
 sleep 1
done
test "$seen" = 1
test "$(wc -l < "$out/devices.tmp")" = 1
read -r found mode extra < "$out/devices.tmp"
test "$mode" = fastboot && test -z "${extra:-}"
case "$found" in ''|*[!a-zA-Z0-9._-]*) exit 3;; esac
# Tie the sole fastboot target to the device on the observed NAS USB port.
test -r "$port/serial"
test "$(cat "$port/serial")" = "$found"
serial=$found
timeout 10 fastboot -s "$serial" getvar product
timeout 10 fastboot -s "$serial" getvar max-download-size
for name in boot rootfs NSHSTATE; do
 # Some bootloaders don't implement these. Preserve failure; never infer size.
 timeout 10 fastboot -s "$serial" getvar "partition-size:$name" || true
done
date -u +%FT%TZ > "$out/queried"
timeout 15 fastboot -s "$serial" reboot
returned=1
date -u +%FT%TZ > "$out/reboot-ack"
