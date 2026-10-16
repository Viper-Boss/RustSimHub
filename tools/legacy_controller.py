#!/usr/bin/env python3
# SPDX-License-Identifier: AGPL-3.0-only
"""Bounded, host-only exclusive modem control for legacy-system trials.

This never dials. The device's independent systemd service stops the trial Core
before restoring the original Go agent, including host disconnect or timeout.
Do not install this helper as a product control API.
"""
import re
import shlex
import time
import uuid

UNIT = 'rustsimhub-legacy-controller.service'


def scripts(core_unit, directory, seconds):
    if not re.fullmatch(r'rustsimhub-web-media-[a-f0-9]{10}-core', core_unit):
        raise ValueError('Expected an owned HTTPS trial Core unit')
    if not re.fullmatch(r'/var/tmp/rustsimhub-web-media-[A-Za-z0-9]{8}/operator/controller-[a-f0-9]{16}', directory):
        raise ValueError('Expected a private root-owned trial directory')
    if not isinstance(seconds, int) or not 30 <= seconds <= 900:
        raise ValueError('Exclusive controller window must be 30..900 seconds')
    q = shlex.quote
    preamble = '#!/bin/sh\nset -eu\numask 077\n'
    start = preamble + f'''
state=$(systemctl is-active nassimhub-agent.service) || true
case "$state" in active|inactive) ;; *) exit 4 ;; esac
calls=$(mmcli -m 0 --voice-list-calls)
if printf '%s\\n' "$calls" | grep -Ev '^[[:space:]]*/org/freedesktop/ModemManager1/Call/[0-9]+ (incoming|outgoing) \\(terminated\\)$|^No calls were found\\.?$';then exit 4;fi
# Record intent before stopping, so ExecStopPost handles a failed stop as well.
printf '%s\\n' "$state" > {q(directory + '/original')}
if test "$state" = active;then systemctl stop nassimhub-agent.service || true;fi
state=$(systemctl is-active nassimhub-agent.service) || true
# The legacy ExecStopPost hangup-all returns failure when there are no calls.
# Accept that stopped state only with no remaining controller process.
case "$state" in inactive|failed) ;; *) exit 4 ;; esac
test "$(systemctl show --value -p MainPID nassimhub-agent.service)" = 0
test "$(systemctl is-active ModemManager NetworkManager rmtfs | grep -c '^active$')" = 3
printf 'ready\\n' > {q(directory + '/ready')}
sleep {seconds}
'''
    restore = preamble + f'''
test -e {q(directory + '/original')} || exit 0
test ! -e {q(directory + '/restored')} || exit 0
# Core's lease cleanup must finish before the old controller takes over again.
systemctl stop {q(core_unit)} || true
if test "$(cat {q(directory + '/original')})" = active;then
 systemctl start nassimhub-agent.service
 systemctl is-active --quiet nassimhub-agent.service
else
 state=$(systemctl is-active nassimhub-agent.service) || true
 test "$state" = inactive
fi
printf 'restored\\n' > {q(directory + '/restored')}
'''
    return start, restore


class LegacyControllerWindow:
    def __init__(self, run, sftp, base, core_unit, seconds=75):
        self.run, self.sftp = run, sftp
        self.directory = base + '/operator/controller-' + uuid.uuid4().hex[:16]
        self.start_script, self.restore_script = scripts(core_unit, self.directory, seconds)
        self.seconds = seconds
        self.started = False

    def arm(self):
        q = shlex.quote
        self.run('mkdir -m 0700 ' + q(self.directory))
        for name, data in [('start', self.start_script), ('restore', self.restore_script)]:
            path = self.directory + '/' + name
            with self.sftp.open(path, 'wx') as handle:
                handle.write(data.encode())
            self.sftp.chown(path, 0, 0)
            self.sftp.chmod(path, 0o500)
        # A single fixed unit prevents overlapping trial owners. Register the
        # restore action and hard timeout before the script can stop the agent.
        command = ['systemd-run', '--quiet', '--collect', '--unit=' + UNIT,
                   '-p', 'Type=exec', '-p', 'RuntimeMaxSec=' + str(self.seconds + 2),
                   '-p', 'TimeoutStopSec=10', '-p', 'KillMode=control-group',
                   '-p', 'ExecStopPost=' + self.directory + '/restore',
                   '--', self.directory + '/start']
        self.run(shlex.join(command))
        self.started = True
        end = time.monotonic() + 10
        while time.monotonic() < end:
            try:
                with self.sftp.open(self.directory + '/ready', 'rb') as handle:
                    if handle.read(16) == b'ready\n':
                        return {'unit': UNIT, 'directory': self.directory,
                                'hold_seconds': self.seconds, 'independent_restore': True}
            except FileNotFoundError:
                pass
            time.sleep(.1)
        raise RuntimeError('Exclusive controller did not become ready; independent restore remains armed')

    def release(self):
        if not self.started:
            return False
        # If already collected, the independent restore's completion file is
        # authoritative. Never stop a later trial that reused the fixed unit.
        try:
            with self.sftp.open(self.directory + '/restored', 'rb') as handle:
                if handle.read(16) == b'restored\n':
                    return True
        except FileNotFoundError:
            pass
        current = self.run('systemctl show --value -p ExecStart ' + UNIT)
        if self.directory + '/start' not in current:
            raise RuntimeError('Exclusive controller ownership changed; refusing to stop another trial')
        self.run('systemctl stop ' + UNIT)
        with self.sftp.open(self.directory + '/restored', 'rb') as handle:
            if handle.read(16) != b'restored\n':
                raise RuntimeError('Legacy controller restore was not confirmed')
        return True
