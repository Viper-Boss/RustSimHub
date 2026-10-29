// SPDX-License-Identifier: AGPL-3.0-only
export const requestId=s=>typeof s==='string'&&/^[0-9a-f]{8}(?:-[0-9a-f]{4}){3}-[0-9a-f]{12}$/.test(s);
export const pin=s=>typeof s==='string'&&/^[0-9a-f]{64}$/.test(s);
export const actions=['begin_wifi','confirm_wifi','cancel_wifi'];
export const states=['starting','connecting','ready','saving','saved','rolled_back','unknown'];
export const need=(ok,code='protocol_error')=>{if(!ok)throw Error(code);};
const exact=(v,keys)=>v&&typeof v==='object'&&!Array.isArray(v)&&Object.keys(v).length===keys.length&&Object.keys(v).every(k=>keys.includes(k));
const booleanOrNull=v=>v===null||typeof v==='boolean';
export function operation(v,id,action){
 need(exact(v,['contract','source','request_id','action','trial_id','state','authoritative','settings_saved','rollback_verified','expires_in_ms']));
 need(v.contract===1&&v.source==='core'&&v.request_id===id&&requestId(id)&&actions.includes(v.action)&&(!action||v.action===action)&&states.includes(v.state));
 need(v.trial_id===null||pin(v.trial_id));need([v.authoritative,v.settings_saved,v.rollback_verified].every(booleanOrNull));
 need(v.expires_in_ms===null||Number.isSafeInteger(v.expires_in_ms)&&v.expires_in_ms>=0&&v.expires_in_ms<=40000);
 if(v.state==='saved')need(v.authoritative===true&&v.settings_saved===true);
 else if(v.state==='rolled_back')need(v.authoritative===true&&v.settings_saved===false&&v.rollback_verified===true);
 else need(v.settings_saved!==true&&v.rollback_verified!==true);
 return structuredClone(v);
}
export function status(v){need(exact(v,['contract','source','wifi','busy','recovery_available']));need(v.contract===1&&v.source==='core'&&['connected','disconnected','unknown'].includes(v.wifi)&&typeof v.busy==='boolean'&&typeof v.recovery_available==='boolean');return structuredClone(v);}
export function lookup(v,id){need(exact(v,['contract','source','target_request_id','lookup','operation']));need(v.contract===1&&v.source==='core'&&v.target_request_id===id&&['found','not_found','unknown'].includes(v.lookup));if(v.lookup==='found')operation(v.operation,id);else need(v.operation===null);return structuredClone(v);}
export function pending(v,expectedPin){
 if(v===null||v===undefined)return null;
 need(exact(v,['contract','device_pin','revision','requests']));need(v.contract===1&&pin(v.device_pin)&&(!expectedPin||v.device_pin===expectedPin)&&Number.isSafeInteger(v.revision)&&v.revision>=0);
 need(Array.isArray(v.requests)&&v.requests.length<=16&&new Set(v.requests.map(r=>r?.request_id)).size===v.requests.length);
 need(v.requests.every(r=>exact(r,['request_id','action','trial_id','submit_started'])&&requestId(r.request_id)&&actions.includes(r.action)&&(r.trial_id===null||pin(r.trial_id))&&typeof r.submit_started==='boolean'));
 return structuredClone(v);
}
export function credentials(ssid,passphrase){need(typeof ssid==='string'&&typeof passphrase==='string','invalid_input');const n=new TextEncoder().encode(ssid).length;need(n>=1&&n<=32&&!/[\u0000-\u001f\u007f-\u009f]/u.test(ssid),'invalid_input');need(/^[\x20-\x7e]{8,63}$/.test(passphrase)||/^[0-9a-fA-F]{64}$/.test(passphrase),'invalid_input');return {ssid,passphrase};}
export const terminal=o=>o?.authoritative===true&&(o.state==='saved'&&o.settings_saved===true||o.state==='rolled_back'&&o.settings_saved===false&&o.rollback_verified===true);

export const errors=['unsupported','permission_denied','not_preapproved','pin_mismatch','auth_failed','revoked','busy','invalid_input','protocol_error','transport_lost','deadline','unknown','unavailable'];
export const errorCode=e=>errors.includes(e?.code)?e.code:errors.includes(e?.message)?e.message:'unknown';
