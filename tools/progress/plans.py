# SPDX-License-Identifier: AGPL-3.0-only
"""Project public task-book steps into the standalone dashboard; never accept work."""
import hashlib
from pathlib import Path
import re


def require(ok, message):
    if not ok:
        raise ValueError(message)


def project_ids(text, allowed):
    text = re.sub(r'([A-Z][A-Z0-9]*-)(\d{2})[～–](\d{2})',
                  lambda m: ' '.join(m[1]+str(n).zfill(2) for n in range(int(m[2]), int(m[3])+1)), text)
    text = re.sub(r'([A-Z][A-Z0-9]*-)(\d{2})((?:/\d{2})+)',
                  lambda m: ' '.join(m[1]+n for n in [m[2], *m[3].split('/')[1:]]), text)
    return sorted(set(re.findall(r'\b[A-Z][A-Z0-9]*-\d{2}\b', text)) & set(allowed))


def parse_document(text, source, wanted, allowed):
    """Only documented heading/table forms; missing/duplicate steps fail closed."""
    lines = text.splitlines(); rows = {}; section = ''; chapter = ''

    def add(key, title, work, acceptance, line, batch, task_ids=None):
        if key not in wanted:
            return
        require(key not in rows, 'duplicate task-book step: '+key)
        require(title.strip() and work.strip(), 'empty task-book step: '+key)
        rows[key] = {'id': key, 'title': title.strip(), 'work': work.strip(),
                     'acceptance': acceptance.strip(), 'source': source, 'line': line,
                     'batch': batch, 'task_ids': task_ids if task_ids is not None else project_ids(title+' '+work, allowed)}

    for index, line in enumerate(lines):
        if line.startswith('## '):
            chapter = line[3:].strip(); section = chapter
        elif line.startswith('### '):
            section = line[4:].strip()
        heading = re.match(r'^### ([A-D]-\d{2})(?:\s*/\s*(.+))?\s*$', line)
        if heading and heading[1] in wanted:
            end = index+1
            while end < len(lines) and not re.match(r'^#{1,3} ', lines[end]):
                end += 1
            add(heading[1], heading[2] or heading[1], '\n'.join(lines[index+1:end]), '', index+1, chapter)
        if not line.startswith('| '):
            continue
        cells = [cell.strip() for cell in re.split(r'(?<!\\)\|', line)[1:-1]]
        if len(cells) != 3:
            continue
        step = re.match(r'^([A-D]-\d{2})(?:\s*/\s*(.*))?$', cells[0])
        if step:
            key = step[1]
            title = (step[2]+ ' · ' if step[2] else '') + cells[1].split('；')[0]
            add(key, title, cells[1], cells[2], index+1, section)
            continue
        # T5 specifies several concrete steps in one owner row; retain its
        # package-level acceptance and project scope rather than inventing it.
        if not re.match(r'^(fungi1220|wnfhsj(?:88)?|yh1030|RedamancyGTW) / ', cells[0]):
            continue
        matches = list(re.finditer(r'([A-D]-\d{2})：', cells[1]))
        if not matches:
            continue
        prefix = matches[0][1][0]
        scopes = {'A': ['NKN-07'], 'B': ['BLE4-01','BLE4-02','BLE4-03','BLE4-04'],
                  'C': ['AP-04'], 'D': ['BLE3-01','BLE3-04']}[prefix]
        for n, match in enumerate(matches):
            end = matches[n+1].start() if n+1 < len(matches) else len(cells[1])
            work = cells[1][match.end():end].strip('； ')
            add(match[1], work, work, cells[2], index+1, 'T5 · '+cells[0].split(' / ')[0]+'接口离线包', scopes)
    return rows


def build(root, members, reviews, work):
    root = Path(root)
    sources = []; documents = []
    paths = list(dict.fromkeys([m['handoff'] for m in members if m['id'] != 'lead' or m['step_ids']] +
                              ['docs/team/ADDITIONAL-OFFLINE-PACKAGES-2026-10-27.md']))
    for path in paths:
        target = root/path
        require(target.resolve().is_relative_to(root.resolve()) and target.is_file() and not target.is_symlink(), 'task-book source outside repository')
        raw = target.read_bytes()
        sources.append({'path': path, 'sha256': hashlib.sha256(raw).hexdigest()})
        documents.append((path, raw.decode('utf8')))
    output = []
    for member in members:
        if member['id'] == 'lead' and not member['step_ids']:
            continue
        wanted = set(member['step_ids']); steps = {}
        for source, text in documents:
            parsed = parse_document(text, source, wanted, member['task_ids'])
            require(not set(parsed) & set(steps), 'step defined in multiple task books')
            steps.update(parsed)
        require(set(steps) == wanted, 'task-book steps missing: '+', '.join(sorted(wanted-set(steps))))
        decisions = {}
        fresh = {r['pr']: r['state'] for r in work[member['id']]['reviews']}
        for review in reviews:
            if review['member_id'] == member['id'] and fresh.get(review['pr']) in ('accepted','offline_accepted'):
                for key in review['steps']:
                    decisions[key] = fresh[review['pr']]
        records = []
        for key in member['step_ids']:
            record = steps[key]
            record['status'] = decisions.get(key, 'not_accepted')
            records.append(record)
        require(sum(r['status'] != 'not_accepted' for r in records) == work[member['id']]['accepted_steps'], 'plan acceptance count drift')
        output.append({'member_id': member['id'], 'steps': records})
    return {'schema': 1, 'sources': sources, 'members': output}


def validate(plan, members, root=None):
    require(set(plan) == {'schema','sources','members'} and plan['schema'] == 1, 'task plan schema')
    sources = set()
    for source in plan['sources']:
        require(set(source) == {'path','sha256'} and source['path'].startswith('docs/team/')
                and '..' not in Path(source['path']).parts and re.fullmatch('[0-9a-f]{64}', source['sha256']), 'task plan public source')
        require(source['path'] not in sources, 'duplicate plan source'); sources.add(source['path'])
        if root is not None:
            target = Path(root)/source['path']
            require(target.resolve().is_relative_to(Path(root).resolve()) and target.is_file()
                    and hashlib.sha256(target.read_bytes()).hexdigest() == source['sha256'], 'task plan source changed; regenerate task plan')
    expected = {m['id']: m for m in members if m['id'] != 'lead' or m['step_ids']}
    require({m['member_id'] for m in plan['members']} == set(expected) and len(plan['members']) == len(expected), 'plan member inventory')
    for entry in plan['members']:
        require(set(entry) == {'member_id','steps'}, 'plan member fields')
        member = expected[entry['member_id']]
        require([r['id'] for r in entry['steps']] == member['step_ids'], 'plan step inventory/order')
        for row in entry['steps']:
            require(set(row) == {'id','title','work','acceptance','source','line','batch','task_ids','status'}, 'plan step fields')
            require(row['source'] in sources and type(row['line']) is int and row['line'] > 0, 'plan source reference')
            require(row['status'] in ('not_accepted','offline_accepted','accepted'), 'plan review state')
            require(all(isinstance(row[k],str) and len(row[k]) <= 10000 for k in ['title','work','acceptance','batch']) and row['title'] and row['work'], 'plan content bounds')
            require(len(set(row['task_ids'])) == len(row['task_ids']) and set(row['task_ids']) <= set(member['task_ids']), 'plan project scope')
        require(sum(r['status'] != 'not_accepted' for r in entry['steps']) == member['work']['accepted_steps'], 'plan accepted steps')
