#!/bin/sh
set -eu
base=/opt/rustsimhub/dev-audio-p3
test -x "$base/rustsimhub-audio-alsa"
systemctl is-active nassimhub-agent ModemManager NetworkManager rmtfs
for path in /proc/asound/card0/pcm*/sub*/status; do test "$(cat "$path")" = closed; done
systemd-run --unit=rustsimhub-audio-probe-p3 --collect --wait --pipe \
 -p User=nassimhub -p Group=nassimhub -p SupplementaryGroups=audio \
 -p NoNewPrivileges=yes -p ProtectSystem=strict -p ProtectHome=yes -p PrivateTmp=yes \
 -p PrivateDevices=no -p DevicePolicy=closed -p 'DeviceAllow=/dev/snd/controlC0 rw' \
 -p RuntimeMaxSec=15 -p TimeoutStopSec=3 -p MemoryMax=32M -p TasksMax=8 \
 "$base/rustsimhub-audio-alsa" --probe-readonly
for path in /proc/asound/card0/pcm*/sub*/status; do test "$(cat "$path")" = closed; done
systemctl is-active nassimhub-agent ModemManager NetworkManager rmtfs
echo 'PCM nodes remained closed; old services active'
