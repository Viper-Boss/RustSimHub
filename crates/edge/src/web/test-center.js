// SPDX-License-Identifier: AGPL-3.0-only
// Presentation adapter only. The host's existing controllers own all operations.
const tasks=[['check','检查状态'],['outgoing','测试拨出'],['incoming','测试来电'],['sms','测试短信'],['forwarding','测试转发'],['stop','停止当前测试'],['recover','查询原操作']];
const submissions=new Set(['outgoing','incoming','sms','forwarding']);
const styles=`.test-center-component{max-width:100%;min-width:0;overflow-wrap:anywhere}.test-center-component .test-center-actions{display:flex;flex-wrap:wrap;gap:12px}.test-center-component .test-center-task{flex:1 1 180px;min-width:0;max-width:100%}.test-center-component button{font:inherit;max-width:100%;min-height:44px;white-space:normal;overflow-wrap:anywhere}.test-center-component button:focus-visible,.test-center-component summary:focus-visible{outline:3px solid #19b38a;outline-offset:3px}.test-center-component p{white-space:pre-wrap}.test-center-component details{margin-top:16px}`;
export class TestCenter {
 constructor({document,root,actions={},debugElement=null}){
  if(!root||!/^[-a-zA-Z0-9_]+$/.test(root.id||''))throw Error('测试中心需要独立且有 ID 的挂载元素');
  this.doc=document;this.root=root;this.actions=actions&&typeof actions==='object'?actions:{};this.epoch=0;this.sequence=0;this.version=0;this.busy=false;this.stopping=false;this.stopUnconfirmed=false;this.needsReview=false;this.view={};this.feedback='';this.buttons=new Map();this.reasons=new Map();
  root.classList.add('test-center-component');
  const style=document.createElement('style');style.textContent=styles;
  const title=document.createElement('h2');title.textContent='一键测试中心';
  this.status=document.createElement('p');this.status.setAttribute('role','status');this.status.setAttribute('aria-live','polite');
  const group=document.createElement('div');group.className='test-center-actions';
  for(const [id,label] of tasks){
   const box=document.createElement('div');box.className='test-center-task';
   const button=document.createElement('button');button.type='button';button.textContent=label;button.dataset.task=id;
   const reason=document.createElement('p');reason.id=`${root.id}-${id}-reason`;button.setAttribute('aria-describedby',reason.id);
   button.onclick=()=>this.run(id);box.append(button,reason);group.append(box);this.buttons.set(id,button);this.reasons.set(id,reason);
  }
  const advanced=document.createElement('details'),summary=document.createElement('summary');summary.textContent='手动调试';advanced.append(summary);
  if(debugElement)advanced.append(debugElement);
  root.replaceChildren(style,title,this.status,group,advanced);this.render();
 }
 update(view={}){
  view=view&&typeof view==='object'?view:{};
  // Accept display fields only; operation IDs, receipts, secrets and storage stay in the host.
  this.view={message:typeof view.message==='string'?view.message:'',busy:view.busy===true,unresolved:view.unresolved===true,actions:{}};
  for(const [id] of tasks){const a=view.actions?.[id];this.view.actions[id]={enabled:a?.enabled===true,reason:typeof a?.reason==='string'?a.reason:''};}
  this.version++;this.stopUnconfirmed=false;this.needsReview=false;this.feedback='';this.render();
 }
 reason(id){
  if(typeof this.actions[id]!=='function')return '宿主页尚未接入此功能';
  const state=this.view.actions?.[id];
  if(!state?.enabled)return state?.reason||'等待控制器提供可操作状态';
  if(id==='stop')return this.stopUnconfirmed?'停止未确认，请先查询原操作':this.stopping?'停止请求处理中，请等待核对':'';
  if(this.busy||this.view.busy)return '当前操作处理中，请等待或使用停止按钮';
  if(submissions.has(id)&&(this.view.unresolved||this.needsReview))return '已有结果未确认，请查询原操作；不会自动重拨或重发';
  return '';
 }
 render(){
  this.status.textContent=this.feedback||this.view.message||'选择一项测试；尚未读取设备或提交操作';
  for(const [id,button] of this.buttons){const reason=this.reason(id);button.disabled=!!reason;this.reasons.get(id).textContent=reason||'可由你点击开始';}
 }
 async run(id){
  if(!this.buttons.has(id)||this.reason(id))return;
  const epoch=this.epoch,sequence=++this.sequence,version=this.version;
  if(id==='stop')this.stopping=true;else this.busy=true;
  this.feedback=id==='stop'?'已请求停止，等待控制器确认；结果不明的操作仍需核对':'正在执行所选操作；以现有控制器的状态为准';this.render();
  try{
   await this.actions[id]({isCurrent:()=>epoch===this.epoch,update:next=>{if(epoch===this.epoch)this.update(next);}});
   if(epoch!==this.epoch)return;
   if(sequence===this.sequence&&version===this.version)this.feedback=id==='stop'?'停止请求已返回，尚需控制器确认结束':'操作已返回，请核对现有控制器的结果；未自动重试';
  }catch{
   if(epoch!==this.epoch)return;
   this.needsReview=true;if(id==='stop')this.stopUnconfirmed=true;
   if(sequence===this.sequence)this.feedback=id==='stop'?'停止未确认，请查询原操作，勿重新开始测试':'操作未确认，请查询原操作；未自动重拨或重发';
  }finally{
   if(epoch===this.epoch){if(id==='stop')this.stopping=false;else this.busy=false;this.render();}
  }
 }
 reset(){this.epoch++;this.sequence++;this.busy=false;this.stopping=false;this.stopUnconfirmed=false;this.needsReview=false;this.view={};this.feedback='已退出；等待宿主页重新提供状态';this.render();}
 dispose(){this.reset();this.root.replaceChildren();}
}
