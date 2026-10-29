#!/usr/bin/env python3
# SPDX-License-Identifier: AGPL-3.0-only
"""STAB long-run observer. Writes nothing on the device.

One SSH session runs a read-only sampling loop fed through stdin (no file is
created on the device); it prints one line per minute until a fixed deadline.
If the session drops, the event is logged and a new session starts the same
loop. A second thread performs a strict, certificate-pinned HTTPS GET of the
device home page every 30 seconds. All output stays in a private directory.

  stab-longrun.py --config CFG.json --cert CERT.pem --url URL \
      --paramiko-path DIR --out PRIVATE_DIR --hours 24
"""
import argparse
import hashlib
import json
import pathlib
import ssl
import sys
import threading
import time
import urllib.request

SAMPLER = r'''
set -u
END=__END__
START=__START__
UNITS="rustsimhub-core rustsimhub-edge rustsimhub-media rustsimhub-status-led rustsimhub-network-agent rustsimhub-carrier-voice ModemManager NetworkManager rmtfs"
STATE=/var/lib/rustsimhub/rustsimhub-v1/core/state
i=0
while [ "$(date +%s)" -lt "$END" ]; do
  t0=$(date +%s)
  # One blank-line separated block per unit; field order inside a block is not fixed.
  units=$(systemctl show $UNITS -p Id -p ActiveState -p NRestarts -p MemoryCurrent -p CPUUsageNSec -p TasksCurrent 2>/dev/null |
    awk 'BEGIN{RS=""; FS="\n"} {delete v; for(i=1;i<=NF;i++){p=index($i,"="); v[substr($i,1,p-1)]=substr($i,p+1)}
         id=v["Id"]; sub(/\.service$/,"",id); printf "%s:%s:%s:%s:%s:%s,", id,v["ActiveState"],v["NRestarts"],v["MemoryCurrent"],v["CPUUsageNSec"],v["TasksCurrent"]}')
  set -- $(cat /proc/loadavg); load=$1
  mem=$(awk '/^MemAvailable:/{print $2}' /proc/meminfo)
  p14=$(df -B1 --output=used / | tail -1 | tr -d ' ')
  p15=$(df -B1 --output=used /var/lib/rustsimhub | tail -1 | tr -d ' ')
  jbytes=$(du -sb /var/log/journal 2>/dev/null | cut -f1)
  set -- $(cat /sys/block/mmcblk0/stat); mmc_rd=$3; mmc_wr=$7
  na=$(systemctl show rustsimhub-network-agent -p MainPID --value); jd=$(systemctl show systemd-journald -p MainPID --value)
  na_w=$(awk '/^write_bytes/{print $2}' /proc/$na/io 2>/dev/null); jd_w=$(awk '/^write_bytes/{print $2}' /proc/$jd/io 2>/dev/null)
  wal=$(stat -c %s $STATE/store.sqlite3-wal 2>/dev/null || echo 0); outbox=$(stat -c %s $STATE/forwarding/outbox.sqlite3 2>/dev/null || echo 0)
  net=$(nmcli -t -f DEVICE,STATE device status 2>/dev/null | awk -F: '$1=="wlan0"||$1=="wwan0qmi0"||$1=="usb0"{printf "%s=%s,",$1,$2}')
  calls=$(mmcli -m any --voice-list-calls 2>/dev/null | grep -c '/Call/')
  pcm=0; for s in /proc/asound/card*/pcm*/sub*/status; do [ "$(head -1 $s)" = closed ] || pcm=$((pcm+1)); done
  marker=0; [ -e /run/rustsimhub-app/recovery.used ] && marker=1
  failed=$(systemctl --failed --no-legend --plain | wc -l)
  temp=$(cat /sys/class/thermal/thermal_zone0/temp 2>/dev/null)
  errs=-; locale=-
  if [ $((i % 10)) -eq 0 ]; then
    errs=$(journalctl -p 3 -q --no-pager --since "@$START" 2>/dev/null | wc -l)
    locale=$(journalctl -p 3 -q --no-pager --since "@$START" -u cron 2>/dev/null | grep -c 'default/locale')
  fi
  echo "S t=$t0 up=$(cut -d' ' -f1 /proc/uptime) boot=$(cat /proc/sys/kernel/random/boot_id) load=$load memavail_kb=$mem p14=$p14 p15=$p15 journal=$jbytes mmc_rd=$mmc_rd mmc_wr=$mmc_wr na_wb=$na_w jd_wb=$jd_w wal=$wal outbox=$outbox net=$net calls=$calls pcm_open=$pcm recover_marker=$marker failed=$failed temp=$temp errs=$errs cron_locale=$locale units=$units"
  i=$((i+1))
  left=$((60 - ($(date +%s) - t0))); [ "$left" -gt 0 ] && sleep "$left"
done
echo "END t=$(date +%s)"
'''


def now():
    return time.strftime('%Y-%m-%dT%H:%M:%S%z')


def main():
    ap = argparse.ArgumentParser(description=__doc__)
    for name in ('--config', '--cert', '--url', '--paramiko-path', '--out'):
        ap.add_argument(name, required=True)
    ap.add_argument('--hours', type=float, default=24.0)
    a = ap.parse_args()
    sys.path.insert(0, a.paramiko_path)
    import paramiko

    out = pathlib.Path(a.out)
    out.mkdir(parents=True, exist_ok=False)
    cfg = json.loads(pathlib.Path(a.config).read_text(encoding='utf-8-sig'))
    if 'password' in cfg or 'jump' in cfg:
        raise SystemExit('refusing: key-only direct configuration required')
    if sys.platform == 'win32':
        # Process-scoped: prevents idle sleep only while this process runs; no setting changes.
        import ctypes
        ctypes.windll.kernel32.SetThreadExecutionState(0x80000000 | 0x00000001)
    start = int(time.time())
    end = start + int(a.hours * 3600) + 60
    lock = threading.Lock()
    stop = threading.Event()
    status = {'started': now(), 'end_epoch': end, 'samples': 0, 'sessions': 0,
              'https_ok': 0, 'https_fail': 0, 'last_sample': None, 'last_https': None}

    def log(name, line):
        with lock, (out / name).open('a', encoding='utf-8') as f:
            f.write(f'{now()} {line}\n')

    def save_status():
        with lock:
            (out / 'status.json').write_text(json.dumps(status, indent=1))

    def https_loop():
        ctx = ssl.create_default_context(cafile=a.cert)
        # Direct to the device: a system proxy on the workstation (seen 2026-10-29 at
        # localhost:15236) added ~2 s to every sample and was mistaken for slow device TLS.
        opener = urllib.request.build_opener(urllib.request.ProxyHandler({}),
                                             urllib.request.HTTPSHandler(context=ctx))
        while not stop.is_set() and time.time() < end:
            t = time.time()
            try:
                with opener.open(a.url, timeout=15) as r:
                    body = r.read()
                log('https.log', f'ok status={r.status} bytes={len(body)} ms={int((time.time() - t) * 1000)} '
                                 f'sha={hashlib.sha256(body).hexdigest()[:12]}')
                status['https_ok'] += 1
            except Exception as e:  # recorded, never retried faster or with weaker TLS
                log('https.log', f'fail {type(e).__name__}: {str(e)[:160]}')
                status['https_fail'] += 1
            status['last_https'] = now()
            save_status()
            stop.wait(max(0.0, 30 - (time.time() - t)))

    threading.Thread(target=https_loop, daemon=True).start()
    script = SAMPLER.replace('__END__', str(end)).replace('__START__', str(start))
    log('events.log', f'start end_epoch={end} hours={a.hours}')
    while time.time() < end:
        status['sessions'] += 1
        save_status()
        client = paramiko.SSHClient()
        try:
            client.load_host_keys(cfg['known_hosts'])
            client.set_missing_host_key_policy(paramiko.RejectPolicy())
            client.connect(cfg['host'], port=cfg.get('port', 22), username=cfg['username'],
                           key_filename=cfg['key_filename'], timeout=15, auth_timeout=15,
                           banner_timeout=15, look_for_keys=False, allow_agent=False)
            client.get_transport().set_keepalive(30)
            stdin, stdout, _ = client.exec_command('sh -s')
            stdout.channel.set_combine_stderr(True)
            stdin.write(script)
            stdin.channel.shutdown_write()
            log('events.log', f'session {status["sessions"]} open')
            for line in iter(stdout.readline, ''):
                line = line.rstrip('\n')
                log('samples.log', line)
                if line.startswith('S '):
                    status['samples'] += 1
                    status['last_sample'] = now()
                    save_status()
            log('events.log', f'session {status["sessions"]} closed exit={stdout.channel.recv_exit_status()}')
        except Exception as e:
            log('events.log', f'session {status["sessions"]} error {type(e).__name__}: {str(e)[:160]}')
        finally:
            client.close()
        if time.time() < end:
            time.sleep(30)
    stop.set()
    status['finished'] = now()
    save_status()
    log('events.log', 'finished')


if __name__ == '__main__':
    main()
