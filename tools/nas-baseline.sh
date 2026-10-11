#!/bin/sh
# Read-only discovery; omit SIM IDs, modem paths with identifiers, and application secrets.
set -u
printf 'ARCH='; uname -m
printf 'KERNEL='; uname -r
printf '\nTOOLS\n'
for tool in docker rustc cargo adb lsusb ssh gh; do command -v "$tool" || true; done
printf '\nUSB\n'; lsusb 2>/dev/null || true
printf '\nDEVICE ROUTE\n'; ip route get 10.55.0.2 2>/dev/null || true
printf '\nCONTAINERS\n'; docker ps --format '{{.Names}} {{.Image}}' 2>/dev/null || true
printf '\nSSH DEVICE KEY STATUS\n'
ssh-keygen -F 10.55.0.2 >/dev/null 2>&1 && echo 'device key known' || true
printf '\nGITHUB CLI ACCOUNT\n'; gh api user --jq .login 2>/dev/null || true
printf '\nDEVICE KEY AUTH CHECK\n'
timeout 15 ssh -o BatchMode=yes -o ConnectTimeout=8 -o StrictHostKeyChecking=yes root@10.55.0.2 'uname -m; uname -r' 2>/dev/null || true
