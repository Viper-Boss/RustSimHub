// SPDX-License-Identifier: AGPL-3.0-only
// Pure-JS password key derivation and Ed25519 against RFC vectors and Node's WebCrypto.
import assert from 'node:assert/strict';
import {webcrypto} from 'node:crypto';
import {sha256,sha512,pbkdf2,publicKey,sign,deriveSeed,importKeyPair,normalizeUsername,normalizePassword,tryIdentity,ITERATIONS} from '../crates/edge/src/web/password.js';
const subtle=webcrypto.subtle,enc=new TextEncoder();
const hex=b=>Buffer.from(b).toString('hex'),bytes=h=>new Uint8Array(Buffer.from(h,'hex'));
let passed=0;const ok=()=>passed++;
// Hashes (FIPS 180 "abc", empty, and multi-block lengths) match Node.
assert.equal(hex(sha256(enc.encode('abc'))),'ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad');
assert.equal(hex(sha512(new Uint8Array())),'cf83e1357eefb8bdf1542850d66d8007d620e4050b5715dc83f4a921d36ce9ce47d0d13c5d85f2b0ff8318d2877eec2f63b931bd47417a81a538327af927da3e');
for(const n of [55,56,63,64,111,112,127,128,200]){const m=new Uint8Array(n).map((_,i)=>i*7);
 assert.equal(hex(sha256(m)),hex(new Uint8Array(await subtle.digest('SHA-256',m))));
 assert.equal(hex(sha512(m)),hex(new Uint8Array(await subtle.digest('SHA-512',m))));}
ok();
// PBKDF2-HMAC-SHA256 RFC-style vectors and Node equality, including a long password (> block).
assert.equal(hex(pbkdf2(enc.encode('password'),enc.encode('salt'),1)),'120fb6cffcf8b32c43e7225256c4f837a86548c92ccc35480805987cb70be17b');
assert.equal(hex(pbkdf2(enc.encode('password'),enc.encode('salt'),4096)),'c5e478d59288c841aa530db6845c4c8d962893a001ce4e11a4963873aa98134a');
for(const pw of ['x','密码很长'.repeat(30)]){const key=await subtle.importKey('raw',enc.encode(pw),'PBKDF2',false,['deriveBits']);
 const want=new Uint8Array(await subtle.deriveBits({name:'PBKDF2',hash:'SHA-256',salt:enc.encode('s'),iterations:1000},key,256));
 assert.equal(hex(pbkdf2(enc.encode(pw),enc.encode('s'),1000)),hex(want));}
ok();
// Ed25519 RFC 8032 test 1 and 2.
const s1=bytes('9d61b19deffd5a60ba844af492ec2cc44449c5697b326919703bac031cae7f60');
assert.equal(hex(publicKey(s1)),'d75a980182b10ab7d54bfed3c964073a0ee172f3daa62325af021a68f707511a');
assert.equal(hex(sign(s1,new Uint8Array())),'e5564300c360ac729086e2cc806e828a84877f1eb8e5d974d873e065224901555fb8821590a33bacc61e39701cf9b46bd25bf5f0595bbe24655141438e7a100b');
const s2=bytes('4ccd089b28ff96da9db6c346ec114e0f5b8a319f35aba624da8cf6ed4fb8a6fb');
assert.equal(hex(publicKey(s2)),'3d4017c3e843895a92b70aa74d1b7ebc9c982ccf2ec4968cc0cd55f12af4660c');
assert.equal(hex(sign(s2,bytes('72'))),'92a009a9f0d4cab8720e820b5f642540a2b27b5416503f8fb3762223ebdb69da085ac1e43e15996e458f3613d0f11d8c387b2eaeb4302aeeb00d291612bb0c00');
ok();
// Derived key: same seed for name case/whitespace variants, different per device/password,
// and the WebCrypto import signs verifiably with the pure-JS public key.
const device=new Uint8Array(32).fill(5),other=new Uint8Array(32).fill(6);
const t0=Date.now(),seed=deriveSeed(device,' Admin ','correct horse');const ms=Date.now()-t0;
assert.equal(hex(seed),hex(deriveSeed(device,'admin','correct horse')));
assert.notEqual(hex(seed),hex(deriveSeed(other,'admin','correct horse')));
assert.notEqual(hex(seed),hex(deriveSeed(device,'admin','correct horsf')));
const key=await subtle.importKey('raw',enc.encode('correct horse'),'PBKDF2',false,['deriveBits']);
const saltBytes=enc.encode('RustSimHub.Password.v1\0'+hex(device)+'\0admin');
assert.equal(hex(seed),hex(new Uint8Array(await subtle.deriveBits({name:'PBKDF2',hash:'SHA-256',salt:saltBytes,iterations:ITERATIONS},key,256))));
const pair=await importKeyPair(seed,subtle),message=enc.encode('transcript');
const webSig=new Uint8Array(await subtle.sign('Ed25519',pair.privateKey,message));
assert.equal(hex(webSig),hex(sign(seed,message)));
assert.ok(await subtle.verify('Ed25519',pair.publicKey,sign(seed,message),message));
assert.equal(hex(new Uint8Array(await subtle.exportKey('raw',pair.publicKey))),hex(publicKey(seed)));
ok();
// Input bounds.
for(const bad of ['', 'a'.repeat(33), 'a\u0000b'])assert.throws(()=>normalizeUsername(bad));
for(const bad of ['short', 'a'.repeat(65), 'abcdefg\n'])assert.throws(()=>normalizePassword(bad));
await assert.rejects(importKeyPair(seed,null));
assert.throws(()=>deriveSeed(new Uint8Array(31),'admin','correct horse'));
ok();
// A failed password attempt must leave the approved browser key usable and stored.
{const attempt=async({fails,token=null,throwsAfterToken=false})=>{const state={key:'approved',stored:'approved',token:null,archived:null};let error=null;
  try{await tryIdentity({use:()=>{state.key='derived';},login:async()=>{assert.equal(state.key,'derived','login signs with the candidate');if(fails)throw Error('denied');state.token=token;if(throwsAfterToken)throw Error('refresh failed');},loggedIn:()=>!!state.token,commit:async()=>{state.archived=state.stored;state.stored=state.key;},rollback:()=>{state.key='approved';}});}catch(e){error=e;}
  return {...state,error};};
 const denied=await attempt({fails:true});assert.equal(denied.error?.message,'denied','the failure still reaches the page');assert.equal(denied.key,'approved');assert.equal(denied.stored,'approved');assert.equal(denied.archived,null);
 const noToken=await attempt({fails:false});assert.equal(noToken.key,'approved','no session means no replacement');assert.equal(noToken.stored,'approved');
 const good=await attempt({fails:false,token:'t'});assert.equal(good.error,null);assert.equal(good.stored,'derived');assert.equal(good.archived,'approved');
 const late=await attempt({fails:false,token:'t',throwsAfterToken:true});assert.equal(late.error?.message,'refresh failed');assert.equal(late.stored,'derived','signed in, then a later read failed: keep the working key');}
ok();
console.log(`password ${passed}/6 groups passed; derivation ${ms} ms`);
