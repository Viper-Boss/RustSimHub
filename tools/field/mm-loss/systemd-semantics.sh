#!/bin/bash
# Runtime-only unit stand-ins mirroring production dependency lines:
#   core:  Requires=/After= ModemManager, OnFailure=app-recover
#   edge:  Requires=/After= core,        OnFailure=app-recover
# Processes are /bin/sleep. Units live in /run and are removed at the end.
set -u
U=/run/systemd/system
M=/run/rsh-sim-recover.marker
cleanup() {
  systemctl stop rsh-sim-edge rsh-sim-core rsh-sim-mm >/dev/null 2>&1
  rm -f $U/rsh-sim-{mm,core,edge,recover}.service $M
  systemctl daemon-reload; systemctl reset-failed 'rsh-sim-*' >/dev/null 2>&1
}
trap cleanup EXIT
cat > $U/rsh-sim-mm.service <<'X'
[Service]
ExecStart=/bin/sleep infinity
X
cat > $U/rsh-sim-core.service <<'X'
[Unit]
Requires=rsh-sim-mm.service
After=rsh-sim-mm.service
OnFailure=rsh-sim-recover.service
[Service]
ExecStart=/bin/sleep infinity
X
cat > $U/rsh-sim-edge.service <<'X'
[Unit]
Requires=rsh-sim-core.service
After=rsh-sim-core.service
OnFailure=rsh-sim-recover.service
[Service]
ExecStart=/bin/sleep infinity
X
cat > $U/rsh-sim-recover.service <<'X'
[Service]
Type=oneshot
ExecStart=/bin/sh -c 'date +%%s >> /run/rsh-sim-recover.marker'
X
systemctl daemon-reload
state() { for u in mm core edge; do printf '%s=%s/%s ' $u "$(systemctl is-active rsh-sim-$u)" "$(systemctl show -p MainPID --value rsh-sim-$u)"; done; printf 'recover_runs=%s\n' "$( [ -f $M ] && wc -l < $M || echo 0)"; }
systemctl start rsh-sim-edge; sleep 1
echo "1 start:                 $(state)"
systemctl kill -s KILL rsh-sim-mm; sleep 2
echo "2 SIGKILL mm:            $(state)"
systemctl reset-failed rsh-sim-mm; systemctl start rsh-sim-mm; sleep 1
echo "3 reset-failed + start:  $(state)"
systemctl restart rsh-sim-mm; sleep 1
echo "4 restart mm (contrast): $(state)"
systemctl stop rsh-sim-mm; sleep 1
echo "5 stop mm (contrast):    $(state)"
