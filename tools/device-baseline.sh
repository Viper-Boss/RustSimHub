#!/bin/sh
set -u
printf 'ARCH='; uname -m
printf 'KERNEL='; uname -r
printf '\nOS\n'; cat /etc/os-release
printf '\nBOARD\n'; tr '\000' '\n' </proc/device-tree/compatible; cat /proc/device-tree/model
printf '\nSTORAGE\n'; lsblk -o NAME,SIZE,FSTYPE,MOUNTPOINTS; df -h / /var /tmp
printf '\nPARTITION NAMES\n'; ls /dev/disk/by-partlabel 2>/dev/null || true
printf '\nMEMORY\n'; head -n 5 /proc/meminfo
printf '\nSERVICES\n'; systemctl is-active ModemManager NetworkManager rmtfs nassimhub-agent airsimhub-agent || true
printf '\nTOOLS\n'; for t in mmcli busctl nmcli aplay gcc rustc cargo; do command -v "$t" || true; done
printf '\nMODEM VERSION\n'; mmcli --version
printf '\nMODEM COUNT\n'; busctl get-property org.freedesktop.ModemManager1 /org/freedesktop/ModemManager1 org.freedesktop.DBus.ObjectManager ManagedObjects 2>/dev/null || true
printf '\nALSA PLAYBACK\n'; aplay -l 2>/dev/null || true
printf '\nALSA CAPTURE\n'; arecord -l 2>/dev/null || true
printf '\nNETWORK TYPES\n'; nmcli -t -f TYPE,STATE device status
printf '\nBOOT CONFIG FILE NAMES\n'; find /boot -maxdepth 2 -type f 2>/dev/null | head -n 30
