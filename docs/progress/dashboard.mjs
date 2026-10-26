// SPDX-License-Identifier: AGPL-3.0-only
export const REPOSITORY = 'https://github.com/Viper-Boss/RustSimHub';
export const needLabels = {device:'代理实机',user:'用户配合',provider:'获准提供商目标',data_card:'待换流量卡',second_device:'第二设备',server:'测试服务器',remote_peer:'外网对端',sms_authorization:'新短信授权',ble_authorization:'单独BLE授权'};
export const statusLabels = {todo:'待推进',ready:'已就绪',in_progress:'进行中',needs_user:'待用户',needs_data:'待换卡',done:'已完成',parked:'已搁置'};
export const escapeHTML = value => String(value ?? '').replace(/[&<>"']/g, character => ({'&':'&amp;','<':'&lt;','>':'&gt;','"':'&quot;',"'":'&#39;'}[character]));
export const midpoint = task => (task.estimate_hours[0]+task.estimate_hours[1])/2;
export const number = value => Number.isInteger(value) ? String(value) : value.toFixed(1);
export const publicURL = value => value.startsWith(REPOSITORY+'/') ? value : REPOSITORY+'/blob/main/'+value.split('/').map(encodeURIComponent).join('/');

export function calculate(data) {
  const active=data.modules.filter(m=>m.scope!=='parked').flatMap(m=>m.tasks);
  const done=active.filter(t=>t.status==='done'), remaining=active.filter(t=>t.status!=='done');
  const totalWeight=active.reduce((sum,t)=>sum+midpoint(t),0), doneWeight=done.reduce((sum,t)=>sum+midpoint(t),0);
  return {original:data.original_milestones.reduce((sum,m)=>sum+m.done_points,0),total:active.length,done:done.length,remaining:remaining.length,
    parked:data.modules.filter(m=>m.scope==='parked').reduce((sum,m)=>sum+m.tasks.length,0),
    weighted:totalWeight ? doneWeight/totalWeight*100 : 100,
    hours:[0,1].map(i=>remaining.reduce((sum,t)=>sum+t.estimate_hours[i],0)),
    offline:remaining.filter(t=>t.offline_delegable).length,
    assisted:remaining.filter(t=>t.needs.some(n=>!['device'].includes(n))).length};
}

export function dependencyReady(task, taskMap) {
  return task.dependencies.every(id=>taskMap.get(id)?.status==='done');
}

export function selectModules(data, {query='',scope='all',condition='all'}={}) {
  const taskMap=new Map(data.modules.flatMap(m=>m.tasks).map(t=>[t.id,t]));
  const term=query.trim().toLocaleLowerCase();
  return data.modules.filter(m=>scope==='all' ? m.scope!=='parked' : m.scope===scope).map(m=>{
    const moduleMatches=[m.id,m.title,m.summary,...m.paths].join(' ').toLocaleLowerCase().includes(term);
    const matches=m.tasks.filter(t=>{
      const textMatch=!term||moduleMatches||[t.id,t.title,t.work,t.acceptance].join(' ').toLocaleLowerCase().includes(term);
      const conditionMatch=condition==='all'||(condition==='done'&&t.status==='done')||
        (condition==='offline'&&t.offline_delegable&&t.status!=='done'&&t.status!=='parked')||
        (condition==='assist'&&t.status!=='done'&&t.needs.some(n=>n!=='device'))||
        (condition==='actionable'&&['todo','ready','in_progress'].includes(t.status)&&dependencyReady(t,taskMap)&&t.needs.every(n=>n==='device'));
      return textMatch&&conditionMatch;
    });
    return {module:m,matches};
  }).filter(row=>row.matches.length);
}

export function estimateDays(hours, capacity) {
  if(!Number.isFinite(capacity)||capacity<1||capacity>24) throw new Error('有效工作小时超出范围');
  return hours.map(value=>Math.ceil(value/capacity));
}

export function handoffText(module) {
  return [`RustSimHub / ${module.id} · ${module.title}`,`仓库：${REPOSITORY}`,`范围来源：${publicURL(module.source)}`,`代码：${module.paths.join('、')}`,
    '先读AGENTS.md、HANDOFF顶部和最新进度。仅按任务授权开展工作；不要假定你可以操作设备。真实验收由实机负责人安排，提交时提供SHA、命令、结果及未验证项。',
    ...module.tasks.map(t=>`\n${t.id} [${statusLabels[t.status]}] ${t.title}\n动作：${t.work}\n验收：${t.acceptance}\n预计：${t.estimate_hours.join('–')}有效小时；依赖：${t.dependencies.join('、')||'无'}；条件：${t.needs.map(n=>needLabels[n]).join('、')||'离线可做'}`)].join('\n');
}

export function teamHTML(team) {
  if(!team?.members?.length)return '<p>团队统计尚未生成；不显示推测数值。</p>';
  const count=value=>Number.isSafeInteger(value)&&value>=0?value:0;
  const number=value=>count(value).toLocaleString('zh-CN');
  const total=count(team.totals.product), labels={assigned:'已分配 · 待开工',in_progress:'主线进行中',submitted:'已提交 · 待审核'};
  return team.members.map((m,index)=>{
    const s=m.stats,w=m.work,product=count(s.current_lines.product),pct=total?product/total*100:0;
    const accepted=count(w.accepted_steps),assigned=count(w.assigned_steps);
    const github=m.login?`<a href="https://github.com/${encodeURIComponent(m.login)}" target="_blank" rel="noopener">${escapeHTML(m.name)} ↗</a>`:escapeHTML(m.name);
    const work=index===0?'历史主线 + 持续集成 / 设备 / 审核':`已审 ${accepted} / ${assigned} 个分配步骤`;
    const prRows=w.deliveries.map(p=>{const review=w.reviews.find(r=>r.pr===p.pr),state=review?({needs_re_review:'HEAD变化 · 需复审',needs_changes:'需修改',offline_accepted:'离线范围通过',accepted:'范围已验收'}[review.state]||'待审核'):'未关联验收';return `<a href="https://github.com/Viper-Boss/RustSimHub/pull/${count(p.pr)}" target="_blank" rel="noopener">PR #${count(p.pr)} · ${escapeHTML(state)}</a>`;}).join('');
    return `<article class="team-card team-tone-${index}"><div class="team-person"><span class="team-avatar" aria-hidden="true">${['V','A','B'][index]||'·'}</span><div><h3>${github}</h3><span>${escapeHTML(m.role)}</span></div><b class="team-state">${escapeHTML(labels[m.state]||'状态待核对')}</b></div><div class="team-code"><strong>${number(product)}</strong><span>主线产品非空行</span><small>当前产品代码归属 ${pct.toFixed(1)}%</small></div><div class="team-share" aria-label="当前产品代码归属比例"><i style="width:${pct.toFixed(3)}%"></i></div><dl class="team-numbers"><div><dt>独立测试行</dt><dd>${number(s.current_lines.tests)}</dd></div><div><dt>工具 / 配置行</dt><dd>${number(count(s.current_lines.tooling)+count(s.current_lines.firmware))}</dd></div><div><dt>主线提交（含文档）</dt><dd>${number(s.commits)}</dd></div><div><dt>累计增 / 删（含空行）</dt><dd>+${number(s.added)} / −${number(s.deleted)}</dd></div></dl><div class="team-work"><strong>${escapeHTML(work)}</strong><p>${escapeHTML(m.package)}</p><div class="team-work-track"><i style="width:${assigned?Math.min(100,accepted/assigned*100):0}%"></i></div><span>计划：${m.estimate_hours?m.estimate_hours.map(number).join('–')+'小时':'主线持续推进'} · 实耗：${w.self_reported_hours===null?'未填报':escapeHTML(w.self_reported_hours)+'小时（自报）'}</span></div><div class="team-prs">${prRows||'<span>尚无本仓库PR交付</span>'}</div><p class="team-access">${escapeHTML(m.access)}</p><a class="team-task" href="${escapeHTML(publicURL(m.handoff))}" target="_blank" rel="noopener">查看任务书与交付要求 ↗</a></article>`;
  }).join('');
}

function initialize() {
  let data=JSON.parse(document.getElementById('progress-data').textContent);
  let scope='all', shown=[], selectedModule=null, selectedFocus=null;
  const byId=id=>document.getElementById(id), html=escapeHTML;
  const dialog=byId('module-dialog');
  const safeStorage={get(key){try{return localStorage.getItem(key);}catch{return null;}},set(key,value){try{localStorage.setItem(key,value);}catch{}}};
  let paused=safeStorage.get('rsh-progress-motion')==='paused';
  const motionMedia=window.matchMedia('(prefers-reduced-motion: reduce)');
  const date=value=>new Date(value).toLocaleString('zh-CN',{year:'numeric',month:'2-digit',day:'2-digit',hour:'2-digit',minute:'2-digit',hour12:false});
  function toast(message) {const node=byId('toast');node.textContent=message;node.hidden=false;clearTimeout(toast.timer);toast.timer=setTimeout(()=>{node.hidden=true;},4500);}
  function motion() {document.documentElement.classList.toggle('motion-paused',paused);byId('motion-toggle').setAttribute('aria-pressed',String(paused));byId('motion-toggle').title=paused?'恢复装饰动画':'暂停装饰动画';}
  motion();
  byId('motion-toggle').addEventListener('click',()=>{paused=!paused;safeStorage.set('rsh-progress-motion',paused?'paused':'running');motion();});
  const savedCapacity=Number(safeStorage.get('rsh-progress-capacity'));
  if([4,8,12,16,24].includes(savedCapacity))byId('capacity').value=String(savedCapacity);
  function renderEstimate() {const value=calculate(data),capacity=Number(byId('capacity').value),days=estimateDays(value.hours,capacity);byId('hours-range').textContent=value.hours.map(number).join('–');byId('days-range').textContent=`按${capacity}小时/日约${days[0]}–${days[1]}天 · 等待另计`;}
  byId('capacity').addEventListener('change',()=>{safeStorage.set('rsh-progress-capacity',byId('capacity').value);renderEstimate();});
  function renderSummary() {
    const value=calculate(data);
    byId('overall-value').textContent=number(value.original);byId('overall-gauge').style.setProperty('--progress',value.original+'%');
    byId('original-remaining').textContent=number(100-value.original);byId('original-bar').style.width=value.original+'%';
    byId('sprint-percent').textContent=value.weighted.toFixed(1);byId('sprint-bar').style.width=value.weighted+'%';
    byId('done-fraction').textContent=`${value.done} / ${value.total} 已完成`;byId('remaining-count').textContent=value.remaining;byId('parked-count').textContent=`另${value.parked}项搁置`;
    byId('module-count').textContent=`${data.modules.filter(m=>m.scope!=='parked').length}个当前模块 · ${value.offline}项可离线推进`;
    byId('snapshot-time').textContent='更新于 '+date(data.updated_at);byId('baseline-label').textContent=data.baseline_version;
    byId('focus-id').textContent=data.focus.task_id;byId('focus-title').textContent=data.focus.title;byId('focus-description').textContent=data.focus.description;
    byId('focus-flags').innerHTML=`<span class="tag mint">${html(data.focus.state)}</span><span class="tag">凭实际证据推进</span>`;
    byId('facts').innerHTML=data.facts.map(f=>`<div class="fact"><strong>${html(f.value)}</strong><span>${html(f.label)}</span><small>${html(f.detail)}</small></div>`).join('');
    byId('milestones').innerHTML=data.original_milestones.map(m=>`<div class="milestone"><div class="milestone-top"><span>${html(m.id)}</span><strong>${number(m.done_points)}<span>/${m.weight}</span></strong></div><div class="milestone-bar"><i style="--amount:${m.done_points/m.weight*100}%"></i></div><div class="milestone-name">${html(m.label)}</div><div class="milestone-percent">${number(m.weight-m.done_points)}点待验收</div></div>`).join('');
    byId('constraints').innerHTML=data.constraints.map(value=>`<li>${html(value)}</li>`).join('');
    byId('estimate-note').textContent=data.estimate.method+' '+data.estimate.confidence+'。';
    byId('activity-list').innerHTML=[...data.activity].reverse().slice(0,12).map(item=>`<li class="activity-item"><time datetime="${html(item.at)}">${html(date(item.at))}</time><strong>${html(item.title)}</strong><p>${html(item.detail)}</p></li>`).join('');
    byId('footer-status').textContent=`${value.total}个当前动作 · ${data.modules.length}个模块 · 仓库快照，非设备实时遥测`;
    if(byId('team-grid')){
      byId('team-grid').innerHTML=teamHTML(data.team);
      byId('team-source').textContent=data.team?`Git统计截面 ${data.team.source_revision.slice(0,12)} · ${date(data.team.generated_at)} · 产品 / 测试 / 工具分别计数；未映射产品作者 ${number(data.team.unmapped.current_lines.product)}行`:'团队统计待生成';
    }
    renderEstimate();renderRoute();renderModules();
  }
  function renderRoute() {
    const phases=[['usable','01 / FUNCTION FIRST','把常用功能先交付','转发、独立首配、日常电话/短信、网络与NAS接入。'],['extensions','02 / REACH FURTHER','补齐远程与应急入口','NKNGuard、TURN、使用统计和BLE；保持独立授权与开关。'],['release','03 / PROVE THE SYSTEM','走完真正的发行验收','失败恢复、最终镜像、灯光、安全、长测与第二台复验。']];
    byId('route-grid').innerHTML=phases.map(([phase,code,title,description])=>{
      const tasks=data.modules.filter(m=>m.phase===phase).flatMap(m=>m.tasks),remaining=tasks.filter(t=>t.status!=='done');
      const hours=[0,1].map(i=>remaining.reduce((s,t)=>s+t.estimate_hours[i],0));
      return `<article class="route-card"><div class="route-number"><span>${code}</span><span>↗</span></div><h3>${title}</h3><p>${description}</p><div class="route-meta"><span>${tasks.length}个动作</span><span>·</span><span>${tasks.length-remaining.length}个完成</span><span>·</span><span>余${hours.map(number).join('–')}h</span></div></article>`;
    }).join('');
  }
  function renderModules() {
    shown=selectModules(data,{scope,query:byId('search').value,condition:byId('task-filter').value});
    const count=shown.reduce((sum,row)=>sum+row.matches.length,0);
    byId('result-count').textContent=`${shown.length}个模块 · ${count}个匹配动作`;
    byId('empty-state').hidden=shown.length!==0;
    byId('module-grid').innerHTML=shown.map(({module:m,matches})=>{
      const done=m.tasks.filter(t=>t.status==='done').length,remaining=m.tasks.filter(t=>t.status!=='done');
      const hours=[0,1].map(i=>remaining.reduce((sum,t)=>sum+t.estimate_hours[i],0));
      const current=m.tasks.some(t=>t.id===data.focus.task_id),assisted=remaining.filter(t=>t.needs.some(n=>n!=='device')).length;
      const badge={original:'原计划收尾',extension:'新增扩展',parked:'搁置 / 按需'}[m.scope];
      return `<article class="module-card ${current?'current':''} ${m.scope==='parked'?'parked':''}"><div class="module-head"><span class="module-code">${html(m.id)} ${current?' / NEXT':''}</span><span class="module-badge">${badge}</span></div><h3>${html(m.title)}</h3><p>${html(m.summary)}</p><div class="module-numbers"><span><strong>${m.tasks.length}</strong> 个动作 <span class="done-text">/ ${done}完成</span></span><span class="eta">${hours.map(number).join('–')} h</span></div><div class="module-progress"><i style="--amount:${done/m.tasks.length*100}%"></i></div><div class="module-foot"><span>${m.scope==='parked'?'不计当前冲刺分母':assisted?`后续${assisted}项需外部配合`:'可继续离线 / 实机推进'}</span><button data-module="${html(m.id)}">展开${matches.length===m.tasks.length?m.tasks.length:matches.length+'项匹配 / 共'+m.tasks.length}个动作 ↗</button></div></article>`;
    }).join('');
  }
  function openModule(key, focus=null) {
    selectedModule=data.modules.find(m=>m.id===key);if(!selectedModule)return;
    selectedFocus=focus;
    const m=selectedModule,taskMap=new Map(data.modules.flatMap(x=>x.tasks).map(t=>[t.id,t]));
    const selection=shown.find(row=>row.module.id===key),filtered=(byId('search').value.trim()||byId('task-filter').value!=='all')&&!focus;
    const tasks=filtered&&selection?selection.matches:m.tasks;
    byId('dialog-code').textContent=`${m.id} / ${m.scope==='original'?'原计划收尾':m.scope==='extension'?'新增扩展':'搁置 / 按需'}`;
    byId('dialog-title').textContent=m.title;byId('dialog-summary').textContent=m.summary;
    const done=m.tasks.filter(t=>t.status==='done').length;
    byId('dialog-stat').innerHTML=`<span class="tag mint">${done} / ${m.tasks.length}个完成</span><span class="tag">${filtered?'当前显示'+tasks.length+'个匹配动作':'按每个动作的验收条件更新'}</span>`;
    byId('dialog-paths').textContent=m.paths.join('\n');byId('dialog-source').href=publicURL(m.source);
    byId('task-list').innerHTML=tasks.map(t=>{
      const dependencyText=t.dependencies.length?t.dependencies.map(id=>`${id} ${taskMap.get(id)?.status==='done'?'✓':'待完成'}`).join('、'):'无跨动作硬依赖';
      const needs=t.needs.length?t.needs.map(n=>needLabels[n]||n).join('、'):'可由代理离线推进';
      const state=t.status==='todo'&&!dependencyReady(t,taskMap)?'前置待完成':statusLabels[t.status];
      return `<details id="task-${html(t.id)}" class="task ${t.status==='done'?'done':''} ${focus===t.id?'focused':''}" ${focus===t.id?'open':''}><summary><span class="task-index">${html(t.id)}</span><span class="task-title">${html(t.title)}</span><span class="task-status">${html(state)}</span></summary><div class="task-body"><div class="task-row"><b>动作</b><p>${html(t.work)}</p></div><div class="task-row"><b>验收</b><p>${html(t.acceptance)}</p></div><div class="task-row"><b>前置</b><p>${html(dependencyText)}</p></div><div class="task-row"><b>条件</b><p>${html(needs)}</p></div><div class="task-extra"><span class="tag lavender">预计${t.estimate_hours.map(number).join('–')}有效小时</span>${t.passive_wait_hours?`<span class="tag gold">另有${t.passive_wait_hours}小时观测</span>`:''}<span class="tag">${t.offline_delegable?'可交Claude离线实现':'含实机 / 外部验收条件'}</span>${t.actual_hours!==null?`<span class="tag mint">实耗${number(t.actual_hours)}h</span>`:''}</div>${t.evidence.length?`<div class="task-row"><b>证据</b><p>${t.evidence.map(e=>`<a class="task-link" href="${html(publicURL(e))}" target="_blank" rel="noopener">${html(e.split('/').pop())} ↗</a>`).join('<br>')}</p></div>`:''}</div></details>`;
    }).join('');
    if(!dialog.open)dialog.showModal();document.body.style.overflow='hidden';
    if(focus)requestAnimationFrame(()=>byId('task-'+focus)?.scrollIntoView({block:'center',behavior:paused||motionMedia.matches?'auto':'smooth'}));
    else dialog.scrollTop=0;
  }
  byId('module-grid').addEventListener('click',event=>{const target=event.target.closest('[data-module]');if(target)openModule(target.dataset.module);});
  const openFocus=()=>{const module=data.modules.find(m=>m.tasks.some(t=>t.id===data.focus.task_id));if(module)openModule(module.id,data.focus.task_id);};
  byId('focus-open').addEventListener('click',openFocus);byId('focus-detail').addEventListener('click',openFocus);
  byId('dialog-close').addEventListener('click',()=>dialog.close());dialog.addEventListener('close',()=>{document.body.style.overflow='';selectedModule=null;selectedFocus=null;});
  dialog.addEventListener('click',event=>{if(event.target===dialog){const rect=dialog.getBoundingClientRect();if(event.clientX<rect.left||event.clientX>rect.right||event.clientY<rect.top||event.clientY>rect.bottom)dialog.close();}});
  byId('copy-handoff').addEventListener('click',async()=>{if(!selectedModule)return;try{await navigator.clipboard.writeText(handoffText(selectedModule));toast('已复制完整模块说明，可交给Claude离线接手。');}catch{toast('当前环境不能写剪贴板，可下载完整Markdown清单。');}});
  document.querySelectorAll('[data-scope]').forEach(button=>button.addEventListener('click',()=>{scope=button.dataset.scope;document.querySelectorAll('[data-scope]').forEach(x=>{x.classList.toggle('selected',x===button);x.setAttribute('aria-pressed',String(x===button));});renderModules();}));
  byId('search').addEventListener('input',renderModules);byId('task-filter').addEventListener('change',renderModules);
  document.addEventListener('keydown',event=>{if(event.key==='/'&&!dialog.open&&!['INPUT','TEXTAREA','SELECT'].includes(document.activeElement.tagName)){event.preventDefault();byId('search').focus();}});
  byId('export-json').addEventListener('click',()=>{const blob=new Blob([JSON.stringify(data,null,2)+'\n'],{type:'application/json;charset=utf-8'}),url=URL.createObjectURL(blob),anchor=document.createElement('a');anchor.href=url;anchor.download='RustSimHub-remaining-'+data.updated_at.slice(0,10)+'.json';anchor.click();setTimeout(()=>URL.revokeObjectURL(url),1000);});
  byId('refresh-data').addEventListener('click',async()=>{
    if(location.protocol==='file:'){toast('这是可离线阅读的单文件快照；请打开GitHub Pages读取更新。');return;}
    const button=byId('refresh-data');button.disabled=true;button.textContent='读取中…';
    try{const response=await fetch(new URL('progress.json',location.href),{cache:'no-store',credentials:'omit'});if(!response.ok)throw new Error('snapshot unavailable');const next=await response.json();if(next.schema!==1||next.project?.repository!==REPOSITORY||!Array.isArray(next.modules))throw new Error('snapshot shape');calculate(next);data=next;renderSummary();toast('已读取仓库最新公开快照。');}
    catch{toast('暂时未能读取更新，保留当前快照和进度。');}
    finally{button.disabled=false;button.textContent='读取最新进度 ↻';}
  });
  renderSummary();
}

if(typeof document!=='undefined')initialize();
