// SPDX-License-Identifier: AGPL-3.0-only
// Included only by the explicit candidate artifact builder. Static initialization
// does not select a device, create a key, grant authority or reconnect a session.
import {mount} from './app.js';
import init,{RecoverySessionClient} from '../wasm/rustsimhub_recovery_session.js';
import {createRecoveryBackend} from './production-backend.js';
import {getOrCreateBinding,readBinding,rememberGrant} from './security/recovery-binding.js';
const $=id=>document.getElementById(id);
async function start(){
 await init({module_or_path:new URL('../wasm/rustsimhub_recovery_session_bg.wasm',import.meta.url)});
 const backend=createRecoveryBackend({wasm:{RecoverySessionClient},realmForDevice:async pin=>{
  const realm=$('binding-realm').value,record=await readBinding(pin,realm);
  if(!record?.grant?.active)throw Error('not_preapproved');return realm;
 }});
 mount({client:backend});
 $('binding-key').onclick=async()=>{
  $('binding-public').value='';try{const b=await getOrCreateBinding($('pin').value,$('binding-realm').value);$('binding-public').value=b.public_key;$('binding-state').textContent='请在原可信管理入口登记此独立公钥；本页不能批准恢复权限。';}catch{$('binding-state').textContent='独立身份读取或存储未确认；请核对可信标识和浏览器支持。';}
 };
 $('binding-save').onclick=async()=>{
  try{const pin=$('pin').value,realm=$('binding-realm').value,b=await readBinding(pin,realm);if(!b)throw Error();
   await rememberGrant(pin,realm,b.revision,{device_pin:pin,realm,public_key:b.public_key,grant:{id:$('binding-grant').value,generation:$('binding-generation').value,active:true}});
   $('binding-state').textContent='登记回执已保存在本机；设备连接时仍会验证真正授权、私钥、有效期和撤销状态。';
  }catch{$('binding-state').textContent='原授权回执未能保存；不会连接或请求设备操作。';}
 };
}
start().catch(()=>{$('state').textContent='生产恢复候选不可用；请检查浏览器支持与完整静态资源。没有选择设备或提交操作。';});
