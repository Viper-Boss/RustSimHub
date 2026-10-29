// SPDX-License-Identifier: AGPL-3.0-only
window.cspViolations=[];window.addEventListener('securitypolicyviolation',event=>window.cspViolations.push({directive:event.effectiveDirective,blocked:event.blockedURI}));

import {TestCenter} from '/test-center.js';
window.calls=[];
const ids=['check','outgoing','incoming','sms','forwarding','stop','recover'];
const actions=Object.fromEntries(ids.map(id=>[id,async()=>{window.calls.push(id);panel.update(state());}]));
const debug=document.createElement('p');debug.textContent='仅本地合成调试；无设备连接、短信、电话或转发。';
const panel=new TestCenter({document,root:document.getElementById('synthetic-test-center'),actions,debugElement:debug});
function state(){return {message:'合成状态：'+('长中文提示和未确认结果必须完整显示；'.repeat(40)),actions:Object.fromEntries(ids.map(id=>[id,{enabled:id==='check'||id==='recover',reason:'合成禁止原因：'+('请先核对原操作，不要重发。'.repeat(12))}]))};}
panel.update(state());window.panel=panel;window.ready=true;
