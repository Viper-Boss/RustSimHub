// SPDX-License-Identifier: AGPL-3.0-only
// Source-specific, read-only projections. Never infer process state from saved intent.
export function configurationText(source,data,age){
 if(!Number.isFinite(age)||age<0||age>5000||!data||typeof data!=='object')return '配置证据已陈旧或未知；保存和进程状态均需重新查询';
 if(source==='forwarding'){const saved=data.configuration_applied===true?'本次保存意图已确认':'本次保存意图尚未确认';const process=data.process_state_verified===true&&data.enabled===true&&data.state==='running'?'服务运行已核对':data.process_state_verified===true&&data.enabled===false&&data.state==='disabled'?'服务关闭已核对':'服务进程状态未确认';return `${saved}；${process}`;}
 if(source==='ap-commit'){if(data.authoritative_result===true&&data.settings_saved===true&&data.state==='saved')return '配网设置已保存；其他服务进程需单独查询';if(data.authoritative_result===true&&data.settings_saved===false&&data.state==='restored_ap')return '未保存设置，恢复热点已核对；其他服务未知';return '配网保存结果未确认；请查询原操作';}
 if(source==='ap-trial')return data.authoritative_result===true&&data.ap_return_verified===true&&data.settings_saved===false&&data.state==='ready'?'试连成功且已返回热点；尚未保存设置':'试连结果未确认；没有保存成功证据';
 return '此来源尚无已批准的适配；状态未知';
}
export class ConfigurationState {
 constructor({document,root,read,clock=()=>performance.now()}){this.read=read;this.clock=clock;this.epoch=0;this.output=document.createElement('p');this.output.style.overflowWrap='anywhere';this.output.setAttribute('role','status');this.button=document.createElement('button');this.button.type='button';this.button.textContent='只读查询配置状态';this.button.onclick=()=>this.refresh();root.replaceChildren(this.output,this.button);this.reset();}
 reset(){this.epoch++;clearTimeout(this.timer);this.controller?.abort();this.busy=false;this.snapshot=null;this.output.textContent='等待授权的状态来源';this.button.disabled=typeof this.read!=='function';}
 render(){const s=this.snapshot;this.output.textContent=s?configurationText(s.source,s.data,s.age_ms+this.clock()-s.received):'配置状态未知';}
 async refresh(){
  if(this.busy||typeof this.read!=='function')return;const epoch=++this.epoch;this.busy=true;this.button.disabled=true;this.controller=new AbortController();const controller=this.controller;clearTimeout(this.timer);let timer;
  try{
   const started=this.clock();if(!Number.isFinite(started))throw Error('deadline');
   const timeout=new Promise((_,reject)=>{timer=setTimeout(()=>{reject(Error('deadline'));controller.abort();},3000);});
   const v=await Promise.race([this.read({signal:controller.signal}),timeout]);if(epoch!==this.epoch)return;
   const received=this.clock(),elapsed=received-started;if(!Number.isFinite(elapsed)||elapsed<0||elapsed>=3000){controller.abort();throw Error('deadline');}
   this.snapshot=v&&typeof v.source==='string'&&Number.isFinite(v.age_ms)&&v.age_ms>=0?{source:v.source,data:Object.fromEntries(['configuration_applied','process_state_verified','enabled','state','authoritative_result','settings_saved','ap_return_verified'].map(k=>[k,v.data?.[k]])),age_ms:v.age_ms,received}:null;
   this.render();if(this.snapshot&&this.snapshot.age_ms<=5000)this.timer=setTimeout(()=>this.render(),5001-this.snapshot.age_ms);
  }catch{if(epoch===this.epoch){this.snapshot=null;this.output.textContent='查询未确认；保存意图不等于进程运行，没有自动重试';}}
  finally{clearTimeout(timer);if(epoch===this.epoch){this.busy=false;this.button.disabled=false;}}
 }
}
