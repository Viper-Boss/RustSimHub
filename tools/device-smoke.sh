#!/bin/sh
# Deploy a bounded transient read-only service. Existing services stay running.
set -eu
upload=/var/tmp/rustsimhub-core-p0
binary=/opt/rustsimhub/dev-p0/rustsimhub-core
test -f "$upload"
install -d -m 0755 /opt/rustsimhub/dev-p0
install -m 0755 "$upload" "$binary"
"$binary" --self-check
printf '\nROOT PROBE\n'; "$binary" --probe
printf '\nUNPRIVILEGED PROBE\n'; runuser -u nassimhub -- "$binary" --probe
uid=$(id -u nassimhub)
if systemctl is-active --quiet rustsimhub-probe-p0; then
    echo 'Smoke service already running; inspect before retrying' >&2
    exit 1
fi
systemctl reset-failed rustsimhub-probe-p0 2>/dev/null || true
trap 'systemctl stop rustsimhub-probe-p0 2>/dev/null || true' EXIT
systemd-run --unit=rustsimhub-probe-p0 --property=User=nassimhub \
    --property=RuntimeDirectory=rustsimhub-p0 --property=RuntimeDirectoryMode=0700 \
    --property=RuntimeMaxSec=300 --property=NoNewPrivileges=yes --property=ProtectSystem=strict \
    --property=ProtectHome=yes --property=PrivateTmp=yes --property=PrivateNetwork=yes \
    --property=RestrictAddressFamilies=AF_UNIX --property=DevicePolicy=closed \
    --property=MemoryMax=64M --property=TasksMax=32 --property=UMask=0077 \
    "$binary" --serve-readonly /run/rustsimhub-p0/core.sock "$uid"
ready=false
for attempt in 1 2 3 4 5; do
    if test -S /run/rustsimhub-p0/core.sock; then ready=true; break; fi
    sleep 1
done
if [ "$ready" != true ]; then systemctl status rustsimhub-probe-p0 --no-pager; exit 1; fi
printf '\nIPC HEALTH\n'
runuser -u nassimhub -- "$binary" --request /run/rustsimhub-p0/core.sock health
printf '\nIPC CAPABILITIES\n'
runuser -u nassimhub -- "$binary" --request /run/rustsimhub-p0/core.sock capabilities
printf '\nDISALLOWED ROOT CLIENT\n'
if "$binary" --request /run/rustsimhub-p0/core.sock health; then
    echo 'ERROR: unexpected UID accepted'; exit 1
else
    echo 'Different UID rejected as expected'
fi
printf '\nRESOURCE SAMPLE\n'
systemctl show rustsimhub-probe-p0 -p MemoryCurrent -p TasksCurrent -p ActiveState
pid=$(systemctl show rustsimhub-probe-p0 -p MainPID --value)
awk '/VmRSS|VmHWM/ {print}' "/proc/$pid/status"
printf '\nOLD SERVICE STATUS\n'
systemctl is-active nassimhub-agent ModemManager NetworkManager rmtfs
systemctl stop rustsimhub-probe-p0
echo 'Transient smoke service stopped'
