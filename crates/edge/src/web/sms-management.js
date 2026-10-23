// SPDX-License-Identifier: AGPL-3.0-only
export class SmsManager {
 constructor({document,request,reloadInbox,confirm}){
  this.doc=document;this.request=request;this.reloadInbox=reloadInbox;this.confirm=confirm;this.$=id=>document.getElementById(id);this.generation=0;
  this.$('delete-sms').onclick=()=>void this.deleteSelected();
  this.$('trash-refresh').onclick=()=>void this.loadTrash(true);
  this.$('more-trash').onclick=()=>void this.loadTrash();this.reset();
 }
 reset(){this.generation++;this.available=false;this.selected=null;this.cursor=null;this.active=null;this.loading=null;this.uncertain=false;this.preview=0;this.restoreButtons=new Set();this.$('trash-section').hidden=true;this.$('trash-list').replaceChildren();this.$('trash-detail').textContent='';this.$('sms-management-state').textContent='删除和恢复需要所有者最近5分钟内登录';this.$('more-trash').disabled=true;this.update();}
 current(g){if(g!==this.generation)throw Error('短信管理会话已结束');}
 update(){this.$('delete-sms').disabled=!this.available||!this.selected||!!this.active||!!this.loading||this.uncertain;this.$('trash-refresh').disabled=!this.available||!!this.active||!!this.loading;for(const button of this.restoreButtons)button.disabled=!this.available||!!this.active||!!this.loading||this.uncertain;}
 select(id){this.selected=id;this.update();}
 async permissions(){const g=this.generation;try{const r=await this.request({op:'sms_management_status'});this.current(g);this.available=r.available===true;this.$('trash-section').hidden=!this.available;this.update();}catch{if(g===this.generation){this.available=false;this.$('trash-section').hidden=true;this.update();}}}
 async loadTrash(reset=false,internal=false){
  if(!this.available||this.loading||this.active&&!internal)return;
  const g=this.generation,marker={};this.loading=marker;this.update();
  try{const page=await this.request({op:'deleted_inbox',after:reset?null:this.cursor,limit:16});this.current(g);
   if(!Array.isArray(page.messages)||page.messages.length>16||!page.cursor||typeof page.cursor.seq!=='string'||typeof page.cursor.db_epoch!=='string')throw Error('回收站响应无效');
   const rows=[],buttons=[];
   for(const sms of page.messages){if(typeof sms.id!=='string'||sms.id.length!==36)throw Error('回收站记录无效');
    const row=this.doc.createElement('div'),label=this.doc.createElement('span'),view=this.doc.createElement('button'),restore=this.doc.createElement('button');row.className='row';label.textContent=`${sms.id} · SIM 代次 ${sms.line}`;view.textContent='查看';restore.textContent='恢复';view.onclick=()=>void this.showDeleted(sms.id);restore.onclick=()=>void this.restore(sms.id);row.append(label,view,restore);rows.push(row);buttons.push(restore);
   }
   if(reset){this.$('trash-list').replaceChildren();this.$('trash-detail').textContent='';this.preview++;this.restoreButtons.clear();}
   this.$('trash-list').append(...rows);for(const button of buttons)this.restoreButtons.add(button);this.cursor=page.cursor;this.$('more-trash').disabled=page.messages.length<16;this.$('sms-management-state').textContent=page.messages.length?'回收站已更新':'本页没有已删除的短信';return true;
  }catch(error){if(g===this.generation)this.$('sms-management-state').textContent=`回收站读取失败：${error.message||'请重新登录后刷新'}`;return false;}
  finally{if(this.loading===marker){this.loading=null;this.update();}}
 }
 async showDeleted(id){const g=this.generation,preview=++this.preview;try{const detail=await this.request({op:'deleted_sms_detail',sms_id:id});this.current(g);if(preview===this.preview)this.$('trash-detail').textContent=JSON.stringify(detail,null,2);}catch(error){if(g===this.generation&&preview===this.preview){this.$('trash-detail').textContent='';this.$('sms-management-state').textContent=error.message||'回收站详情暂不可用';}}}
 async deleteSelected(){const id=this.selected;if(!this.available||!id||this.active||this.loading||this.uncertain)return;if(!this.confirm('将这条短信移到棒子本地回收站？其他客户端也会隐藏它，SIM 存储保持不变。'))return;await this.change(id,true);}
 async restore(id){if(!this.available||this.active||this.loading||this.uncertain)return;if(!this.confirm('恢复这条短信，让其他客户端重新可见？'))return;await this.change(id,false);}
 async change(id,deleted){
  const g=this.generation,marker={};let confirmed=false;this.active=marker;this.update();
  try{const result=await this.request({op:deleted?'delete_sms':'restore_sms',sms_id:id});this.current(g);if(result.sms_id!==id||result.deleted!==deleted)throw Error('操作响应不匹配');confirmed=true;this.selected=null;this.$('sms-detail').textContent='';this.$('trash-detail').textContent='';this.preview++;
   await this.reloadInbox();this.current(g);if(!await this.loadTrash(true,true))throw Error('回收站刷新未完成');this.current(g);this.$('sms-management-state').textContent=deleted?'已移到本地回收站；其他客户端补同步后会隐藏该记录':'已恢复；其他客户端补同步后可重新查看';
  }catch(error){if(g===this.generation){this.uncertain=!confirmed;this.$('sms-management-state').textContent=confirmed?'操作已确认，列表刷新未完成；请刷新核对':`操作结果未确认：${error.message||'连接中断'}。请刷新状态核对；不会自动重试`;}}
  finally{if(this.active===marker){this.active=null;this.update();}}
 }
}
