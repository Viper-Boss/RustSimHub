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

export function personAssignments(data, team=data?.team) {
  if(!team?.members?.length)return [];
  const active=(data?.modules||[]).filter(m=>m.scope!=='parked').flatMap(m=>m.tasks.map(task=>({task,module:m})));
  const owners=new Map();
  for(const member of team.members.filter(m=>m.id!=='lead'))for(const id of member.task_ids){
    if(owners.has(id))throw new Error('项目动作有重复离线负责人');
    owners.set(id,member.id);
  }
  return team.members.map(member=>{
    const projects=active.filter(row=>(owners.get(row.task.id)||'lead')===member.id);
    const steps=team.task_plan?.members.find(m=>m.member_id===member.id)?.steps||[];
    return {member,projects,steps,remaining:projects.filter(row=>row.task.status!=='done').length,
      completed:projects.filter(row=>row.task.status==='done').length,delegated:owners.size};
  });
}

const stepLabels={not_accepted:'待交付 / 待审',offline_accepted:'离线已审',accepted:'范围已验收'};
const idQuery=term=>/^[a-z][a-z0-9]*-\d{2}$/.test(term);
const stepMatches=(step,term)=>idQuery(term)?[step.id,...step.task_ids].some(id=>id.toLocaleLowerCase()===term):[step.id,step.title,step.work,step.acceptance,...step.task_ids].join(' ').toLocaleLowerCase().includes(term);
const projectMatches=(row,term)=>idQuery(term)?row.task.id.toLocaleLowerCase()===term:[row.task.id,row.task.title,row.task.work,row.task.acceptance,row.module.title].join(' ').toLocaleLowerCase().includes(term);

export function personHandoffText(person) {
  const m=person.member;
  return [`RustSimHub / ${m.name}`,`任务书：${publicURL(m.handoff)}`,
    m.id==='lead'?'主线保留实际执行、签名、审核集成与最终验收；成员动作不重复计数。':'仅实现任务书明确授权的离线子范围。先交当前小PR，接口/迁移/安全提案先审；不能把离线、合并或CI当实机验收。',
    ...person.steps.map(s=>`\n${s.id} [${stepLabels[s.status]}] ${s.title}\n${s.work}${s.acceptance?'\n验收/主线保留：'+s.acceptance:''}\n来源：${publicURL(s.source)}#L${s.line}`),
    '\n涉及的项目动作（仍按全局验收闭环）：',
    ...person.projects.filter(row=>row.task.status!=='done').map(({task:t})=>`${t.id} [${statusLabels[t.status]}] ${t.title}\n动作：${t.work}\n验收：${t.acceptance}`)].join('\n');
}

function personTasksHTML(person,data,query) {
  const {member:m,steps,projects}=person,term=query.trim().toLocaleLowerCase();
  const personMatch=term&&[m.name,m.login,m.role].join(' ').toLocaleLowerCase().includes(term);
  const visibleSteps=steps.filter(s=>!term||personMatch||stepMatches(s,term));
  const visibleProjects=projects.filter(row=>!term||personMatch||projectMatches(row,term));
  if(term&&!visibleSteps.length&&!visibleProjects.length)return null;
  const group=(items,key)=>{const result=new Map();for(const item of items){const k=key(item);if(!result.has(k))result.set(k,[]);result.get(k).push(item);}return [...result];};
  const batches=group(visibleSteps,s=>s.batch);
  const current=batches.find(([_,rows])=>rows.some(s=>s.status==='not_accepted'))?.[0];
  const stepHTML=batches.map(([name,rows])=>`<details class="person-batch" data-person-batch ${term||name===current?'open':''}><summary><span>${escapeHTML(name)}</span><small>${rows.filter(s=>s.status!=='not_accepted').length} / ${rows.length} 已审</small></summary><div class="person-step-list">${rows.map(s=>`<details class="person-step ${s.status==='not_accepted'?'':'reviewed'}" data-step-id="${escapeHTML(s.id)}"><summary><code>${escapeHTML(s.id)}</code><span>${escapeHTML(s.title)}</span><b>${escapeHTML(stepLabels[s.status])}</b></summary><div class="person-step-body"><p>${escapeHTML(s.work)}</p>${s.acceptance?`<h5>包级验收与主线保留</h5><p>${escapeHTML(s.acceptance)}</p>`:''}${s.task_ids.length?`<p class="person-scope">涉及项目动作：${s.task_ids.map(escapeHTML).join('、')}</p>`:''}<a href="${escapeHTML(publicURL(s.source))}#L${s.line}" target="_blank" rel="noopener">核对任务书原文 ↗</a></div></details>`).join('')}</div></details>`).join('');
  const taskMap=new Map((data?.modules||[]).flatMap(module=>module.tasks).map(t=>[t.id,t]));
  const projectHTML=rows=>rows.map(({task:t})=>{
    const state=t.status==='todo'&&!dependencyReady(t,taskMap)?'前置待完成':statusLabels[t.status];
    return `<button type="button" class="person-project" data-team-task="${escapeHTML(t.id)}"><code>${escapeHTML(t.id)}</code><span>${escapeHTML(t.title)}</span><small>${escapeHTML(state)}</small></button>`;
  }).join('');
  const pending=visibleProjects.filter(row=>row.task.status!=='done'),archived=visibleProjects.filter(row=>row.task.status==='done');
  const modules=group(pending,row=>row.module.title);
  const focus=modules.find(([_,rows])=>rows.some(row=>row.task.id===data?.focus?.task_id))?.[0]||modules[0]?.[0];
  const moduleHTML=modules.map(([name,rows])=>`<details class="person-batch project-batch" data-person-batch ${term||m.id==='lead'&&name===focus?'open':''}><summary><span>${escapeHTML(name)}</span><small>${rows.length} 个动作</small></summary><div class="person-project-list">${projectHTML(rows)}</div></details>`).join('');
  const body=m.id==='lead'?`<p class="person-boundary">主线执行 <strong>${person.remaining}</strong> 项；另有 ${person.delegated} 项成员动作需要主线审核/集成，见其个人列表，不重复计数。</p>${steps.length?`<p class="person-boundary">主线接回 <strong>${steps.length}</strong> 个离线交付步骤；仍按实际交付与审核记账。</p><div class="person-batches">${stepHTML}</div>`:''}<div class="person-batches">${moduleHTML}</div>`:
    `<p class="person-boundary"><strong>${steps.length}</strong> 个个人交付步骤 · 涉及 <strong>${projects.length}</strong> 个项目动作。步骤已审和项目闭环分别记录；未验收不等于未开工。</p>${steps.length?`<div class="person-batches">${stepHTML}</div>`:'<p>详细步骤数据待生成，请先查看任务书。</p>'}<details class="person-projects" ${term&&pending.length?'open':''}><summary>涉及的项目动作 · ${pending.length}项未闭环</summary><p class="person-boundary">本人只做任务书划定的离线子范围；下列为全局动作原验收，真实环境仍由主线安排。</p><div class="person-batches">${moduleHTML}</div></details>`;
  return `<div class="person-tasks"><div class="person-tasks-head"><h4>${m.id==='lead'?'主线执行清单':'个人交付清单'}</h4><div><button type="button" data-person-expand="${escapeHTML(m.id)}" aria-expanded="false">显示全部任务</button><button type="button" data-person-copy="${escapeHTML(m.id)}">复制个人清单</button></div></div>${body}${archived.length?`<details class="person-archive" ${term?'open':''}><summary>已闭环项目归档 · ${archived.length}项</summary><p class="person-boundary">归档入口不用于推定代码作者或贡献比例。</p><div class="person-project-list">${projectHTML(archived)}</div></details>`:''}</div>`;
}

export function teamHTML(team, data=null, {query=''}={}) {
  if(!team?.members?.length)return '<p>团队统计尚未生成；不显示推测数值。</p>';
  const count=value=>Number.isSafeInteger(value)&&value>=0?value:0;
  const number=value=>count(value).toLocaleString('zh-CN');
  const total=count(team.totals.product), labels={assigned:'已分配 · 待开工',in_progress:'任务进行中',submitted:'已提交 · 待审核'};
  const people=personAssignments(data,team);
  return team.members.map((m,index)=>{
    const s=m.stats,w=m.work,product=count(s.current_lines.product),pct=total?product/total*100:0;
    const accepted=count(w.accepted_steps),assigned=count(w.assigned_steps);
    const github=m.login?`<a href="https://github.com/${encodeURIComponent(m.login)}" target="_blank" rel="noopener">${escapeHTML(m.name)} ↗</a>`:escapeHTML(m.name);
    const work=index===0?`历史主线 + 集成 / 设备 / 审核${assigned?` · 接回 ${assigned} 步，已审 ${accepted}`:''}`:`已审 ${accepted} / ${assigned} 个分配步骤`;
    const prRows=w.deliveries.map(p=>{const review=w.reviews.find(r=>r.pr===p.pr),state=review?({needs_re_review:'HEAD变化 · 需复审',needs_changes:'需修改',offline_accepted:'离线范围通过',accepted:'范围已验收'}[review.state]||'待审核'):'未关联验收';return `<a href="https://github.com/Viper-Boss/RustSimHub/pull/${count(p.pr)}" target="_blank" rel="noopener">PR #${count(p.pr)} · ${escapeHTML(state)}</a>`;}).join('');
    const tasks=personTasksHTML(people[index],data,query);if(tasks===null)return '';
    const details=`<div class="person-detail-scope" data-person-id="${escapeHTML(m.id)}"><div class="person-detail-overview"><b class="team-state">${escapeHTML(labels[m.state]||'状态待核对')}</b><div class="team-work"><strong>${escapeHTML(work)}</strong><p>${escapeHTML(m.package)}</p><div class="team-work-track"><i style="width:${assigned?Math.min(100,accepted/assigned*100):0}%"></i></div><span>计划：${m.estimate_hours?m.estimate_hours.map(number).join('–')+'小时':'主线持续推进'} · 实耗：${w.self_reported_hours===null?'未填报':escapeHTML(w.self_reported_hours)+'小时（自报）'}</span></div><a class="team-task" href="${escapeHTML(publicURL(m.handoff))}" target="_blank" rel="noopener">任务书与交付要求 ↗</a></div>${tasks}<details class="team-contribution"><summary>代码贡献与PR记录</summary><dl class="team-numbers"><div><dt>独立测试行</dt><dd>${number(s.current_lines.tests)}</dd></div><div><dt>工具 / 配置行</dt><dd>${number(count(s.current_lines.tooling)+count(s.current_lines.firmware))}</dd></div><div><dt>主线提交（含文档）</dt><dd>${number(s.commits)}</dd></div><div><dt>累计增 / 删（含空行）</dt><dd>+${number(s.added)} / −${number(s.deleted)}</dd></div></dl><div class="team-prs">${prRows||'<span>尚无本仓库PR交付</span>'}</div><p class="team-access">${escapeHTML(m.access)}</p></details></div>`;
    return `<article id="person-${escapeHTML(m.id)}" data-person-id="${escapeHTML(m.id)}" class="team-card team-tone-${index}"><div class="team-profile"><div class="team-person"><span class="team-avatar" aria-hidden="true">${escapeHTML(m.id==='lead'?'V':m.id.slice('member-'.length).toUpperCase())}</span><div><h3>${github}</h3><span>${escapeHTML(m.role)}</span></div></div><div class="team-percentage"><strong>${total?pct.toFixed(1):'—'}</strong><span>${total?'%':''}</span><small>主线产品代码占比</small></div><div class="team-column" role="img" aria-label="${escapeHTML(m.name)}：${total?pct.toFixed(1)+'%':'暂无统计'}"><div class="team-column-track"><i style="height:${Math.min(100,pct).toFixed(3)}%"></i></div></div><p class="team-line-count"><strong>${number(product)}</strong> 行产品代码</p><button type="button" class="team-details-button" data-person-details="${escapeHTML(m.id)}" aria-haspopup="dialog" aria-label="查看${escapeHTML(m.name)}的详情">详情 <span aria-hidden="true">↗</span></button></div><template data-person-template="${escapeHTML(m.id)}">${details}</template></article>`;
  }).join('');
}

function initialize() {
  let data=JSON.parse(document.getElementById('progress-data').textContent);
  let scope='all', shown=[], selectedModule=null, selectedFocus=null;
  const byId=id=>document.getElementById(id), html=escapeHTML;
  const dialog=byId('module-dialog');
  const personDialog=byId('person-dialog');
  function renderPeople(){
    const output=teamHTML(data.team,data,{query:byId('team-search')?.value||''});
    byId('team-grid').innerHTML=output||'<p class="person-empty">没有匹配的人员或任务，请换一个关键词。</p>';
    if(byId('team-jumps'))byId('team-jumps').innerHTML=(data.team?.members||[]).map(m=>`<a href="#person-${html(m.id)}" data-person-jump="${html(m.id)}">${html(m.name)}</a>`).join('');
    if(byId('team-result'))byId('team-result').textContent=`${byId('team-grid').querySelectorAll('[data-person-id]').length}位成员 · 任务内容来自任务书，审核状态来自对应HEAD记录`;
  }
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
    byId('mission-visual').setAttribute('aria-label',`原计划进度${number(value.original)}%，目标100%`);
    byId('original-progress-description').textContent=`已有${number(value.original)}点；新增扩展独立跟踪`;
    byId('milestone-title').textContent=`${number(value.original)}%的来处，每一项都保留边界。`;
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
      renderPeople();
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
    const owners=new Map(personAssignments(data).flatMap(p=>p.projects.map(row=>[row.task.id,p.member])));
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
      const owner=owners.get(t.id),ownership=owner?.id==='lead'?(t.status==='done'?'主线项目归档；不据此推定代码作者。':'主线执行；真实环境、签名与最终验收按前置安排。'):owner?`${owner.name}负责任务书明确的离线子范围；主线审核、集成并安排实际环境验收。`:'搁置或按需范围，未分配执行。';
      return `<details id="task-${html(t.id)}" class="task ${t.status==='done'?'done':''} ${focus===t.id?'focused':''}" ${focus===t.id?'open':''}><summary><span class="task-index">${html(t.id)}</span><span class="task-title">${html(t.title)}</span><span class="task-status">${html(state)}</span></summary><div class="task-body"><div class="task-row"><b>分工</b><p>${html(ownership)}</p></div><div class="task-row"><b>动作</b><p>${html(t.work)}</p></div><div class="task-row"><b>验收</b><p>${html(t.acceptance)}</p></div><div class="task-row"><b>前置</b><p>${html(dependencyText)}</p></div><div class="task-row"><b>条件</b><p>${html(needs)}</p></div><div class="task-extra"><span class="tag lavender">动作原估时${t.estimate_hours.map(number).join('–')}有效小时</span>${t.passive_wait_hours?`<span class="tag gold">另有${t.passive_wait_hours}小时观测</span>`:''}<span class="tag">${t.offline_delegable?'含离线实现范围':'含实机 / 外部验收条件'}</span>${t.actual_hours!==null?`<span class="tag mint">实耗${number(t.actual_hours)}h</span>`:''}</div>${t.evidence.length?`<div class="task-row"><b>证据</b><p>${t.evidence.map(e=>`<a class="task-link" href="${html(publicURL(e))}" target="_blank" rel="noopener">${html(e.split('/').pop())} ↗</a>`).join('<br>')}</p></div>`:''}</div></details>`;
    }).join('');
    if(!dialog.open)dialog.showModal();document.body.style.overflow='hidden';
    if(focus)requestAnimationFrame(()=>byId('task-'+focus)?.scrollIntoView({block:'center',behavior:paused||motionMedia.matches?'auto':'smooth'}));
    else dialog.scrollTop=0;
  }
  byId('module-grid').addEventListener('click',event=>{const target=event.target.closest('[data-module]');if(target)openModule(target.dataset.module);});
  byId('team-search')?.addEventListener('input',renderPeople);
  byId('team-jumps')?.addEventListener('click',event=>{
    const link=event.target.closest('[data-person-jump]');if(!link)return;
    event.preventDefault();if(byId('team-search').value){byId('team-search').value='';renderPeople();}
    byId('person-'+link.dataset.personJump)?.scrollIntoView({block:'start',behavior:paused||motionMedia.matches?'auto':'smooth'});
  });
  async function handlePersonAction(event){
    const details=event.target.closest('[data-person-details]');
    if(details){
      const member=data.team?.members.find(m=>m.id===details.dataset.personDetails);
      const template=details.closest('.team-card')?.querySelector('template[data-person-template]');
      if(!member||!template)return;
      byId('person-dialog-title').textContent=member.name+' · 详情与剩余任务';
      byId('person-dialog-body').replaceChildren(template.content.cloneNode(true));
      personDialog.showModal();document.body.style.overflow='hidden';return;
    }
    const task=event.target.closest('[data-team-task]');
    if(task){const module=data.modules.find(m=>m.tasks.some(t=>t.id===task.dataset.teamTask));if(module){personDialog.close();openModule(module.id,task.dataset.teamTask);}return;}
    const expand=event.target.closest('[data-person-expand]');
    if(expand){const card=expand.closest('[data-person-id]'),open=expand.getAttribute('aria-expanded')!=='true';card.querySelectorAll('[data-person-batch],.person-projects').forEach(node=>{node.open=open;});expand.setAttribute('aria-expanded',String(open));expand.textContent=open?'收起任务分组':'显示全部任务';return;}
    const copy=event.target.closest('[data-person-copy]');
    if(copy){const person=personAssignments(data).find(p=>p.member.id===copy.dataset.personCopy);if(!person)return;try{await navigator.clipboard.writeText(personHandoffText(person));toast('已复制本人步骤、项目动作和验收边界。');}catch{toast('当前环境不能写剪贴板，请展开任务或查看任务书。');}}
  }
  byId('team-grid')?.addEventListener('click',handlePersonAction);
  personDialog.addEventListener('click',handlePersonAction);
  byId('person-dialog-close').addEventListener('click',()=>personDialog.close());
  personDialog.addEventListener('close',()=>{if(!dialog.open)document.body.style.overflow='';byId('person-dialog-body').replaceChildren();});
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
