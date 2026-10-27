#!/bin/sh
# SPDX-License-Identifier: AGPL-3.0-only
# Read-only field snapshot for lead acceptance sessions. Never dials, answers,
# hangs up, sends, writes sysfs, changes services/networks or reads secrets.
# Phone numbers are not printed: only call path, direction, state and reason.
#   rsh-readonly-snapshot.sh once
#   rsh-readonly-snapshot.sh watch SECONDS    (1..900, prints only changes)
set -u
umask 077
root=${RSH_FIELD_ROOT:-}
services="rustsimhub-core rustsimhub-edge rustsimhub-media rustsimhub-status-led rustsimhub-network-agent ModemManager NetworkManager rmtfs nassimhub-agent"

services_state() {
  for unit in $services; do
    printf 'service %s %s\n' "$unit" "$(systemctl is-active "$unit" 2>/dev/null || true)"
  done
}

calls_state() {
  list=$(mmcli -m any --voice-list-calls --output-keyvalue 2>/dev/null) || { echo 'calls unavailable'; return; }
  paths=$(printf '%s\n' "$list" | sed -n 's#.*\(/org/freedesktop/ModemManager1/Call/[0-9][0-9]*\).*#\1#p' | sort -u)
  [ -n "$paths" ] || { echo 'calls none'; return; }
  for path in $paths; do
    mmcli -o "$path" --output-keyvalue 2>/dev/null |
      sed -n 's/^call\.properties\.\(direction\|state\|state-reason\) *: *\(.*\)$/\1=\2/p' |
      tr '\n' ' ' | sed "s#^#call ${path##*/} #; s/ *\$//"
    echo
  done
}

pcm_state() {
  found=0
  for status in "$root"/proc/asound/card*/pcm*/sub*/status; do
    [ -r "$status" ] || continue
    found=1
    name=${status#"$root"/proc/asound/}
    printf 'pcm %s %s\n' "${name%/status}" "$(sed -n '1{s/^state: *//;p}' "$status")"
  done
  [ "$found" = 1 ] || echo 'pcm unavailable'
}

network_state() {
  nmcli -t -f DEVICE,TYPE,STATE device status 2>/dev/null | sed 's/^/net /' || echo 'net unavailable'
}

snapshot() {
  services_state
  calls_state
  pcm_state
  network_state
}

stamp() { date -u +%Y-%m-%dT%H:%M:%SZ; }

case "${1:-}" in
  once)
    echo "snapshot $(stamp)"
    snapshot
    ;;
  watch)
    seconds=${2:-}
    case "$seconds" in ''|*[!0-9]*) echo 'usage: watch SECONDS' >&2; exit 2;; esac
    [ "$seconds" -ge 1 ] && [ "$seconds" -le 900 ] || { echo 'watch window must be 1..900 seconds' >&2; exit 2; }
    previous=
    end=$(( $(date +%s) + seconds ))
    while [ "$(date +%s)" -lt "$end" ]; do
      current=$(snapshot)
      if [ "$current" != "$previous" ]; then
        echo "snapshot $(stamp)"
        printf '%s\n' "$current"
        previous=$current
      fi
      sleep 1
    done
    echo "watch-end $(stamp)"
    ;;
  *)
    echo 'usage: rsh-readonly-snapshot.sh once | watch SECONDS' >&2
    exit 2
    ;;
esac
