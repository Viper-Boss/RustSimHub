#!/usr/bin/env python3
# SPDX-License-Identifier: AGPL-3.0-only
"""Refresh public, SHA-bound team attribution; never exports author emails."""
import argparse
import datetime
import json
from pathlib import Path, PurePosixPath
import re
import shutil
import subprocess

ROOT = Path(__file__).resolve().parents[2]
REPO = 'Viper-Boss/RustSimHub'
CATEGORIES = ('product', 'tests', 'tooling', 'firmware')


def require(value, message):
    if not value:
        raise ValueError(message)


def git(repo, *args):
    return subprocess.check_output(['git', '--literal-pathspecs', '-c', 'core.quotepath=false', *args], cwd=repo).decode('utf8', 'replace')


def classify(name):
    path = PurePosixPath(name)
    if path.is_absolute() or '..' in path.parts or '\\' in name:
        return None
    suffix = path.suffix.lower()
    if name.startswith(('crates/', 'native/audio-alsa/src/')) and suffix in ('.rs', '.js', '.mjs', '.html', '.css'):
        return 'tests' if '/tests/' in name or path.name in ('tests.rs', 'test.rs') or path.name.endswith(('_tests.rs', '_test.rs')) else 'product'
    # These are product Go programs under tools/, not build scripts. Never
    # attribute vendored SDK/dependency sources to repository contributors.
    if name.startswith(('tools/nas-adapter/', 'tools/nkn-engine/')) and suffix == '.go' and 'vendor' not in path.parts:
        return 'tests' if path.name.endswith('_test.go') else 'product'
    if name.startswith('configurator/') and suffix in ('.js', '.mjs', '.html', '.css', '.py'):
        if 'tests' in path.parts:
            return 'tests'
        if name.startswith('configurator/src/') or name in ('configurator/index.html', 'configurator/sw.js'):
            return 'product'
    if name.startswith('tools/') and suffix in ('.py', '.sh', '.js', '.mjs'):
        return 'tests' if path.name.startswith(('test_', 'test-')) else 'tooling'
    if name.startswith('firmware/system/') and suffix in ('.sh', '.json', '.conf', '.service', '.target', '.rules'):
        return 'firmware'
    return None


def roster_valid(roster):
    require(roster.get('schema') == 1, 'roster schema')
    members = roster['members']
    ids=[m['id'] for m in members]
    require(1 <= len(ids) <= 20 and ids[0]=='lead' and len(set(ids))==len(ids)
            and all(re.fullmatch('member-[a-z]+',value) for value in ids[1:]), 'stable member IDs, lead first')
    aliases = {}
    for member in members:
        login = member['login']
        require(login is None or re.fullmatch(r'[A-Za-z0-9](?:[A-Za-z0-9-]{0,37}[A-Za-z0-9])?', login), 'GitHub login')
        require(0 < len(member['name']) <= 100 and member['state'] in ('assigned', 'in_progress', 'submitted'), 'member display/state')
        require(member['handoff'].startswith('docs/team/') and '..' not in PurePosixPath(member['handoff']).parts, 'public taskbook')
        require(len(set(member['step_ids'])) == len(member['step_ids']), 'duplicate assignment step')
        for alias in [*member['git_names'], *([login] if login else [])]:
            key = alias.casefold()
            require(key not in aliases or aliases[key] == member['id'], 'ambiguous Git identity mapping')
            aliases[key] = member['id']
    return members


def identity(name, email, members):
    # Emails are used only transiently for public GitHub noreply attribution.
    # A commit author is an engineering attribution, not identity authentication.
    match = re.fullmatch(r'(?:\d+\+)?([A-Za-z0-9-]+)@users\.noreply\.github\.com', email, re.I)
    login = match[1].casefold() if match else None
    for member in members:
        if login and member['login'] and login == member['login'].casefold():
            return member['id']
        # roster_valid treats an explicit GitHub login as a declared alias too.
        # A member may commit with a verified non-noreply email: never publish it.
        if (member['login'] and name.casefold() == member['login'].casefold()) or name.casefold() in {alias.casefold() for alias in member['git_names']}:
            return member['id']
    return 'unmapped'


def empty():
    return {'current_lines': dict.fromkeys(CATEGORIES, 0), 'commits': 0,
            'code_commits': 0, 'added': 0, 'deleted': 0}


def code_stats(repo, revision, members):
    require(re.fullmatch('[0-9a-f]{40}', revision), 'full source SHA required')
    result = {m['id']: empty() for m in members}
    result['unmapped'] = empty()
    paths = git(repo, 'ls-tree', '-rz', revision).split('\0')
    for entry in paths:
        if not entry:
            continue
        meta, name = entry.split('\t', 1)
        if meta.split()[0] not in ('100644', '100755'):
            continue
        category = classify(name)
        if not category:
            continue
        author = email = None
        blame = git(repo, 'blame', '--root', '--line-porcelain', revision, '--', name)
        for line in blame.split('\n'):
            if line.startswith('author '):
                author = line[7:]
            elif line.startswith('author-mail <'):
                email = line[13:-1]
            elif line.startswith('\t') and line[1:].strip():
                require(author is not None and email is not None, 'missing blame attribution')
                result[identity(author, email, members)]['current_lines'][category] += 1
    history = git(repo, 'log', '--no-merges', '--no-renames', '--numstat', '-z',
                  '--format=%x1e%H%x1f%an%x1f%ae', revision)
    for record in history.split('\x1e')[1:]:
        chunks = record.split('\0')
        sha, name, email = chunks[0].split('\x1f')
        require(re.fullmatch('[0-9a-f]{40}', sha), 'history SHA')
        row = result[identity(name, email, members)]
        row['commits'] += 1
        touched = False
        for entry in chunks[1:]:
            fields = entry.lstrip('\n').split('\t', 2)
            if len(fields) != 3 or not classify(fields[2]):
                continue
            if fields[0].isdigit() and fields[1].isdigit():
                row['added'] += int(fields[0]); row['deleted'] += int(fields[1])
                touched = True
        row['code_commits'] += int(touched)
    return result


def gh_prs(members, reviews):
    gh = shutil.which('gh') or 'C:/Program Files/GitHub CLI/gh.exe'
    # REST works with the existing repository connection; no GraphQL-only
    # login refresh or new repository permission is required for this read.
    all_prs = {}
    for page in range(1, 6):
        rows = json.loads(subprocess.check_output([gh, 'api', 'repos/'+REPO+'/pulls?state=all&per_page=100&page='+str(page)], timeout=30))
        all_prs.update({row['number']: row for row in rows})
        if len(rows) < 100:
            break
    else:
        raise ValueError('PR inventory limit reached; do not report a partial scan as complete')
    numbers = {row['pr'] for row in reviews} | set(all_prs)
    result = {}
    for number in sorted(numbers):
        require(type(number) is int and number > 0, 'PR number')
        row = all_prs.get(number)
        if row is None:
            row = json.loads(subprocess.check_output([gh, 'api', 'repos/'+REPO+'/pulls/'+str(number)], timeout=30))
        result[number] = {'number': number, 'headRefOid': row['head']['sha'],
                          'state': 'MERGED' if row.get('merged_at') else row['state'].upper(),
                          'author': {'login': row['user']['login']}}
    return result


def review_summary(member, reviews, prs):
    accepted = set(); reported = []; states = []
    for row in reviews:
        if row['member_id'] != member['id']:
            continue
        require(re.fullmatch('[0-9a-f]{40}', row['head']), 'review full HEAD')
        require(row['decision'] in ('needs_changes', 'offline_accepted', 'accepted'), 'review decision')
        require(set(row['steps']) <= set(member['step_ids']) and row['evidence'], 'review scope and evidence')
        require(all(p.startswith('docs/') and '..' not in PurePosixPath(p).parts for p in row['evidence']), 'public review evidence')
        actual = prs.get(row['pr'])
        fresh = (actual is not None and actual['headRefOid'] == row['head'] and member['login'] is not None
                 and actual['author']['login'].casefold() == member['login'].casefold())
        state = row['decision'] if fresh else 'needs_re_review'
        if fresh and row['decision'] in ('offline_accepted', 'accepted'):
            accepted.update(row['steps'])
            hours = row.get('self_reported_hours')
            if hours is not None:
                require(type(hours) in (int, float) and 0 < hours <= 1000, 'self-reported hours')
                reported.append(hours)
        states.append({'pr': row['pr'], 'head': row['head'], 'state': state})
    delivered = [{'pr': p['number'], 'head': p['headRefOid'], 'state': p['state']} for p in prs.values()
                 if member['login'] and p['author']['login'].casefold() == member['login'].casefold()]
    return {'assigned_steps': len(member['step_ids']), 'accepted_steps': len(accepted),
            'self_reported_hours': sum(reported) if reported else None, 'reviews': states, 'deliveries': delivered}


def snapshot(repo, revision, roster, reviews, prs):
    members = roster_valid(roster)
    require(reviews.get('schema') == 1, 'reviews schema')
    require(all(row['member_id'] in {m['id'] for m in members} for row in reviews['reviews']), 'unknown reviewer member')
    require(len({row['pr'] for row in reviews['reviews']}) == len(reviews['reviews']), 'duplicate PR review')
    work = {m['id']: review_summary(m, reviews['reviews'], prs) for m in members}
    import plans
    task_plan = plans.build(ROOT, members, reviews['reviews'], work)
    stats = code_stats(repo, revision, members)
    public = []
    for member in members:
        item = {key: value for key, value in member.items() if key != 'git_names'}
        item.update(stats=stats[member['id']], work=work[member['id']])
        public.append(item)
    return {'schema': 1, 'source_revision': revision,
            'generated_at': datetime.datetime.now(datetime.timezone.utc).isoformat(),
            'method': 'Git blame非空行；主线非merge增删/提交；明确HEAD审核的步骤；工时仅成员自报',
            'members': public, 'unmapped': stats['unmapped'],
            'totals': {kind: sum(row['current_lines'][kind] for row in stats.values()) for kind in CATEGORIES},
            'historical_note': '团队加入前主线历史保留；分配、CI、合并均不自动构成完整功能验收。',
            'task_plan': task_plan}


def validate_snapshot(value, task_ids):
    require(value.get('schema') == 1 and re.fullmatch('[0-9a-f]{40}', value['source_revision']), 'team snapshot source')
    fields = {'schema','source_revision','generated_at','method','members','unmapped','totals','historical_note'}
    require(set(value) == fields or set(value) == fields | {'task_plan'}, 'team snapshot allowlist')
    ids=[m['id'] for m in value['members']]
    require(1 <= len(ids) <= 20 and ids[0]=='lead' and len(set(ids))==len(ids)
            and all(re.fullmatch('member-[a-z]+',item) for item in ids[1:]), 'team snapshot member set')
    member_keys = {'id','name','login','role','access','package','task_ids','step_ids','handoff','state','estimate_hours','stats','work'}
    for member in value['members']:
        require(set(member) == member_keys, 'public member allowlist')
        require(set(member['task_ids']) <= set(task_ids), 'unknown assigned task')
        require((ROOT/member['handoff']).is_file() and member['handoff'].startswith('docs/team/'), 'missing public handoff')
        require(member['login'] is None or re.fullmatch(r'[A-Za-z0-9](?:[A-Za-z0-9-]{0,37}[A-Za-z0-9])?', member['login']), 'public login')
        work = member['work']
        require(type(work['accepted_steps']) is int and type(work['assigned_steps']) is int, 'work counts must be integers')
        require(0 <= work['accepted_steps'] <= work['assigned_steps'] == len(member['step_ids']), 'work count boundary')
        require(work['self_reported_hours'] is None or type(work['self_reported_hours']) in (int,float) and 0 < work['self_reported_hours'] <= 1000, 'work hours boundary')
        for pr in work['deliveries']:
            require(type(pr['pr']) is int and pr['pr'] > 0 and re.fullmatch('[0-9a-f]{40}', pr['head']) and pr['state'] in ('OPEN','CLOSED','MERGED'), 'public delivery')
    for row in [m['stats'] for m in value['members']] + [value['unmapped']]:
        require(set(row) == {'current_lines','commits','code_commits','added','deleted'} and set(row['current_lines']) == set(CATEGORIES), 'stats allowlist')
        require(all(type(n) is int and 0 <= n < 2**53 for n in [*row['current_lines'].values(),row['commits'],row['code_commits'],row['added'],row['deleted']]), 'stats count boundary')
    require(set(value['totals']) == set(CATEGORIES), 'total category set')
    for category in CATEGORIES:
        require(value['totals'][category] == sum(m['stats']['current_lines'][category] for m in value['members'])+value['unmapped']['current_lines'][category], 'team total drift')
    assigned = [task for m in value['members'] if m['id'] != 'lead' for task in m['task_ids']]
    require(len(assigned) == len(set(assigned)), 'project action has multiple offline owners')
    if 'task_plan' in value:
        import plans
        plans.validate(value['task_plan'], value['members'], ROOT)


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--ref', choices=['origin/main'], default='origin/main')
    args = parser.parse_args()
    remote = git(ROOT, 'remote', 'get-url', 'origin').strip()
    require(remote in ('https://github.com/'+REPO+'.git', 'git@github.com:'+REPO+'.git'), 'repository binding')
    revision = git(ROOT, 'rev-parse', '--verify', args.ref+'^{commit}').strip()
    roster = json.loads((ROOT/'docs/team/roster.json').read_text(encoding='utf8'))
    reviews = json.loads((ROOT/'docs/team/reviews.json').read_text(encoding='utf8'))
    members = roster_valid(roster)
    team = snapshot(ROOT, revision, roster, reviews, gh_prs(members, reviews['reviews']))
    path = ROOT/'docs/progress/progress.json'
    data = json.loads(path.read_text(encoding='utf8')); data['team'] = team
    data['updated_at'] = team['generated_at']
    path.write_text(json.dumps(data, ensure_ascii=False, indent=2)+'\n', encoding='utf8', newline='\n')
    print(json.dumps({'source_revision': revision, 'members': len(team['members']), 'current_nonempty_lines': team['totals']}, ensure_ascii=False))


if __name__ == '__main__':
    main()
