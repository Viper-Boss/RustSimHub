// SPDX-License-Identifier: AGPL-3.0-only
export function hex(bytes){return Array.from(bytes,b=>b.toString(16).padStart(2,'0')).join('');}
export function unhex(value){if(!/^[a-fA-F0-9]{64}$/.test(value))throw Error('请填写并核对 64 位设备公钥');return Uint8Array.from(value.match(/../g),h=>parseInt(h,16));}
function bytes(value,size){if(!Array.isArray(value)||(size&&value.length!==size)||value.some(b=>!Number.isInteger(b)||b<0||b>255))throw Error('挑战格式无效');return new Uint8Array(value);}
const names=['status.read','sms.read','sms.send','calls.read','calls.control','media.use','network.manage','clients.manage','update.manage','system.manage'];
export async function prove(c,pin,publicKey,privateKey,purpose,scopes,expectedKind='client',sessionToken=null){
 if(!c||!/^[a-f0-9]{64}$/.test(c.id)||c.expires_in_seconds!==60||!Array.isArray(c.transcript)||c.transcript.length<1||c.transcript.length>4096)throw Error('挑战格式无效');
 const transcript=bytes(c.transcript),signature=bytes(c.device_signature,64);const device=await crypto.subtle.importKey('raw',pin,{name:'Ed25519'},false,['verify']);
 if(!await crypto.subtle.verify('Ed25519',device,signature,transcript))throw Error('设备签名与核对公钥不匹配');
 const t=JSON.parse(new TextDecoder('utf-8',{fatal:true}).decode(transcript));
 const fields=['domain','purpose','instance','challenge_id','device_public_key','client_public_key','kind','scopes','generation'];
 if(purpose==='session_renew')fields.push('session_digest');
 if(!['client','owner'].includes(expectedKind)||Object.keys(t).sort().join(',')!==fields.sort().join(',')||t.domain!=='RustSimHub.Auth.v1'||t.purpose!==purpose||t.kind!==expectedKind||t.challenge_id!==c.id||!/^[a-f0-9]{64}$/.test(t.instance)||hex(bytes(t.device_public_key,32))!==hex(pin)||hex(bytes(t.client_public_key,32))!==hex(publicKey)||!Number.isSafeInteger(t.scopes)||t.scopes<1||t.scopes>1023||!Number.isSafeInteger(t.generation)||t.generation<0)throw Error('挑战绑定不匹配');
 if(expectedKind==='owner'&&t.scopes!==1023)throw Error('所有者权限绑定无效');
 if(purpose==='bootstrap'&&(expectedKind!=='owner'||t.generation!==0))throw Error('首次所有者绑定无效');
 if(['login','session_renew'].includes(purpose)&&t.generation<1)throw Error('登录代次无效');
 if(purpose==='session_renew'){
  if(typeof sessionToken!=='string'||!/^[a-f0-9]{64}$/.test(sessionToken))throw Error('续期会话无效');
  const digest=new Uint8Array(await crypto.subtle.digest('SHA-256',new TextEncoder().encode(sessionToken)));
  if(hex(bytes(t.session_digest,32))!==hex(digest))throw Error('续期会话绑定不匹配');
 }
 if(purpose==='pair'){if(expectedKind!=='client')throw Error('配对身份无效');let mask=0;for(const scope of scopes){const i=names.indexOf(scope);if(i<0||(mask&(1<<i)))throw Error('配对权限无效');mask|=1<<i;}if(t.scopes!==mask)throw Error('配对权限不匹配');}
 if(!['pair','login','bootstrap','session_renew'].includes(purpose))throw Error('签名用途不支持');
 return Array.from(new Uint8Array(await crypto.subtle.sign('Ed25519',privateKey,transcript)));
}
