# SPDX-License-Identifier: AGPL-3.0-only
"""Only explicit public files are examined; findings never include their contents."""
import base64
from pathlib import PurePosixPath
import re
from safeio import need, parse

PRIVATE_NAME = re.compile(r'(^|[._-])(seed|private|password|token|secret)([._-]|$)', re.I)
PRIVATE_PEM = re.compile(rb'-----BEGIN ([A-Z0-9 ]*PRIVATE KEY)-----\s*'
                         rb'([A-Za-z0-9+/=\r\n ]{16,65536})'
                         rb'-----END \1-----')
SECRET_FIELD = re.compile(r'^(?:private[_-]?key|identity[_-]?seed|seed|password|access[_-]?token|'
                          r'secret|token|psk|ssid|wifi[_-]?password|sms[_-]?body|message[_-]?body|phone[_-]?number|'
                          r'(?:one_time_)?setup_code|claim_code|imei|imsi|iccid|msisdn|usb_serial)$', re.I)


def inspect(name, raw):
    """Public keys/certificates are allowed. A raw secret renamed as a public key
    cannot be identified from entropy; this is explicitly not a secrecy proof.
    """
    parts = PurePosixPath(name).parts
    need(not any(p in ('.private', '.ssh', '.gnupg', '.aws') for p in parts), 'private_path')
    need(not PRIVATE_NAME.search(parts[-1]) or parts[-1].endswith('.public'), 'private_filename')
    need(parts[-1] not in ('id_rsa', 'id_ed25519', 'identity.pending')
         and not parts[-1].endswith('.key'), 'private_filename')
    # Compiled key parsers and source examples contain PEM label constants.
    # Require a complete base64 body, not just such a label.
    for match in PRIVATE_PEM.finditer(raw):
        try:
            body = base64.b64decode(re.sub(rb'\s', b'', match[2]), validate=True)
        except ValueError:
            continue
        need(len(body) < 16, 'private_key_content')
    need(b'openssh-key-v1\x00' not in raw, 'private_key_content')
    # RFC8410 PKCS8 raw Ed25519 seed. Public RFC8410 SPKI is a different prefix.
    need(not raw.startswith(bytes.fromhex('302e020100300506032b657004220420')), 'private_key_content')
    if name.endswith('.json'):
        value = parse(raw)
        def visit(value):
            if isinstance(value, dict):
                for key, item in value.items():
                    need(not (SECRET_FIELD.fullmatch(key) and not (item is None or item is False or (type(item) is str and item==''))),
                         'private_json_field')
                    visit(item)
            elif isinstance(value, list):
                for item in value:
                    visit(item)
        visit(value)
    return {'private_material_detected': False,
            'key_class': 'public_or_certificate' if (b'PUBLIC KEY-----' in raw or
                b'CERTIFICATE-----' in raw or raw.startswith(b'ssh-ed25519 ')) else 'not_classified'}
