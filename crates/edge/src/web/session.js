// SPDX-License-Identifier: AGPL-3.0-only
// Continuation proves key possession; it never renews recent-owner authority.
const validLifetime = seconds => seconds === 900 || seconds === 86400;
const MAX_TAB_SESSION_MS = 86400000;
const tabSessionKey = 'rustsimhub-console-session-v1';
const hexKey = value => typeof value==='string' && /^[a-f0-9]{64}$/.test(value);
export function forgetTabSession(env=globalThis){
 try{env.sessionStorage?.removeItem(tabSessionKey);}catch{}
}
export function saveTabSession({token,pin,clientKey,remaining},env=globalThis,now=Date.now()){
 if(!hexKey(token)||!hexKey(pin)||!hexKey(clientKey)||!Number.isFinite(remaining)||remaining<=0||remaining>MAX_TAB_SESSION_MS)return;
 try{env.sessionStorage?.setItem(tabSessionKey,JSON.stringify({token,pin,clientKey,savedAt:now,expiresAt:now+Math.floor(remaining)}));}catch{}
}
export function readTabSession(pin,clientKey,env=globalThis,now=Date.now()){
 let raw;
 try{raw=env.sessionStorage?.getItem(tabSessionKey);}catch{return null;}
 if(!raw)return null;
 if(raw.length>512){forgetTabSession(env);return null;}
 try{
  const entry=JSON.parse(raw);
  if(hexKey(pin)&&hexKey(clientKey)&&entry.pin===pin&&entry.clientKey===clientKey&&hexKey(entry.token)&&
     Number.isSafeInteger(entry.savedAt)&&Number.isSafeInteger(entry.expiresAt)&&entry.savedAt>=0&&entry.savedAt<=now&&now<entry.expiresAt&&
     entry.expiresAt-entry.savedAt<=MAX_TAB_SESSION_MS)return entry;
 }catch{}
 forgetTabSession(env);
 return null;
}
export class SessionKeeper {
 constructor({env=globalThis,token,renew,apply=()=>{throw Error('登录凭证无法更新');}}){this.env=env;this.token=token;this.renew=renew;this.apply=apply;this.generation=0;this.reset();}
 now(){return this.env.performance?.now()??Date.now();}
 reset(){++this.generation;this.cancel?.();this.cancel=null;this.sessionToken=null;this.deadline=0;this.failed=false;this.flight=null;this.env.clearTimeout(this.timer);}
 start(token,seconds){if(!validLifetime(seconds))throw Error('登录有效期响应无效');this.resume(token,seconds*1000);}
 resume(token,remaining){
  this.reset();
  if(!hexKey(token)||!Number.isFinite(remaining)||remaining<=0||remaining>MAX_TAB_SESSION_MS)throw Error('登录有效期响应无效');
  this.sessionToken=token;this.deadline=this.now()+remaining;
 }
 remaining(){return Math.max(0,this.deadline-this.now());}
 current(g,token){if(g!==this.generation||this.failed||this.sessionToken!==token||this.token()!==token)throw Error('通话登录验证已停止，请重新登录');}
 async ensure(){
  const g=this.generation,token=this.sessionToken;this.current(g,token);
  if(!token)throw Error('请先登录');
  if(this.flight)return this.flight;
  const started=this.now();
  if(started>=this.deadline){this.failed=true;throw Error('登录已过期，请重新登录');}
  if(this.deadline-started>180000)return;
  const timed=new Promise((_,reject)=>{this.cancel=()=>reject(Error('通话登录验证已取消'));this.timer=this.env.setTimeout(()=>reject(Error('通话登录验证超时；已停止续租')),45000);});
  const continuing=(async()=>{const result=await this.renew(token,()=>this.current(g,token));this.current(g,token);return result;})();
  this.flight=Promise.race([continuing,timed]).then(result=>{
   this.current(g,token);
   if(result?.renewed!==true||!validLifetime(result.expires_in_seconds)||result.recent_owner_login_refreshed!==false||typeof result.token!=='string'||!(/^[a-f0-9]{64}$/.test(result.token))||result.token===token)throw Error('通话登录续期未获确认');
   // Start-of-request accounting is conservative despite a delayed reply.
   const end=started+result.expires_in_seconds*1000,remaining=end-this.now();if(remaining<=0)throw Error('通话登录续期已过期');this.apply(token,result.token,remaining);if(this.token()!==result.token)throw Error('登录凭证更新未确认');this.sessionToken=result.token;this.deadline=end;
  }).catch(error=>{if(g===this.generation)this.failed=true;throw error;}).finally(()=>{if(g===this.generation){this.env.clearTimeout(this.timer);this.flight=null;this.cancel=null;}});
  return this.flight;
 }
}
