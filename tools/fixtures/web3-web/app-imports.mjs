// SPDX-License-Identifier: AGPL-3.0-only
// The VM tests isolate their existing business ports, but import bindings come
// from the shipped ESM modules (including transitive imports), never empty NKN classes.
import assert from 'node:assert/strict';
import {readFile} from 'node:fs/promises';
import vm from 'node:vm';
const appUrl=new URL('../../../crates/edge/src/web/app.js',import.meta.url);
const source=await readFile(appUrl,'utf8');
const declaration=/^import\s*\{([^}]+)\}\s*from\s*['"](.\/[-a-z0-9]+\.js)['"];[ \t]*\r?$/gm;
export function importInventory(text){
 const imports=[];
 for(const match of text.matchAll(declaration)){
  for(const name of match[1].split(',')){
   const binding=/^\s*([A-Za-z_$][\w$]*)(?:\s+as\s+([A-Za-z_$][\w$]*))?\s*$/.exec(name);
   assert(binding,'Unsupported app import binding');
   imports.push({module:match[2],exported:binding[1],local:binding[2]||binding[1]});
  }
 }
 assert(imports.length>0,'No app imports found');
 assert(!/^\s*import\b/m.test(text.replace(declaration,'')),'Unsupported import syntax; update the finite VM loader');
 assert.equal(new Set(imports.map(i=>i.local)).size,imports.length,'Duplicate app import binding');
 return imports;
}
export const appImportInventory=Object.freeze(importInventory(source).map(Object.freeze));
const modules=new Map(await Promise.all([...new Set(appImportInventory.map(i=>i.module))].map(async path=>[path,await import(new URL(path,appUrl))])));
export const appImports=Object.freeze(Object.fromEntries(appImportInventory.map(i=>{
 const value=modules.get(i.module)[i.exported];assert.notEqual(value,undefined,`Missing real export ${i.module}:${i.exported}`);return [i.local,value];
})));
export function assertAppImports(context){
 for(const {local} of appImportInventory)assert.equal(typeof context[local],typeof appImports[local],`Missing app import: ${local}`);
 for(const name of ['NknProductPanel','nknPendingInput','nknArchiveInput'])assert.equal(context[name],appImports[name],`${name} must be the real production export`);
}
// These are the DOM semantics newly exercised by the actual NKN constructor.
// Preserve existing fixture classes, textContent checks, RPCs, clocks and IDB transactions.
function attributes(element){
 element.style??={};
 if(!element.setAttribute){
  const values=new Map();
  element.setAttribute=(name,value)=>values.set(name,String(value));
  element.getAttribute=name=>values.get(name)??null;
  element.removeAttribute=name=>values.delete(name);
 }
 return element;
}
export function installAppImports(context){
 const missing=appImportInventory.filter(({local})=>!Object.hasOwn(context,local)).map(i=>i.local);
 for(const name of missing)context[name]=appImports[name];
 assertAppImports(context);
 const make=context.document.createElement.bind(context.document);
 context.document.createElement=(...args)=>attributes(make(...args));
 return missing;
}
export function appVmSource(text){
 assert.deepEqual(importInventory(text),appImportInventory,'App source and loaded module inventory differ');
 return text.replace(declaration,'');
}

export function assertNknAssembly(context){
 const panel=vm.runInNewContext('nknProductPanel',context);
 assert(panel instanceof appImports.NknProductPanel,'The app must instantiate the real NKN component');
 assert.equal(panel.sample,null,'Initial assembly must not claim an engine status');
 assert.equal(panel.pending,null);
 assert.equal(panel.buttons.remote_on.disabled,true,'Initial assembly cannot enable Remote ON');
 return panel;
}
