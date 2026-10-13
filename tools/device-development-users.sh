#!/bin/sh
# Only create the new project's locked system identities. Never modify old service users.
set -eu
marker=/opt/rustsimhub/.development-users-installed
if ! test -f "$marker"; then
    for name in rustsimhub-core rustsimhub-edge; do
        if getent passwd "$name" >/dev/null; then echo 'Project user name already occupied; inspect first' >&2; exit 1; fi
    done
    if getent group rustsimhub-ipc >/dev/null; then echo 'Project group name already occupied; inspect first' >&2; exit 1; fi
    config=$(mktemp /var/tmp/rustsimhub-sysusers-XXXXXXXX)
    chmod 0600 "$config"
    cat > "$config" <<'CONFIG'
g rustsimhub-ipc -
u rustsimhub-core - "RustSimHub core" /nonexistent /usr/sbin/nologin
u rustsimhub-edge - "RustSimHub edge" /nonexistent /usr/sbin/nologin
m rustsimhub-core rustsimhub-ipc
m rustsimhub-edge rustsimhub-ipc
CONFIG
    systemd-sysusers "$config"
    install -d -m 0755 /opt/rustsimhub
    (umask 077; printf '%s\n' 'RustSimHub development users v1' > "$marker")
fi
for name in rustsimhub-core rustsimhub-edge; do
    test "$(id -u "$name")" -ne 0
    test "$(getent passwd "$name" | cut -d: -f6)" = /nonexistent
    test "$(getent passwd "$name" | cut -d: -f7)" = /usr/sbin/nologin
    id -Gn "$name" | tr ' ' '\n' | grep -qx rustsimhub-ipc
done
echo 'Distinct locked core/edge users and private IPC group ready; old service identities unchanged'
