// SPDX-License-Identifier: AGPL-3.0-only
// Public, redacted display only: never network discovery, authentication or recovery authority.
const enumValue=(value,allowed)=>allowed.includes(value)?value:'unknown';
const uuid=value=>typeof value==='string'&&/^[0-9a-f]{8}(?:-[0-9a-f]{4}){3}-[0-9a-f]{12}$/.test(value);
export function diagnosticSummary(value={}){
 const input=value&&typeof value==='object'?value:{};
 let scheme='unknown',address_valid=false;
 if(typeof input.origin==='string'&&input.origin.length<=2048)try{
  const url=new URL(input.origin);
  if(['https:','http:'].includes(url.protocol)&&!url.username&&!url.password&&!url.search&&!url.hash&&url.pathname==='/'){
   scheme=url.protocol.slice(0,-1);address_valid=true;
  }
 }catch{}
 const original=input.operation&&typeof input.operation==='object'?input.operation:{};
 const id=uuid(original.id)?original.id:null;
 return {format:1,address:{valid:address_valid,scheme,host:'redacted'},
  device_pin_present:typeof input.device_pin==='string'&&/^[0-9a-f]{64}$/.test(input.device_pin),
  certificate:enumValue(input.certificate,['trusted','untrusted']),
  session:enumValue(input.session,['authenticated','expired','denied']),
  remote_path:enumValue(input.remote_path,['direct','relay','unreachable']),
  business:enumValue(input.business,['available','unavailable']),
  operation:id?{id,type:enumValue(original.type,['sms','call','wifi','forwarding']),
   state:enumValue(original.state,['prepared','submitting','submitted','cancelled','unknown','starting','connecting','ready','saving','saved','rolled_back']),
   submit_started:typeof original.submit_started==='boolean'?original.submit_started:null}:null,
  safety:'Only a query of the original operation is available here; no submit, archive or retry.'};
}
export function diagnosticText(snapshot){return JSON.stringify(diagnosticSummary(snapshot),null,2);}
export class DiagnosticPanel {
 constructor({document,root,queryOperation,copy}){
  this.root=root;this.queryOperation=queryOperation;this.copy=copy;this.epoch=0;this.busy=false;
  this.output=document.createElement('pre');this.output.style.whiteSpace='pre-wrap';this.output.style.overflowWrap='anywhere';
  this.message=document.createElement('p');this.message.setAttribute('role','status');
  this.queryButton=document.createElement('button');this.queryButton.type='button';this.queryButton.textContent='只查询原操作';this.queryButton.onclick=()=>this.query();
  this.copyButton=document.createElement('button');this.copyButton.type='button';this.copyButton.textContent='复制脱敏诊断';this.copyButton.onclick=()=>this.copyResult();
  const guidance=document.createElement('p');guidance.textContent='先核对设备地址、证书和可信管理渠道提供的完整设备公钥，再登录核对业务状态。诊断隐藏主机和公钥；不能用于配对或批准身份。';
  root.replaceChildren(this.output,this.queryButton,this.copyButton,this.message,guidance);this.reset();
 }
 update(snapshot){this.epoch++;this.controller?.abort();this.busy=false;this.summary=diagnosticSummary(snapshot);this.output.textContent=JSON.stringify(this.summary,null,2);this.message.textContent='证书、登录、业务和远程路径分别展示；未知字段保持 unknown';this.render();}
 render(){this.queryButton.disabled=this.busy||!this.summary?.operation||typeof this.queryOperation!=='function';this.copyButton.disabled=!this.summary||typeof this.copy!=='function';}
 async query(){
  if(this.queryButton.disabled)return;
  const id=this.summary.operation.id,epoch=this.epoch;this.busy=true;this.controller=new AbortController();const controller=this.controller;this.render();
  let timer;const timeout=new Promise((_,reject)=>{timer=setTimeout(()=>{reject(Error('deadline'));controller.abort();},3000);});
  try{
   const snapshot=await Promise.race([this.queryOperation(id,{signal:controller.signal}),timeout]);
   if(epoch!==this.epoch)return;
   const result=diagnosticSummary(snapshot);
   if(result.operation?.id!==id)throw Error('identity mismatch');
   this.summary=result;this.output.textContent=JSON.stringify(result,null,2);this.message.textContent='已查询原操作；本页没有提交、归档或重试';
  }catch{if(epoch===this.epoch)this.message.textContent='原操作未确认，请保留原记录；没有自动提交或重试';}
  finally{clearTimeout(timer);if(epoch===this.epoch){this.busy=false;this.render();}}
 }
 async copyResult(){if(this.copyButton.disabled)return;const epoch=this.epoch;try{await this.copy(JSON.stringify(this.summary,null,2));if(epoch===this.epoch)this.message.textContent='已复制脱敏诊断；不含地址、公钥、口令或业务正文';}catch{if(epoch===this.epoch)this.message.textContent='复制未完成，请手动选择上方脱敏文本';}}
 reset(){this.epoch++;this.controller?.abort();this.busy=false;this.summary=null;this.output.textContent='等待授权的宿主页提供诊断';this.message.textContent='';this.render();}
}
