#!/usr/bin/env python3
# SPDX-License-Identifier: AGPL-3.0-only
"""Validate the public action ledger and render standalone HTML + detailed MD."""
from pathlib import Path
import argparse
import json
import math
import re

ROOT = Path(__file__).resolve().parents[2]
DIRECTORY = ROOT / 'docs/progress'
STATUSES = {'todo', 'ready', 'in_progress', 'needs_user', 'needs_data', 'done', 'parked'}
SCOPES = {'original', 'extension', 'parked'}
REPO = 'https://github.com/Viper-Boss/RustSimHub'


def require(value, message):
    if not value:
        raise ValueError(message)


def public_evidence(value):
    if value.startswith(REPO + '/'):
        return value
    require(not value.startswith(('/', '\\')) and '\\' not in value, 'evidence must be repository relative')
    path = Path(value)
    require('..' not in path.parts and path.parts and path.parts[0] in ['docs', 'tools', 'crates', 'firmware', 'native'], 'private/outside evidence')
    resolved = (ROOT / path).resolve()
    require(resolved.is_relative_to(ROOT.resolve()) and resolved.is_file(), 'evidence file missing: ' + value)
    return REPO + '/blob/main/' + value


def validate(data):
    require(data.get('schema') == 1, 'unsupported ledger schema')
    require(data['project']['repository'] == REPO and data['project']['branch'] == 'main', 'repository binding')
    require(data['baseline_version'] and data['updated_at'] and data['baseline_at'], 'missing snapshot time')
    milestones = data['original_milestones']
    require([x['id'] for x in milestones] == ['P'+str(i) for i in range(8)], 'milestone inventory')
    require(sum(x['weight'] for x in milestones) == 100, 'original milestone denominator')
    require(all(0 <= x['done_points'] <= x['weight'] for x in milestones), 'original milestone bound')
    # The original percentage is evidence-bound, not derived from new tasks.
    scale = (ROOT / 'docs/PROGRESS-SCALE.md').read_text(encoding='utf-8')
    for item in milestones:
        match = re.search(r'^\| '+item['id']+r' [^|]+\|\s*([\d.]+)\s*\|\s*([\d.]+)\s*\|', scale, re.M)
        require(match and (float(match[1]), float(match[2])) == (item['weight'], item['done_points']), 'milestone drift: '+item['id'])
    modules = data['modules']
    require(len({x['id'] for x in modules}) == len(modules), 'duplicate module')
    tasks = {}
    for module in modules:
        require(module['scope'] in SCOPES and module['phase'] in ['usable', 'extensions', 'release', 'parked'], 'module scope/phase')
        public_evidence(module['source'])
        require(module['tasks'] and module['paths'], 'empty module')
        for entry in module['paths']:
            if entry.startswith(('planned:', 'upstream:')):
                continue
            require(not Path(entry).is_absolute() and '..' not in Path(entry).parts, 'outside source path')
            require(bool(list(ROOT.glob(entry))) if '*' in entry else (ROOT/entry).exists(), 'source path missing: '+entry)
        for index, task in enumerate(module['tasks'], 1):
            require(task['id'] == module['id']+'-'+str(index).zfill(2) and task['id'] not in tasks, 'stable action ID/order')
            require(task['status'] in STATUSES, 'action status')
            require(task['title'] and task['work'] and task['acceptance'], 'missing concrete work/acceptance')
            hours = task['estimate_hours']
            require(len(hours) == 2 and all(isinstance(h, (int, float)) and not isinstance(h, bool) and math.isfinite(h) for h in hours) and 0 < hours[0] <= hours[1] <= 100, 'estimate bound')
            require(task['passive_wait_hours'] >= 0, 'wait bound')
            require(isinstance(task['needs'], list) and isinstance(task['dependencies'], list), 'dependency types')
            require(len(set(task['dependencies'])) == len(task['dependencies']), 'duplicate dependency')
            require((module['scope'] == 'parked') == (task['status'] == 'parked'), 'parked scope must stay outside active denominator')
            if task['status'] == 'done':
                require(task['evidence'] and task['completed_at'], 'done needs evidence and time: '+task['id'])
            for evidence in task['evidence']:
                public_evidence(evidence)
            tasks[task['id']] = task
    def visit(key, visiting, visited):
        require(key in tasks, 'unknown dependency: '+key)
        require(key not in visiting, 'dependency cycle: '+key)
        if key in visited:
            return
        visiting.add(key)
        for dependency in tasks[key]['dependencies']:
            visit(dependency, visiting, visited)
        visiting.remove(key)
        visited.add(key)
    visited = set()
    for key, task in tasks.items():
        visit(key, set(), visited)
        if task['status'] == 'done':
            require(all(tasks[x]['status'] == 'done' for x in task['dependencies']), 'done before dependency: '+key)
    require(data['focus']['task_id'] in tasks, 'focus task missing')
    if 'team' in data:
        import team
        team.validate_snapshot(data['team'], tasks)
    for source in data['sources']:
        public_evidence(source)
    # Public metadata may contain source SHA/estimates, never maintenance hosts
    # or raw device/SIM data. The ledger has no reason to contain an IPv4 URL.
    encoded = json.dumps(data, ensure_ascii=False)
    require(not re.search(r'https?://(?:\d{1,3}\.){3}\d{1,3}', encoded), 'private network URL in public ledger')
    require('-----BEGIN ' not in encoded and '.private/' not in encoded and '.local/' not in encoded, 'private material in public ledger')
    return tasks


def metrics(data):
    active = [t for m in data['modules'] if m['scope'] != 'parked' for t in m['tasks']]
    remaining = [t for t in active if t['status'] != 'done']
    weight = lambda t: sum(t['estimate_hours'])/2
    total_weight = sum(map(weight, active))
    done_weight = sum(weight(t) for t in active if t['status'] == 'done')
    return {'original_percent': sum(x['done_points'] for x in data['original_milestones']),
        'active': len(active), 'done': len(active)-len(remaining), 'remaining': len(remaining),
        'parked': sum(len(m['tasks']) for m in data['modules'] if m['scope'] == 'parked'),
        'sprint_percent': round(done_weight/total_weight*100, 1) if total_weight else 100,
        'remaining_hours': [sum(t['estimate_hours'][i] for t in remaining) for i in range(2)],
        'passive_wait_max': max((t['passive_wait_hours'] for t in remaining), default=0)}


def markdown(data):
    value = metrics(data)
    labels = {'original':'原计划收尾','extension':'新增扩展','parked':'搁置 / 按需'}
    states = {'todo':'待做','ready':'就绪','in_progress':'进行中','needs_user':'待用户','needs_data':'待换卡','done':'已完成','parked':'已搁置'}
    needs = {'device':'代理实机','user':'用户配合','provider':'获准提供商目标','data_card':'换有流量卡','second_device':'第二设备','server':'测试服务器','remote_peer':'外网对端','sms_authorization':'新的短信发送授权','ble_authorization':'单独BLE授权'}
    lines = ['# RustSimHub 完整剩余行动台账', '', f"快照：{data['updated_at']} · 基线：{data['baseline_version']}", '',
        f"原P0–P7：**{value['original_percent']:g}%**，剩余 **{100-value['original_percent']:g}** 个验收点。新增X1/X2/X3/BLE独立保留，不混入旧分母。", '',
        f"本轮台账：**{value['done']}/{value['active']}** 动作已完成，按冻结工时中值加权 **{value['sprint_percent']}%**；另有{value['parked']}项搁置/按需动作不计入本轮分母。", '',
        f"剩余有效开发工时粗估 **{value['remaining_hours'][0]:g}–{value['remaining_hours'][1]:g}小时**。{data['estimate']['calendar_note']}", '',
        '看板入口：[GitHub Pages](https://viper-boss.github.io/RustSimHub/) · [单文件HTML](index.html) · [结构化台账](progress.json)', '',
        '## 使用和更新', '', '完成标准以每项验收为准；源码通过、设备交付、真人验收分开记录。每次完成后附公开脱敏证据，用维护脚本更新，再生成HTML/本清单并随提交发布。', '',
        '```sh', 'python tools/progress/update.py complete FWD-03 --evidence docs/验证记录.md --note "实际完成及验收结果" --actual-hours 1.0', 'python tools/progress/build.py --check', '```', '',
        '## 当前关卡', '', f"**{data['focus']['task_id']} · {data['focus']['title']}**：{data['focus']['state']}。{data['focus']['description']}", '']
    for m in data['modules']:
        done = sum(t['status']=='done' for t in m['tasks'])
        lines += [f"## {m['id']} · {m['title']}", '', f"{labels[m['scope']]} · {len(m['tasks'])}个动作 · 已完成{done}个", '', m['summary'], '',
            '代码/资料：'+'、'.join('`'+p+'`' for p in m['paths']), '', '[范围来源]('+public_evidence(m['source'])+')', '']
        for t in m['tasks']:
            lines += [f"### {'[x]' if t['status']=='done' else '[ ]'} {t['id']} · {t['title']}", '',
                '- 状态：'+states[t['status']], '- 动作：'+t['work'], '- 验收：'+t['acceptance'],
                f"- 预计有效工时：{t['estimate_hours'][0]:g}–{t['estimate_hours'][1]:g}小时" + (f"；另有约{t['passive_wait_hours']:g}小时观测等待" if t['passive_wait_hours'] else ''),
                '- 前置：'+('、'.join(t['dependencies']) or '无跨动作硬依赖'),
                '- 配合/条件：'+('、'.join(needs[n] for n in t['needs']) or '可由代理继续离线推进'),
                '- Claude离线接手：'+('可以；仍须提供SHA与本地验证报告' if t['offline_delegable'] else '可协助代码审查；本动作完整验收含实机/外部条件')]
            if t['evidence']:
                lines += ['- 证据：'+'、'.join('['+p.split('/')[-1]+']('+public_evidence(p)+')' for p in t['evidence'])]
            lines += ['']
    lines += ['## 估算口径与当前约束', '', data['estimate']['method'], '', data['estimate']['confidence'], '']
    lines += ['- '+x for x in data['constraints']]
    return '\n'.join(lines)+'\n'


def outputs(data):
    validate(data)
    template = (DIRECTORY/'index.template.html').read_text(encoding='utf-8')
    original = metrics(data)['original_percent']
    template = template.replace('{{MILESTONE_PERCENT}}', f'{original:g}').replace('{{MILESTONE_REMAINING}}', f'{100-original:g}')
    payload = json.dumps(data, ensure_ascii=False, separators=(',', ':')).replace('<','\\u003c').replace('\u2028','\\u2028').replace('\u2029','\\u2029')
    css = (DIRECTORY/'dashboard.css').read_text(encoding='utf-8')+'\n'+(DIRECTORY/'team.css').read_text(encoding='utf-8')
    script = (DIRECTORY/'dashboard.mjs').read_text(encoding='utf-8')
    require('</script' not in script.lower() and '</style' not in css.lower(), 'unsafe embedded source delimiter')
    result = template.replace('<!-- DASHBOARD_STYLE -->', '<style>\n'+css+'\n</style>').replace('<!-- DASHBOARD_DATA -->','<script id="progress-data" type="application/json">'+payload+'</script>').replace('<!-- DASHBOARD_SCRIPT -->','<script type="module">\n'+script+'\n</script>')
    return {'index.html':result, 'remaining.md':markdown(data)}


def main():
    parser=argparse.ArgumentParser(description=__doc__);parser.add_argument('--check',action='store_true');args=parser.parse_args()
    data=json.loads((DIRECTORY/'progress.json').read_text(encoding='utf-8'))
    for name,text in outputs(data).items():
        path=DIRECTORY/name;encoded=text.encode('utf-8')
        if args.check:
            require(path.is_file() and path.read_bytes()==encoded, 'generated file stale: '+name)
        else:path.write_bytes(encoded)
    print(json.dumps(metrics(data),ensure_ascii=False))


if __name__=='__main__':main()
