#!/usr/bin/env python3
# SPDX-License-Identifier: AGPL-3.0-only
"""Check C15 draft links/anchors and evidence IDs without network or device access.

This checks documentation structure, not the truth of hardware acceptance.
Run from any directory with Python 3.9+; the repository root is source-relative.
"""
from pathlib import Path
import json
import re
import sys
from urllib.parse import unquote

root = Path(__file__).resolve().parents[2]
pages = sorted((root / 'docs/manuals').glob('*.md'))
errors = []
count = 0
external = set()
refs = set()
for page in pages:
    data = page.read_text(encoding='utf-8')
    refs.update(re.findall(r'support-evidence\.md#(s\d+)', data))
    if re.search(r'\.private[/\\]|\.local[/\\]|[A-Z]:\\Users\\|BEGIN (?:OPENSSH |RSA |EC )?PRIVATE KEY', data):
        errors.append([page.name, 'private_material_marker'])
    for number, line in enumerate(data.splitlines(), 1):
        if line.rstrip() != line:
            errors.append([page.name, number, 'trailing_whitespace'])
    for link in re.findall(r'\]\(([^)]+)\)', data):
        if re.match(r'https?://', link):
            external.add(link)
            continue
        count += 1
        path, separator, anchor = link.partition('#')
        target = (page.parent / unquote(path)).resolve() if path else page
        if not target.is_relative_to(root) or not target.is_file():
            errors.append([page.name, link, 'missing_or_outside_file'])
            continue
        if separator:
            target_data = target.read_text(encoding='utf-8')
            ids = set(re.findall(r'<a id="([^"]+)">', target_data))
            ids.update(re.sub(r'[^\w\-\s]', '', heading.strip().lower()).replace(' ', '-')
                       for heading in re.findall(r'^#{1,6} (.+)$', target_data, re.M))
            if unquote(anchor) not in ids:
                errors.append([page.name, link, 'missing_anchor'])
ledger = (root / 'docs/manuals/support-evidence.md').read_text(encoding='utf-8')
defined = re.findall(r'<a id="(s\d+)"></a>', ledger)
if len(defined) != 16 or len(set(defined)) != 16 or refs - set(defined):
    errors.append(['support-evidence.md', 'support_id_inventory'])
print(json.dumps({'manual_pages': len(pages), 'local_links_checked': count,
                  'support_ids': len(defined), 'referenced_support_ids': len(refs),
                  'external_links': sorted(external), 'errors': errors}, ensure_ascii=False))
sys.exit(bool(errors))
