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
slot=$(/usr/bin/rustsimhub-ab-select select "$ledger") || exit 1
case "$slot" in
    A) printf '%s\n' "$root_a" ;;
    B) printf '%s\n' "$root_b" ;;
    *) exit 1 ;;
esac
