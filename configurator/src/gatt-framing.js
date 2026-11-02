// SPDX-License-Identifier: AGPL-3.0-only
// Frozen T5 RB v1 framing. Conservative ATT MTU 23 / attribute 20 bytes;
// fragment IDs are transport counters and never cryptographic nonces.
const fail=()=>{throw Error('protocol_error');};
export function fragments(kind,id,wire){
 if(![1,2,3].includes(kind)||!Number.isInteger(id)||id<1||id>65535||!(wire instanceof Uint8Array)||wire.length<1||wire.length>2048)fail();
 const count=Math.ceil(wire.length/8),frames=[];
 for(let i=0;i<count;i++){
  const frame=new Uint8Array(12+Math.min(8,wire.length-i*8)),view=new DataView(frame.buffer);
  frame.set([82,66,1,kind]);[id,i,count,wire.length].forEach((v,j)=>view.setUint16(4+j*2,v,true));frame.set(wire.subarray(i*8,i*8+8),12);frames.push(frame);
 }
 return frames;
}
export class Receiver {
 constructor(clock=()=>performance.now()){this.clock=clock;this.next=1;this.partial=null;this.closed=false;this.last=-Infinity;}
 close(){this.closed=true;if(this.partial)this.partial.bytes.fill(0);this.partial=null;}
 receive(kind,frame){
  try{
   const now=this.clock();if(this.closed||!Number.isFinite(now)||now<this.last)fail();this.last=now;
   if(!(frame instanceof Uint8Array)||frame.length<=12||frame.length>20||![1,3].includes(kind)||frame[0]!==82||frame[1]!==66||frame[2]!==1||frame[3]!==kind)fail();
   const view=new DataView(frame.buffer,frame.byteOffset,frame.byteLength),[id,index,count,total]=[4,6,8,10].map(i=>view.getUint16(i,true));
   if(!total||total>2048||count!==Math.ceil(total/8)||index>=count||frame.length-12!==Math.min(8,total-index*8))fail();
   if(!this.partial){if(id!==this.next||index!==0)fail();this.partial={id,kind,count,total,index:0,deadline:now+15000,bytes:new Uint8Array(total)};this.next++;}
   const p=this.partial;if(p.id!==id||p.kind!==kind||p.count!==count||p.total!==total||p.index!==index||now>=p.deadline)fail();
   p.bytes.set(frame.subarray(12),index*8);p.index++;if(p.index<count)return null;
   this.partial=null;return p.bytes;
  }catch(error){this.close();throw error;}
 }
}
