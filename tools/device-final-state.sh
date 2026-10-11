#!/bin/sh
set -u
printf 'NEW UNIT STATE\n'; systemctl is-active rustsimhub-probe-p0 || true
printf '\nOLD SERVICES\n'; systemctl is-active nassimhub-agent ModemManager NetworkManager rmtfs
printf '\nNETWORK TYPES\n'; nmcli -t -f TYPE,STATE device status
printf '\nARTIFACT HASH\n'; sha256sum /opt/rustsimhub/dev-p0/rustsimhub-core
