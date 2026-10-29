// SPDX-License-Identifier: AGPL-3.0-only
// Metadata only. No recovery identity binding, CryptoKey, SSID, passphrase or response storage.
import {pending,need,pin} from './contracts.js';
export const DATABASE='rustsimhub-recovery-metadata-v1';
export class PendingStore {
 constructor(indexedDB,name=DATABASE){this.indexedDB=indexedDB;this.name=name;}
 open(){return new Promise((resolve,reject)=>{need(this.indexedDB,'unavailable');const q=this.indexedDB.open(this.name,1);q.onupgradeneeded=event=>{if(event.oldVersion!==0){q.transaction.abort();return;}q.result.createObjectStore('records');};q.onerror=()=>reject(Error('unavailable'));q.onblocked=()=>reject(Error('busy'));q.onsuccess=()=>{const db=q.result;if(db.objectStoreNames.length!==1||!db.objectStoreNames.contains('records')){db.close();reject(Error('protocol_error'));return;}resolve(db);};});}
 async load(devicePin){need(pin(devicePin),'invalid_input');const db=await this.open();try{return await new Promise((resolve,reject)=>{const t=db.transaction('records','readonly'),q=t.objectStore('records').get(devicePin);let value;q.onsuccess=()=>{value=q.result;};t.oncomplete=()=>{try{resolve(pending(value??null,devicePin));}catch{reject(Error('protocol_error'));}};t.onabort=t.onerror=()=>reject(Error('unavailable'));});}finally{db.close();}}
 async save(next,expectedRevision){next=pending(next);need(next!==null,'invalid_input');need(expectedRevision===null||Number.isSafeInteger(expectedRevision)&&expectedRevision>=0,'invalid_input');const db=await this.open();try{return await new Promise((resolve,reject)=>{const t=db.transaction('records','readwrite'),s=t.objectStore('records'),q=s.get(next.device_pin);let conflict=false,applied=false;
 q.onsuccess=()=>{try{const old=pending(q.result??null,next.device_pin);if((old?.revision??null)!==expectedRevision||next.revision!==(old?old.revision+1:0)||!Number.isSafeInteger(next.revision)){conflict=true;t.abort();return;}s.put(next,next.device_pin);applied=true;}catch{t.abort();}};
 t.oncomplete=()=>resolve(applied);t.onabort=()=>conflict?resolve(false):reject(Error('unavailable'));t.onerror=()=>{};
 });}finally{db.close();}}
 async anyPending(){const db=await this.open();try{return await new Promise((resolve,reject)=>{const t=db.transaction('records','readonly'),q=t.objectStore('records').getAll(undefined,65);let rows=[];q.onsuccess=()=>{rows=q.result;};t.oncomplete=()=>{try{resolve(rows.length>64||rows.some(r=>!r||pending(r).requests.length>0));}catch{resolve(true);}};t.onabort=t.onerror=()=>reject(Error('unavailable'));});}finally{db.close();}}
}
