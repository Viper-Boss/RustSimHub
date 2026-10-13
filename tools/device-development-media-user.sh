#!/bin/sh
# A locked project-only media identity; ALSA membership is supplied per unit, never permanent.
exec 2>&1
set -eu
marker=/opt/rustsimhub/.development-media-user-installed
if ! test -f "$marker";then
 if getent passwd rustsimhub-media >/dev/null || getent group rustsimhub-media >/dev/null;then exit 10;fi
 config=$(mktemp /var/tmp/rustsimhub-media-sysusers-XXXXXXXX);chmod 0600 "$config"
 cat > "$config" <<'CONFIG'
u rustsimhub-media - "RustSimHub media" /nonexistent /usr/sbin/nologin
CONFIG
 systemd-sysusers "$config"
 (umask 077;printf '%s\n' 'RustSimHub development media identity v1' > "$marker")
fi
test "$(id -u rustsimhub-media)" -ne 0
test "$(getent passwd rustsimhub-media | cut -d: -f6)" = /nonexistent
test "$(getent passwd rustsimhub-media | cut -d: -f7)" = /usr/sbin/nologin
if id -Gn rustsimhub-media | tr ' ' '\n' | grep -Eq '^(audio|sudo|netdev)$';then exit 10;fi
id rustsimhub-media
