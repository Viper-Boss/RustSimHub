#!/bin/sh
# SPDX-License-Identifier: AGPL-3.0-only
set -eu

device_for_label() {
    found=
    for device in /sys/class/block/mmcblk0/mmcblk0p*; do
        if [ -r "$device/uevent" ] && grep -qx "PARTNAME=$1" "$device/uevent"; then
            [ -z "$found" ] || return 1
            found="/dev/${device##*/}"
        fi
    done
    [ -n "$found" ] && [ -b "$found" ] || return 1
    printf '%s\n' "$found"
}

remaining=12
while :; do
    if ledger=$(device_for_label RSH_BOOTCTL) &&
        root_a=$(device_for_label RSH_A) &&
        root_b=$(device_for_label RSH_B); then
        break
    fi
    [ "$remaining" -gt 0 ] || exit 1
    remaining=$((remaining - 1))
    sleep 1
done
session=$(/usr/bin/rustsimhub-ab-select select-session "$ledger") || exit 1
case "$session" in
    'A '*) root=$root_a ;;
    'B '*) root=$root_b ;;
    *) exit 1 ;;
esac
# This tmpfs is moved to the mounted root by initramfs-tools.
umask 077
printf '%s\n' "$session" > /run/initramfs/rustsimhub-ab-boot
printf '%s\n' "$root"
