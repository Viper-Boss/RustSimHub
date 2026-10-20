# SPDX-License-Identifier: AGPL-3.0-only
"""Finite restore wire policy and no-retry bulk transport. No CLI or USB discovery."""
import copy
import array
import hashlib
import re
import time
import xml.etree.ElementTree as ET

import edl_readonly as recovery

# The fixed 8916 programmer explicitly reported this limit on the real board.
# Keep write configuration and raw chunks identical; do not negotiate/retry a
# rejected write configuration or reuse the read-only client's initial request.
MAX_PAYLOAD = 16384
_readonly_xml = recovery.validate_xml
MAX_REPLY = 65536


def decode_control_reply(data):
    """Parse only complete bounded data frames; a log containing 'ACK' is not an ACK."""
    if not isinstance(data, bytes) or not data or len(data) > MAX_REPLY or b'<!' in data:
        raise ValueError('Unsafe or oversized control reply')
    cleaned = re.sub(rb'<\?xml[ \t\r\n]+[^?<>]{1,256}\?>', b'', data)
    if b'<?' in cleaned:
        raise ValueError('Unexpected control processing instruction')
    root = ET.fromstring(b'<frames>' + cleaned + b'</frames>')
    if (root.text or '').strip():
        raise ValueError('Text outside control frame')
    responses = []
    for frame in root:
        if frame.tag != 'data' or frame.attrib or (frame.text or '').strip() or (frame.tail or '').strip():
            raise ValueError('Unexpected control frame')
        for item in frame:
            if list(item) or (item.text or '').strip() or (item.tail or '').strip():
                raise ValueError('Nested or nonempty control reply element')
            if item.tag == 'log' and set(item.attrib) == {'value'}:
                continue
            if item.tag != 'response' or item.get('value') not in ('ACK', 'NAK'):
                raise ValueError('Unexpected control reply element')
            if item.get('rawmode') not in (None, 'true', 'false'):
                raise ValueError('Invalid control raw mode')
            responses.append(dict(item.attrib))
    if len(responses) > 1:
        raise ValueError('Ambiguous multiple control responses')
    return responses[0] if responses else None


def read_control_reply(endpoint, timeout_seconds=15, clock=time.monotonic):
    """Read EP_IN directly; upstream timeout escalation must not make ACK waits unbounded."""
    if not 0 < timeout_seconds <= 30:
        raise ValueError('Control reply deadline must be bounded')
    deadline = clock() + timeout_seconds
    data = bytearray()
    while clock() < deadline:
        try:
            part = endpoint.read(4096, timeout=1000)
        except Exception as exc:
            if isinstance(exc, TimeoutError) or getattr(exc, 'errno', None) == 110 or getattr(exc, 'backend_error_code', None) == -7:
                continue
            raise
        if (not isinstance(part, (bytes, bytearray, array.array)) or len(part) > 4096
                or (isinstance(part, array.array) and part.typecode != 'B')):
            raise ValueError('Unexpected USB reply type or length')
        data.extend(part)
        if len(data) > MAX_REPLY:
            raise ValueError('Oversized USB control reply')
        if data.rstrip().endswith(b'</data>'):
            reply = decode_control_reply(bytes(data))
            if reply is not None:
                return reply
        if not part:
            time.sleep(0.01)
    raise TimeoutError('Explicit control ACK deadline expired; do not resend the command')


def sha(value):
    return isinstance(value, str) and re.fullmatch('[0-9a-f]{64}', value) is not None


def write_bulk_once(endpoint, data):
    """A short/unknown transfer is terminal; never resend bytes with uncertain delivery."""
    if not isinstance(data, (bytes, bytearray)):
        raise ValueError('Only explicit bytes may reach the bulk endpoint')
    payload = bytes(data)
    if not payload:
        count = endpoint.write(b'', timeout=5000)
        if type(count) is not int or count != 0:
            raise IOError('Zero-length USB transfer was not confirmed')
        return 0
    written = 0
    while written < len(payload):
        block = payload[written:written + MAX_PAYLOAD]
        count = endpoint.write(block, timeout=5000)
        if type(count) is not int or count != len(block):
            raise IOError('USB short/unknown write; inspect delivery, no automatic retry')
        written += count
    return written


class RestorePolicy:
    """Trusted runner must supply a re-derived layout; a plan alone cannot arm writes."""
    def __init__(self, plan, targets, protected):
        self.phase = 'preflight'
        self.targets = copy.deepcopy(targets)
        self.protected = copy.deepcopy(protected)
        self.plan = copy.deepcopy(plan)
        targets, protected, plan = self.targets, self.protected, self.plan
        self.index = 0
        self.current_protected = None
        self.sent_bytes = 0
        self.payload_hash = None
        self._require(plan.get('kind') == 'fixed-system-restore-plan' and plan.get('passed') is True,
                      'A verified restore plan is required')
        self._require(len(targets) == 2 and [(p['index'], p['name']) for p in targets]
                      == [(14, 'rootfs'), (15, 'NSHSTATE')], 'Only the two fixed system partitions may be programmed')
        self._require(plan.get('write_targets') == targets and plan.get('protected_ranges') == protected,
                      'Plan ranges differ from independently re-derived ranges')
        self._require(all(sha(p.get('backup_sha256')) for p in targets + protected)
                      and all(sha(plan.get(k)) for k in ('source_image_sha256', 'gpt_sha256', 'boot_sha256')),
                      'Plan digests are incomplete')
        self._require(plan['boot_sha256'] == recovery.BOOT_SHA256, 'Boot identity differs from the fixed board profile')
        cursor = 0
        for row in sorted(targets + protected, key=lambda p: p['offset']):
            self._require(type(row['offset']) is int and type(row['bytes']) is int
                          and row['offset'] == cursor and row['bytes'] > 0 and row['bytes'] % 512 == 0,
                          'Incomplete, overlapping or unaligned protected/target coverage')
            cursor += row['bytes']
        self._require(cursor == recovery.DISK_BYTES, 'Wrong fixed disk geometry')
        for p in targets:
            self._require(type(p['physical_partition_number']) is int and p['physical_partition_number'] == 0
                          and p['bytes'] == recovery.PARTITIONS[p['index']][1]
                          and p['offset'] >= 34 * 512 and p['offset'] + p['bytes'] <= recovery.DISK_BYTES - 33 * 512
                          and type(p['start_sector']) is int and p['start_sector'] * 512 == p['offset']
                          and type(p['sector_count']) is int and p['sector_count'] * 512 == p['bytes'],
                          'Wrong fixed program geometry')

    def fail(self, message):
        self.phase = 'failed'
        raise ValueError(message)

    def _require(self, condition, message):
        if not condition:
            self.fail(message)

    def _range_hashes(self, values, ranges):
        self._require(isinstance(values, list) and len(values) == len(ranges), 'Incomplete readback ranges')
        normalized = []
        for value, expected in zip(values, ranges):
            self._require(isinstance(value, dict) and set(value) == {'offset', 'bytes', 'sha256'}
                          and type(value['offset']) is int and value['offset'] == expected['offset']
                          and type(value['bytes']) is int and value['bytes'] == expected['bytes']
                          and sha(value['sha256']), 'Readback range or digest mismatch')
            normalized.append(copy.deepcopy(value))
        return normalized

    def confirm_preflight(self, gpt_hash, boot_hash, source_hash, current_protected):
        self._require(self.phase == 'preflight' and gpt_hash == self.plan['gpt_sha256']
                      and boot_hash == self.plan['boot_sha256'] and source_hash == self.plan['source_image_sha256'],
                      'Preflight inputs differ from the trusted plan')
        self.current_protected = self._range_hashes(current_protected, self.protected)
        self.phase = 'arm-configuration'

    def finish_readonly_preflight(self, gpt_hash, boot_hash, source_hash, current_protected):
        self.confirm_preflight(gpt_hash, boot_hash, source_hash, current_protected)
        self.phase = 'readonly-verified'

    def validate_xml(self, data, allow_reset=False):
        # The second argument permits use behind the existing read-only loader's
        # hook. It never grants reset/write authority by itself.
        try:
            raw = data.encode() if isinstance(data, str) else bytes(data)
            self._require(len(raw) <= 8192 and b'<!' not in raw, 'Unsafe restore control XML')
            root = ET.fromstring(raw)
            self._require(root.tag == 'data' and not root.attrib and len(root) == 1,
                          'Exactly one control operation is required')
            command = root[0]
            self._require(not list(command), 'Nested control operation')
            if self.phase in ('preflight', 'post-readback') and command.tag != 'power':
                if command.tag == 'configure':
                    self._require(self.phase == 'preflight', 'No reconfiguration after programming')
                _readonly_xml(raw, False)
                return
            if self.phase == 'arm-configuration' and command.tag == 'configure':
                expected = {'MemoryName': 'eMMC', 'Verbose': '0', 'AlwaysValidate': '0',
                            'MaxDigestTableSizeInBytes': '2048', 'MaxPayloadSizeToTargetInBytes': str(MAX_PAYLOAD),
                            'ZLPAwareHost': '1', 'SkipStorageInit': '0', 'SkipWrite': '0'}
                self._require(command.attrib == expected, 'Write configuration differs from the fixed profile')
                self.phase = 'configuration-sent'
                return
            if self.phase == 'program-ready' and command.tag == 'program':
                p = self.targets[self.index]
                expected = {'SECTOR_SIZE_IN_BYTES': '512', 'physical_partition_number': '0',
                            'start_sector': str(p['start_sector']), 'num_partition_sectors': str(p['sector_count'])}
                self._require(command.attrib == expected, 'Program command is outside the next exact target')
                self.phase = 'program-command-sent'
                return
            if self.phase in ('verified', 'readonly-verified') and command.tag == 'power' and command.attrib == {'value': 'reset'}:
                self.phase = 'reset-sent'
                return
            self.fail('Operation is not permitted in the current restore phase')
        except Exception:
            self.phase = 'failed'
            raise

    def acknowledge(self, response):
        self._require(isinstance(response, dict) and response.get('value') == 'ACK',
                      'Explicit ACK missing; no automatic retry or success inference')
        if self.phase == 'configuration-sent':
            self._require(response.get('rawmode') in (None, 'false'), 'Unexpected raw mode after configuration')
            self.phase = 'program-ready'
        elif self.phase == 'program-command-sent':
            self._require(response.get('rawmode') == 'true', 'Explicit program raw-mode ACK required before payload')
            self.sent_bytes = 0
            self.payload_hash = hashlib.sha256()
            self.phase = 'payload'
        elif self.phase == 'payload-sent':
            self._require(response.get('rawmode') == 'false', 'Explicit final control-mode ACK required')
            self.index += 1
            self.phase = 'program-ready' if self.index < len(self.targets) else 'post-readback'
        elif self.phase == 'reset-sent':
            self.phase = 'reset-acknowledged'
        else:
            self.fail('Unsolicited or repeated ACK')

    def send_payload_chunk(self, endpoint, data):
        self._require(self.phase == 'payload' and isinstance(data, bytes), 'Payload sent outside the raw phase')
        p = self.targets[self.index]
        self._require(0 < len(data) <= MAX_PAYLOAD and len(data) % 512 == 0
                      and self.sent_bytes + len(data) <= p['bytes'], 'Payload exceeds the exact target')
        try:
            write_bulk_once(endpoint, data)
            write_bulk_once(endpoint, b'')
        except BaseException:
            self.phase = 'failed'
            raise
        self.sent_bytes += len(data)
        self.payload_hash.update(data)

    def finish_payload(self):
        self._require(self.phase == 'payload' and self.sent_bytes == self.targets[self.index]['bytes']
                      and self.payload_hash.hexdigest() == self.targets[self.index]['backup_sha256'],
                      'Payload byte count or digest differs from the frozen backup target')
        self.phase = 'payload-sent'

    def confirm_readback(self, gpt_hash, boot_hash, restored, protected):
        self._require(self.phase == 'post-readback' and self.index == 2
                      and gpt_hash == self.plan['gpt_sha256'] and boot_hash == self.plan['boot_sha256'],
                      'Readback cannot finish an incomplete restore')
        targets = self._range_hashes(restored, self.targets)
        self._require(all(p['sha256'] == wanted['backup_sha256'] for p, wanted in zip(targets, self.targets)),
                      'Restored target digest mismatch')
        self._require(self._range_hashes(protected, self.protected) == self.current_protected,
                      'Protected device bytes changed during restore')
        self.phase = 'verified'
