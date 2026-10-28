#!/bin/sh
# SPDX-License-Identifier: AGPL-3.0-only
# Read-only field snapshot for lead acceptance sessions. Never dials, answers,
# hangs up, sends, writes sysfs, changes services/networks or reads secrets.
# Phone numbers are not printed: only call path, direction, state and reason.
#   rsh-readonly-snapshot.sh once
#   rsh-readonly-snapshot.sh watch SECONDS      (1..900, prints only changes)
#   rsh-readonly-snapshot.sh led-trace SECONDS  (1..120, LED changes at ~20 ms)
set -u
umask 077
root=${RSH_FIELD_ROOT:-}
services="rustsimhub-core rustsimhub-edge rustsimhub-media rustsimhub-status-led rustsimhub-network-agent ModemManager NetworkManager rmtfs nassimhub-agent"
leds="red: green:wan blue:"

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

# Three gpio-leds channels, brightness 0/1, printed as rgb=RGB.
led_rgb() {
  rgb=
  for name in $leds; do
    value=x
    [ -r "$root/sys/class/leds/$name/brightness" ] && read -r value < "$root/sys/class/leds/$name/brightness"
    rgb=$rgb$value
  done
  printf '%s' "$rgb"
}

# Only the documented booleans of the fixed LED input files; nothing else.
led_flags() {
  [ -r "$1" ] || { printf 'absent'; return; }
  tr -d ' \n' < "$1" | sed -n 's/.*"expires_ms":\([0-9]*\).*/expires_ms=\1/p' | tr -d '\n'
  for key in $2; do
    tr -d ' \n' < "$1" | sed -n "s/.*\"$key\":\(true\|false\).*/ $key=\1/p" | tr -d '\n'
  done
}

led_state() {
  triggers=
  for name in $leds; do
    t=x
    [ -r "$root/sys/class/leds/$name/trigger" ] && t=$(sed -n 's/.*\[\([^]]*\)\].*/\1/p' "$root/sys/class/leds/$name/trigger")
    triggers="$triggers${triggers:+,}$t"
  done
  mode=absent
  [ -r "$root/etc/rustsimhub/status-led.conf" ] && read -r mode < "$root/etc/rustsimhub/status-led.conf"
  installed=no
  [ -f "$root/var/lib/rustsimhub/rustsimhub-v1/system/installation.complete" ] && installed=yes
  printf 'led rgb=%s trigger=%s mode=%s installed=%s\n' "$(led_rgb)" "$triggers" "$mode" "$installed"
  printf 'led-input %s\n' "$(led_flags "$root/run/rustsimhub-led-input/state.json" 'configuring unread_sms missed_calls')"
  printf 'led-update %s\n' "$(led_flags "$root/run/rustsimhub-led-update/state.json" 'available upgrading')"
}

# Uptime in centiseconds via shell builtins only (no fork per sample).
uptime_cs() {
  read -r up _ < "$root/proc/uptime"
  whole=${up%.*}; frac=${up#*.}
  frac=${frac%"${frac#??}"}
  echo $(( whole * 100 + ${frac#0} ))
}

snapshot() {
  services_state
  calls_state
  pcm_state
  network_state
  led_state
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
  led-trace)
    seconds=${2:-}
    case "$seconds" in ''|*[!0-9]*) echo 'usage: led-trace SECONDS' >&2; exit 2;; esac
    [ "$seconds" -ge 1 ] && [ "$seconds" -le 120 ] || { echo 'led-trace window must be 1..120 seconds' >&2; exit 2; }
    echo "led-trace $(stamp) unit=centiseconds"
    start=$(uptime_cs)
    end=$(( start + seconds * 100 ))
    previous=
    now=$start
    while [ "$now" -lt "$end" ]; do
      current=$(led_rgb)
      if [ "$current" != "$previous" ]; then
        echo "t=$(( now - start )) rgb=$current"
        previous=$current
      fi
      sleep 0.02
      now=$(uptime_cs)
    done
    echo "led-trace-end t=$(( now - start ))"
    ;;
  *)
    echo 'usage: rsh-readonly-snapshot.sh once | watch SECONDS | led-trace SECONDS' >&2
    exit 2
    ;;
esac
