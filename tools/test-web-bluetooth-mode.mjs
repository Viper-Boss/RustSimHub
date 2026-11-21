// SPDX-License-Identifier: AGPL-3.0-only
import assert from 'node:assert/strict';
import { readFile } from 'node:fs/promises';
import { runInNewContext } from 'node:vm';
import { transientReadError } from '../crates/edge/src/web/console.js';

const start = new URL('../crates/edge/src/web/app.js', import.meta.url);
const source = await readFile(start, 'utf8');
const startAt = source.indexOf('const bluetoothLabels=');
const endAt = source.indexOf('let dataRoaming=', startAt);
assert.ok(startAt >= 0 && endAt > startAt);

function harness(initialToken, server, confirmation = true) {
 const nodes = new Map();
 const $ = id => {
  if (!nodes.has(id)) nodes.set(id, {attrs: new Map(), setAttribute(name,value){this.attrs.set(name,value);}});
  return nodes.get(id);
 };
 const requests = [];
 const state = {
  token: initialToken, viewEpoch: 0, $, transientReadError,
  confirm: () => confirmation,
  setTimeout: callback => queueMicrotask(callback),
  auth: command => { requests.push(command); return server(command); },
 };
 runInNewContext(source.slice(startAt, endAt) + '\nObject.assign(globalThis,{readBluetoothMode,applyBluetoothMode,resetBluetoothMode});', state);
 return {state,$,requests,read:()=>state.readBluetoothMode(),apply:()=>state.applyBluetoothMode()};
}

const healthy = {mode:'airsim',effective:'airsim',healthy:true,can_switch:true,nas_configured:false};
{
 const h = harness(null, () => {throw Error('unexpected command');});
 await h.read();
 assert.equal(h.requests.length,0);
 assert.equal(h.$('bt-mode-login').hidden,false);
 assert.equal(h.$('bt-mode-airsim').disabled,true);
}
{
 let attempts = 0;
 const h = harness('owner', () => {
  if (++attempts < 2) throw {coreCode:'busy'};
  return healthy;
 });
 await h.read();
 assert.equal(h.requests.length,2);
 assert.ok(h.requests.every(request => request.op==='bluetooth_mode_status'));
 assert.equal(h.$('bt-mode-login').hidden,true);
 assert.equal(h.$('bt-mode-airsim').disabled,false);
 assert.equal(h.$('bt-airsim-state').textContent,'使用中');
 assert.equal(h.$('bt-mode-nassimhub').disabled,true);
 assert.equal(h.$('bt-mode-apply').disabled,true);
}
{
 let calls = 0;
 const h = harness('owner', command => {
  if (command.op==='bluetooth_mode_set') {
   assert.equal(command.expected,'airsim');
   assert.equal(command.mode,'nassimhub');
   calls++;
   return {mode:'airsim',pending:'nassimhub',nas_configured:true};
  }
  if(calls===1)return {...healthy,mode:'nassimhub',effective:'nassimhub',nas_configured:true};
  return {...healthy,nas_configured:true};
 });
 await h.read();h.$('bt-mode-nassimhub').onclick();
 assert.equal(h.$('bt-mode-apply').disabled,false);
 await h.apply();
 assert.equal(calls,1);
 assert.equal(h.$('bt-nas-state').textContent,'使用中');
}
{
 const h = harness('owner', command => {
  if(command.op==='bluetooth_mode_set')throw {coreCode:'owner_reauthentication_required'};
  return {...healthy,nas_configured:true};
 });
 await h.read();h.$('bt-mode-nassimhub').onclick();await h.apply();
 assert.equal(h.$('bt-mode-login').hidden,false);
 assert.equal(h.$('bt-mode-apply').disabled,true);
 assert.match(h.$('bt-mode-status').textContent,/重新验证/);
 assert.equal(h.requests.filter(request=>request.op==='bluetooth_mode_set').length,1);
}
{
 let done;
 const h = harness('owner', () => new Promise(resolve=>{done=resolve;}));
 const first=h.read();await h.read();
 assert.equal(h.requests.length,1);
 h.state.token=null;h.state.viewEpoch++;h.state.resetBluetoothMode();
 done(healthy);await first;
 assert.equal(h.$('bt-mode-airsim').disabled,true);
 assert.equal(h.$('bt-mode-login').hidden,false);
}
console.log('Bluetooth mode UI passed: current session, busy retry, pending read, unconfigured NAS, single confirmed switch, recent-owner requirement and logout fencing.');
