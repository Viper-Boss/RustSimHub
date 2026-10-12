#!/bin/sh
set -eu
umask 077
install -d -m 0755 /opt/rustsimhub/dev-recovery-p1
install -m 0755 /var/tmp/rustsimhub-core-recovery-p1 /opt/rustsimhub/dev-recovery-p1/rustsimhub-core
install -m 0755 /var/tmp/simhubctl-recovery-p1 /opt/rustsimhub/dev-recovery-p1/simhubctl
base=$(mktemp -d /var/tmp/rustsimhub-recovery-p1-XXXXXXXX)
chown nassimhub:nassimhub "$base"
core=/opt/rustsimhub/dev-recovery-p1/rustsimhub-core
ctl=/opt/rustsimhub/dev-recovery-p1/simhubctl
unit=rustsimhub-recovery-p1
test "$(systemctl is-active "$unit" 2>/dev/null || true)" != active
runuser -u nassimhub -- "$core" --recovery-prepare-fixture "$base"
uid=$(id -u nassimhub)
socket=$base/runtime/core.sock
cleanup() { systemctl stop "$unit" >/dev/null 2>&1 || true; }
trap cleanup EXIT HUP INT TERM
# A timeout covers the entire synthetic workflow; no Python/jq is installed on the stick.
cat > "$base/workflow.sh" <<'WORKFLOW'
#!/bin/sh
set -eu
umask 077
base=$1
core=/opt/rustsimhub/dev-recovery-p1/rustsimhub-core
ctl=/opt/rustsimhub/dev-recovery-p1/simhubctl
pin=$(cat "$base/device.pubhex")
owner=$(cat "$base/owner.pubhex")
old=$(cat "$base/old-client.pubhex")
# Private FIFO input and a private JSONL file coordinate challenge/proof without Python.
mkfifo -m 0600 "$base/proof.fifo"
"$core" --recover-owner "$base/restored" "$base/identity" "$owner" < "$base/proof.fifo" > "$base/recovery.jsonl" &
pid=$!
trap 'kill "$pid" >/dev/null 2>&1 || true; wait "$pid" >/dev/null 2>&1 || true' EXIT HUP INT TERM
exec 3> "$base/proof.fifo"
count=0
while test ! -s "$base/recovery.jsonl"; do
    kill -0 "$pid"
    count=$((count+1)); test "$count" -lt 15; sleep 1
done
head -n 1 "$base/recovery.jsonl" | "$ctl" sign "$base/owner-key" "$pin" recover_owner > "$base/proof.json"
cat "$base/proof.json" >&3
exec 3>&-
wait "$pid"
trap - EXIT HUP INT TERM
tail -n 1 "$base/recovery.jsonl" | grep -q '"owner_recovered":true'
echo 'Offline owner CLI proof accepted'
WORKFLOW
chown nassimhub:nassimhub "$base/workflow.sh"
runuser -u nassimhub -- timeout 40 sh "$base/workflow.sh" "$base"
start_unit() {
    systemd-run --unit "$unit" --property=User=nassimhub --property=Group=nassimhub \
      --property=NoNewPrivileges=yes --property=ProtectSystem=strict --property="ReadWritePaths=$base" \
      --property=RuntimeMaxSec=180 --property=TimeoutStopSec=30 \
      "$core" --serve-authenticated "$socket" "$uid" "$base/restored" "$base/identity"
    count=0
    while test ! -S "$socket"; do count=$((count+1)); test "$count" -lt 15; sleep 1; done
}
start_unit
pin=$(cat "$base/device.pubhex")
old=$(cat "$base/old-client.pubhex")
session=$base/sessions/owner.session.json
runuser -u nassimhub -- "$ctl" login "$socket" "$base/owner-key" "$pin" "$session"
runuser -u nassimhub -- "$ctl" status "$socket" "$session"
runuser -u nassimhub -- "$ctl" clients "$socket" "$session" > "$base/clients-before.json"
if runuser -u nassimhub -- "$ctl" login "$socket" "$base/owner-key" "$old" "$base/sessions/wrong-pin.session.json" >/dev/null 2>&1; then exit 1; fi
test ! -f "$base/sessions/wrong-pin.session.json"
echo 'Wrong pinned device key refused before client proof'
# Restored old client is rejected until explicitly proved and approved again.
if runuser -u nassimhub -- "$ctl" login "$socket" "$base/old-client-key" "$pin" "$base/sessions/denied.session.json" >/dev/null 2>&1; then exit 1; fi
echo 'Quarantined client denied before new approval'
runuser -u nassimhub -- "$ctl" open-pairing "$socket" "$session"
runuser -u nassimhub -- "$ctl" pair "$socket" "$base/old-client-key" "$pin" nas status.read > "$base/pairing.json"
pairing=$(sed -n 's/.*"pairing_id":"\([0-9a-f]\{64\}\)".*/\1/p' "$base/pairing.json")
test "${#pairing}" = 64
runuser -u nassimhub -- "$ctl" pending "$socket" "$session" > "$base/pending.json"
# An approval with different permissions must fail, consuming that pairing request.
if runuser -u nassimhub -- "$ctl" approve "$socket" "$session" "$pairing" "$old" nas sms.send >/dev/null 2>&1; then exit 1; fi
runuser -u nassimhub -- "$ctl" pair "$socket" "$base/old-client-key" "$pin" nas status.read > "$base/pairing.json"
pairing=$(sed -n 's/.*"pairing_id":"\([0-9a-f]\{64\}\)".*/\1/p' "$base/pairing.json")
runuser -u nassimhub -- "$ctl" approve "$socket" "$session" "$pairing" "$old" nas status.read > "$base/approval.json"
client_session=$base/sessions/client.session.json
runuser -u nassimhub -- "$ctl" login "$socket" "$base/old-client-key" "$pin" "$client_session"
runuser -u nassimhub -- "$ctl" status "$socket" "$client_session"
if runuser -u nassimhub -- "$ctl" open-pairing "$socket" "$client_session" >/dev/null 2>&1; then exit 1; fi
runuser -u nassimhub -- "$ctl" revoke "$socket" "$session" "$old"
if runuser -u nassimhub -- "$ctl" status "$socket" "$client_session" >/dev/null 2>&1; then exit 1; fi
echo 'Exact approval, NAS isolation and live revocation verified with real CLI'
systemctl stop "$unit"
test ! -S "$socket"
echo 'SIGTERM removed owned socket'
start_unit
if runuser -u nassimhub -- "$ctl" status "$socket" "$session" >/dev/null 2>&1; then exit 1; fi
runuser -u nassimhub -- "$ctl" login "$socket" "$base/owner-key" "$pin" "$base/sessions/restarted.session.json"
runuser -u nassimhub -- "$ctl" status "$socket" "$base/sessions/restarted.session.json"
systemctl stop "$unit"
test ! -S "$socket"
runuser -u nassimhub -- "$core" --recovery-verify-fixture "$base"
trap - EXIT HUP INT TERM
printf '\nPRIVATE MATERIAL MODES\n'
find "$base" -type f -printf '%m %u %s bytes\n'
printf '\nARTIFACTS\n'
sha256sum "$core" "$ctl"
printf '\nOLD SERVICES\n'
systemctl is-active nassimhub-agent ModemManager NetworkManager rmtfs
printf '\nNETWORK TYPES\n'
nmcli -t -f TYPE,STATE device status
echo 'Only isolated synthetic state used; temporary evidence retained'
