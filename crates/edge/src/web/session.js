// SPDX-License-Identifier: AGPL-3.0-only
// Continuation proves key possession; it never renews recent-owner authority.
export class SessionKeeper {
 constructor({env=globalThis,token,renew,apply=()=>{throw Error('登录凭证无法更新');}}){this.env=env;this.token=token;this.renew=renew;this.apply=apply;this.generation=0;this.reset();}
 now(){return this.env.performance?.now()??Date.now();}
 reset(){++this.generation;this.cancel?.();this.cancel=null;this.sessionToken=null;this.deadline=0;this.failed=false;this.flight=null;this.env.clearTimeout(this.timer);}
 start(token,seconds){this.reset();if(!/^[a-f0-9]{64}$/.test(token)||seconds!==900)throw Error('登录有效期响应无效');this.sessionToken=token;this.deadline=this.now()+seconds*1000;}
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
   if(result?.renewed!==true||result.expires_in_seconds!==900||result.recent_owner_login_refreshed!==false||typeof result.token!=='string'||!(/^[a-f0-9]{64}$/.test(result.token))||result.token===token)throw Error('通话登录续期未获确认');
   // Start-of-request accounting is conservative despite a delayed reply.
   const end=started+900000;if(this.now()>=end)throw Error('通话登录续期已过期');this.apply(token,result.token);if(this.token()!==result.token)throw Error('登录凭证更新未确认');this.sessionToken=result.token;this.deadline=end;
  }).catch(error=>{if(g===this.generation)this.failed=true;throw error;}).finally(()=>{if(g===this.generation){this.env.clearTimeout(this.timer);this.flight=null;this.cancel=null;}});
  return this.flight;
 }
}
