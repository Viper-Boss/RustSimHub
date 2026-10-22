// SPDX-License-Identifier: AGPL-3.0-only
// Read-only catch-up. A cursor becomes visible only after its view has refreshed.
const LIMIT=32, MAX_PAGES=8, MAX_SEQ=9223372036854775807n;
function sequence(value){
 if(typeof value!=='string'||!/^(0|[1-9][0-9]{0,18})$/.test(value)||BigInt(value)>MAX_SEQ)throw Error('事件游标无效，请刷新状态后重试');
 return BigInt(value);
}
function pageCursor(page,after){
 const cursor=page?.cursor;
 if(!Array.isArray(page?.events)||page.events.length>LIMIT||typeof cursor?.db_epoch!=='string'||!cursor.db_epoch||cursor.db_epoch.length>64)throw Error('事件页无效，请刷新状态后重试');
 const end=sequence(cursor.seq);
 let previous=after?sequence(after.seq):0n;
 if(after&&cursor.db_epoch!==after.db_epoch)throw Error('事件库已变化，请刷新状态后重新同步');
 for(const event of page.events){
  const next=sequence(event?.seq);
  if(next<=previous)throw Error('事件顺序无效，请刷新状态后重试');
  previous=next;
 }
 if(previous!==end)throw Error('事件页与游标不匹配，请刷新状态后重试');
 return {db_epoch:cursor.db_epoch,seq:cursor.seq};
}
export class EventSync {
 constructor({request,reload}){this.request=request;this.reload=reload;this.reset();}
 reset(){this.generation=(this.generation||0)+1;this.cursors={sms:null,calls:null};this.active=null;}
 current(generation){if(generation!==this.generation)throw Error('同步已取消');}
 async run(){
  if(this.active)throw Error('补同步正在进行');
  const active={},generation=this.generation,result={};this.active=active;
  try{
   for(const [name,op] of [['sms','inbox_events'],['calls','call_events']]){
    let cursor=this.cursors[name],count=0,more=true;
    for(let i=0;i<MAX_PAGES;i++){
     this.current(generation);
     const page=await this.request({op,after:cursor,limit:LIMIT});this.current(generation);
     cursor=pageCursor(page,cursor);count+=page.events.length;
     if(page.events.length<LIMIT){more=false;break;}
    }
    if(count){this.current(generation);await this.reload[name]();this.current(generation);}
    // A failed fetch/render leaves the previous cursor intact for an explicit retry.
    this.cursors[name]=cursor;result[name]={count,more};
   }
   return result;
  }finally{if(this.active===active)this.active=null;}
 }
}
