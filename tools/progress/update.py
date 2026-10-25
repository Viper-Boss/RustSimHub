#!/usr/bin/env python3
# SPDX-License-Identifier: AGPL-3.0-only
"""Evidence-bound action updates; rebuild the public HTML/MD in the same step."""
from pathlib import Path
from datetime import datetime, timezone
import argparse
import copy
import json
import math
import os
import build


def transition(data, key, command, note, evidence=(), actual_hours=None, state=None, now=None):
    result=copy.deepcopy(data)
    tasks=build.validate(result)
    build.require(key in tasks, 'unknown action: '+key)
    task=tasks[key]
    build.require(task['status']!='parked', 'parked action needs an explicit scope revision')
    build.require(isinstance(note,str) and 1<=len(note.strip())<=1000, 'concrete update note required')
    at=now or datetime.now(timezone.utc).isoformat()
    for item in evidence:build.public_evidence(item)
    if actual_hours is not None:
        build.require(math.isfinite(actual_hours) and 0<actual_hours<=1000, 'actual hours bound')
    if command=='complete':
        build.require(evidence, 'completion needs public sanitized evidence')
        if task['status']=='done':
            build.require(task['evidence']==list(evidence) and task['actual_hours']==actual_hours, 'already completed; review rather than silently rewrite evidence')
            return data
        build.require(all(tasks[x]['status']=='done' for x in task['dependencies']), 'complete dependencies first')
        task.update(status='done',completed_at=at,evidence=list(evidence),actual_hours=actual_hours)
        title=key+' · 已完成：'+task['title']
    elif command=='focus':
        build.require(task['status']!='done', 'choose an unfinished focus action')
        result['focus']={'task_id':key,'title':task['title'],'state':'正在推进' if task['status']=='in_progress' else '下一动作已选定；按前置和验收推进','description':note.strip()}
        title=key+' · 下一关卡：'+task['title']
    elif command=='state':
        build.require(task['status']!='done', 'completed action cannot silently reopen')
        build.require(state in ['todo','ready','in_progress','needs_user','needs_data'], 'invalid action state')
        if state in ['ready','in_progress']:
            build.require(all(tasks[x]['status']=='done' for x in task['dependencies']), 'dependencies not ready')
        task['status']=state
        title=key+' · 状态更新：'+task['title']
    else:raise ValueError('unknown update command')
    result['updated_at']=at
    result['activity'].append({'at':at,'type':'completed' if command=='complete' else 'state','title':title,'detail':note.strip(),'task_ids':[key],'evidence':list(evidence)})
    if result['focus']['task_id']==key and command=='state':
        result['focus']['state']={'todo':'等待推进','ready':'前置已完成，下一动作就绪','in_progress':'正在执行；完成后附证据','needs_user':'等待已说明的用户配合','needs_data':'等待换有流量卡'}[state]
    if result['focus']['task_id']==key and command=='complete':
        module=next(m for m in result['modules'] if any(t['id']==key for t in m['tasks']))
        next_task=next((t for t in module['tasks'] if t['status'] not in ['done','parked'] and all(tasks[x]['status']=='done' for x in t['dependencies'])),None)
        if next_task:
            result['focus']={'task_id':next_task['id'],'title':next_task['title'],'state':'前置已完成，按所列条件继续推进','description':next_task['work']+' 验收：'+next_task['acceptance']}
        else:
            result['focus']['state']='当前动作已完成，继续按依赖选择下一项'
            result['focus']['description']=note.strip()
    build.validate(result)
    return result


def atomic(path, data):
    temporary=path.with_name(path.name+'.tmp')
    with temporary.open('xb') as file:
        file.write(data);file.flush();os.fsync(file.fileno())
    temporary.replace(path)


def main():
    parser=argparse.ArgumentParser(description=__doc__)
    parser.add_argument('command',choices=['complete','state','focus'])
    parser.add_argument('task')
    parser.add_argument('--note',required=True)
    parser.add_argument('--evidence',action='append',default=[])
    parser.add_argument('--actual-hours',type=float)
    parser.add_argument('--state',choices=['todo','ready','in_progress','needs_user','needs_data'])
    args=parser.parse_args()
    directory=build.DIRECTORY;path=directory/'progress.json';lock=directory/'.progress-update.lock'
    # This is a repository writer lock, not a device service or automation.
    descriptor=os.open(lock,os.O_WRONLY|os.O_CREAT|os.O_EXCL,0o600)
    try:
        before=path.read_bytes();data=json.loads(before)
        next_data=transition(data,args.task,args.command,args.note,args.evidence,args.actual_hours,args.state)
        if next_data==data:
            print('Already recorded with the same evidence; no duplicate activity.');return
        generated=build.outputs(next_data)
        build.require(path.read_bytes()==before, 'ledger changed during update')
        atomic(path,(json.dumps(next_data,ensure_ascii=False,indent=2)+'\n').encode())
        for name,text in generated.items():atomic(directory/name,text.encode())
        print(json.dumps(build.metrics(next_data),ensure_ascii=False))
    finally:
        os.close(descriptor);lock.unlink()


if __name__=='__main__':main()
