// SPDX-License-Identifier: AGPL-3.0-only
// Username + password -> Ed25519 Owner key, derived in the browser. The device stores only the
// public key; the password never leaves the page. Pure JS so it also works on the plain-http
// first-setup page, where crypto.subtle does not exist (insecure context).
export const ITERATIONS=210000;
const enc=new TextEncoder();
export function normalizeUsername(name){
 const v=String(name??'').normalize('NFC').trim().toLowerCase();
 if(v.length<1||v.length>32||/[\u0000-\u001f\u007f]/.test(v))throw Error('用户名需为 1–32 个字符');
 return v;
}
export function normalizePassword(password){
 const v=String(password??'').normalize('NFC');
 if(v.length<8||v.length>64||/[\u0000-\u001f\u007f]/.test(v))throw Error('密码需为 8–64 个字符');
 return v;
}
const hex=b=>Array.from(b,v=>v.toString(16).padStart(2,'0')).join('');
function salt(devicePublicKey,username){
 if(!(devicePublicKey instanceof Uint8Array)||devicePublicKey.length!==32)throw Error('设备公钥无效');
 return enc.encode('RustSimHub.Password.v1\0'+hex(devicePublicKey)+'\0'+normalizeUsername(username));
}
// SHA-256 compression on a 64-byte block (Int32 math).
const K256=new Uint32Array([0x428a2f98,0x71374491,0xb5c0fbcf,0xe9b5dba5,0x3956c25b,0x59f111f1,0x923f82a4,0xab1c5ed5,0xd807aa98,0x12835b01,0x243185be,0x550c7dc3,0x72be5d74,0x80deb1fe,0x9bdc06a7,0xc19bf174,0xe49b69c1,0xefbe4786,0x0fc19dc6,0x240ca1cc,0x2de92c6f,0x4a7484aa,0x5cb0a9dc,0x76f988da,0x983e5152,0xa831c66d,0xb00327c8,0xbf597fc7,0xc6e00bf3,0xd5a79147,0x06ca6351,0x14292967,0x27b70a85,0x2e1b2138,0x4d2c6dfc,0x53380d13,0x650a7354,0x766a0abb,0x81c2c92e,0x92722c85,0xa2bfe8a1,0xa81a664b,0xc24b8b70,0xc76c51a3,0xd192e819,0xd6990624,0xf40e3585,0x106aa070,0x19a4c116,0x1e376c08,0x2748774c,0x34b0bcb5,0x391c0cb3,0x4ed8aa4a,0x5b9cca4f,0x682e6ff3,0x748f82ee,0x78a5636f,0x84c87814,0x8cc70208,0x90befffa,0xa4506ceb,0xbef9a3f7,0xc67178f2]);
const IV256=[0x6a09e667,0xbb67ae85,0x3c6ef372,0xa54ff53a,0x510e527f,0x9b05688c,0x1f83d9ab,0x5be0cd19];
const W=new Uint32Array(64);
function compress(state,words){
 for(let i=0;i<16;i++)W[i]=words[i];
 for(let i=16;i<64;i++){const a=W[i-15],b=W[i-2];W[i]=(((a>>>7|a<<25)^(a>>>18|a<<14)^(a>>>3))+W[i-7]+((b>>>17|b<<15)^(b>>>19|b<<13)^(b>>>10))+W[i-16])|0;}
 let [a,b,c,d,e,f,g,h]=state;
 for(let i=0;i<64;i++){
  const t1=(h+((e>>>6|e<<26)^(e>>>11|e<<21)^(e>>>25|e<<7))+((e&f)^(~e&g))+K256[i]+W[i])|0;
  const t2=(((a>>>2|a<<30)^(a>>>13|a<<19)^(a>>>22|a<<10))+((a&b)^(a&c)^(b&c)))|0;
  h=g;g=f;f=e;e=(d+t1)|0;d=c;c=b;b=a;a=(t1+t2)|0;
 }
 return [(state[0]+a)|0,(state[1]+b)|0,(state[2]+c)|0,(state[3]+d)|0,(state[4]+e)|0,(state[5]+f)|0,(state[6]+g)|0,(state[7]+h)|0];
}
function words(bytes){const w=new Array(bytes.length/4);for(let i=0;i<w.length;i++)w[i]=(bytes[i*4]<<24|bytes[i*4+1]<<16|bytes[i*4+2]<<8|bytes[i*4+3])|0;return w;}
function pad(message,prefix){
 const total=prefix+message.length,length=Math.ceil((message.length+9)/64)*64,out=new Uint8Array(length);
 out.set(message);out[message.length]=0x80;const bits=total*8;
 for(let i=0;i<4;i++)out[length-1-i]=(bits/2**(8*i))&0xff;
 return out;
}
export function sha256(message){
 const m=pad(message,0);let s=IV256;for(let i=0;i<m.length;i+=64)s=compress(s,words(m.subarray(i,i+64)));
 const out=new Uint8Array(32);s.forEach((v,i)=>{out[i*4]=v>>>24;out[i*4+1]=v>>>16;out[i*4+2]=v>>>8;out[i*4+3]=v;});return out;
}
function hmacStates(key){
 if(key.length>64)key=sha256(key);
 const block=new Uint8Array(64);block.set(key);
 const inner=compress(IV256,words(block.map(v=>v^0x36))),outer=compress(IV256,words(block.map(v=>v^0x5c)));
 return {inner,outer};
}
function finish(state,message){let s=state;const m=pad(message,64);for(let i=0;i<m.length;i+=64)s=compress(s,words(m.subarray(i,i+64)));return s;}
const toBytes=s=>{const out=new Uint8Array(32);s.forEach((v,i)=>{out[i*4]=v>>>24;out[i*4+1]=v>>>16;out[i*4+2]=v>>>8;out[i*4+3]=v;});return out;};
// PBKDF2-HMAC-SHA256 with dkLen 32 (one block), precomputed pad states.
export function pbkdf2(password,saltBytes,iterations){
 const {inner,outer}=hmacStates(password),first=new Uint8Array(saltBytes.length+4);first.set(saltBytes);first[saltBytes.length+3]=1;
 let u=finish(outer,toBytes(finish(inner,first)));const t=u.slice();
 // Each later round hashes a 32-byte U: one padded block per side.
 const block=new Array(16).fill(0);block[8]=0x80000000|0;block[15]=(64+32)*8;
 for(let i=1;i<iterations;i++){
  for(let j=0;j<8;j++)block[j]=u[j];
  const x=compress(inner,block);for(let j=0;j<8;j++)block[j]=x[j];
  u=compress(outer,block);for(let j=0;j<8;j++)t[j]^=u[j];
 }
 return toBytes(t);
}
// SHA-512 (BigInt; only a few blocks per signature).
const K512=['428a2f98d728ae22','7137449123ef65cd','b5c0fbcfec4d3b2f','e9b5dba58189dbbc','3956c25bf348b538','59f111f1b605d019','923f82a4af194f9b','ab1c5ed5da6d8118','d807aa98a3030242','12835b0145706fbe','243185be4ee4b28c','550c7dc3d5ffb4e2','72be5d74f27b896f','80deb1fe3b1696b1','9bdc06a725c71235','c19bf174cf692694','e49b69c19ef14ad2','efbe4786384f25e3','0fc19dc68b8cd5b5','240ca1cc77ac9c65','2de92c6f592b0275','4a7484aa6ea6e483','5cb0a9dcbd41fbd4','76f988da831153b5','983e5152ee66dfab','a831c66d2db43210','b00327c898fb213f','bf597fc7beef0ee4','c6e00bf33da88fc2','d5a79147930aa725','06ca6351e003826f','142929670a0e6e70','27b70a8546d22ffc','2e1b21385c26c926','4d2c6dfc5ac42aed','53380d139d95b3df','650a73548baf63de','766a0abb3c77b2a8','81c2c92e47edaee6','92722c851482353b','a2bfe8a14cf10364','a81a664bbc423001','c24b8b70d0f89791','c76c51a30654be30','d192e819d6ef5218','d69906245565a910','f40e35855771202a','106aa07032bbd1b8','19a4c116b8d2d0c8','1e376c085141ab53','2748774cdf8eeb99','34b0bcb5e19b48a8','391c0cb3c5c95a63','4ed8aa4ae3418acb','5b9cca4f7763e373','682e6ff3d6b2b8a3','748f82ee5defb2fc','78a5636f43172f60','84c87814a1f0ab72','8cc702081a6439ec','90befffa23631e28','a4506cebde82bde9','bef9a3f7b2c67915','c67178f2e372532b','ca273eceea26619c','d186b8c721c0c207','eada7dd6cde0eb1e','f57d4f7fee6ed178','06f067aa72176fba','0a637dc5a2c898a6','113f9804bef90dae','1b710b35131c471b','28db77f523047d84','32caab7b40c72493','3c9ebe0a15c9bebc','431d67c49c100d4c','4cc5d4becb3e42b6','597f299cfc657e2a','5fcb6fab3ad6faec','6c44198c4a475817'].map(v=>BigInt('0x'+v));
const M64=(1n<<64n)-1n,rot=(x,n)=>((x>>BigInt(n))|(x<<BigInt(64-n)))&M64;
export function sha512(message){
 const len=Math.ceil((message.length+17)/128)*128,m=new Uint8Array(len);m.set(message);m[message.length]=0x80;
 const bits=BigInt(message.length)*8n;for(let i=0;i<16;i++)m[len-1-i]=Number((bits>>BigInt(8*i))&0xffn);
 let h=['6a09e667f3bcc908','bb67ae8584caa73b','3c6ef372fe94f82b','a54ff53a5f1d36f1','510e527fade682d1','9b05688c2b3e6c1f','1f83d9abfb41bd6b','5be0cd19137e2179'].map(v=>BigInt('0x'+v));
 const w=new Array(80);
 for(let o=0;o<len;o+=128){
  for(let i=0;i<16;i++){let v=0n;for(let j=0;j<8;j++)v=(v<<8n)|BigInt(m[o+i*8+j]);w[i]=v;}
  for(let i=16;i<80;i++)w[i]=(w[i-16]+(rot(w[i-15],1)^rot(w[i-15],8)^(w[i-15]>>7n))+w[i-7]+(rot(w[i-2],19)^rot(w[i-2],61)^(w[i-2]>>6n)))&M64;
  let [a,b,c,d,e,f,g,hh]=h;
  for(let i=0;i<80;i++){
   const t1=(hh+(rot(e,14)^rot(e,18)^rot(e,41))+((e&f)^(~e&M64&g))+K512[i]+w[i])&M64,t2=((rot(a,28)^rot(a,34)^rot(a,39))+((a&b)^(a&c)^(b&c)))&M64;
   hh=g;g=f;f=e;e=(d+t1)&M64;d=c;c=b;b=a;a=(t1+t2)&M64;
  }
  h=h.map((v,i)=>(v+[a,b,c,d,e,f,g,hh][i])&M64);
 }
 const out=new Uint8Array(64);h.forEach((v,i)=>{for(let j=0;j<8;j++)out[i*8+j]=Number((v>>BigInt(56-8*j))&0xffn);});return out;
}
// Ed25519 (RFC 8032), extended coordinates. Not constant time; keys are user-derived and
// signing happens a handful of times per login.
const P=2n**255n-19n,L=2n**252n+27742317777372353535851937790883648493n;
const mod=(a,m=P)=>{const r=a%m;return r>=0n?r:r+m;};
const pow=(b,e)=>{let r=1n;b=mod(b);while(e>0n){if(e&1n)r=r*b%P;b=b*b%P;e>>=1n;}return r;};
const inv=a=>pow(a,P-2n);
const D=mod(-121665n*inv(121666n)),D2=mod(2n*D);
const BX=15112221349535400772501151409588531511454012693041857206046113283949847762202n,BY=46316835694926478169428394003475163141307993866256225615783033603165251855960n;
const BASE=[BX,BY,1n,mod(BX*BY)];
function add([X1,Y1,Z1,T1],[X2,Y2,Z2,T2]){
 const A=mod((Y1-X1)*(Y2-X2)),B=mod((Y1+X1)*(Y2+X2)),C=mod(T1*D2*T2),Dd=mod(Z1*2n*Z2);
 const E=B-A,F=Dd-C,G=Dd+C,H=B+A;return [mod(E*F),mod(G*H),mod(F*G),mod(E*H)];
}
function mul(point,n){let r=[0n,1n,1n,0n],q=point;while(n>0n){if(n&1n)r=add(r,q);q=add(q,q);n>>=1n;}return r;}
function encode([X,Y,Z]){const zi=inv(Z),x=mod(X*zi),y=mod(Y*zi),out=new Uint8Array(32);let v=y;for(let i=0;i<32;i++){out[i]=Number(v&0xffn);v>>=8n;}if(x&1n)out[31]|=0x80;return out;}
const le=b=>{let v=0n;for(let i=b.length-1;i>=0;i--)v=(v<<8n)|BigInt(b[i]);return v;};
const leBytes=v=>{const out=new Uint8Array(32);for(let i=0;i<32;i++){out[i]=Number(v&0xffn);v>>=8n;}return out;};
const cat=(...parts)=>{const out=new Uint8Array(parts.reduce((n,p)=>n+p.length,0));let o=0;for(const p of parts){out.set(p,o);o+=p.length;}return out;};
function expand(seed){
 if(!(seed instanceof Uint8Array)||seed.length!==32)throw Error('密钥无效');
 const h=sha512(seed),a=h.slice(0,32);a[0]&=248;a[31]&=127;a[31]|=64;
 const scalar=le(a);return {scalar,prefix:h.slice(32),publicKey:encode(mul(BASE,scalar))};
}
export function publicKey(seed){return expand(seed).publicKey;}
export function sign(seed,message){
 const {scalar,prefix,publicKey:A}=expand(seed),r=mod(le(sha512(cat(prefix,message))),L),R=encode(mul(BASE,r));
 const k=mod(le(sha512(cat(R,A,message))),L);return cat(R,leBytes(mod(r+k*scalar,L)));
}
/// The Owner seed for this device; identical on the http first-setup page and the https app.
export function deriveSeed(devicePublicKey,username,password){
 return pbkdf2(enc.encode(normalizePassword(password)),salt(devicePublicKey,username),ITERATIONS);
}
/// WebCrypto key pair (non-extractable private key) for the https app, from the derived seed.
export async function importKeyPair(seed,subtle=globalThis.crypto?.subtle){
 if(!subtle)throw Error('当前页面不支持浏览器加密，请使用 https 地址');
 const pkcs8=cat(new Uint8Array([0x30,0x2e,0x02,0x01,0x00,0x30,0x05,0x06,0x03,0x2b,0x65,0x70,0x04,0x22,0x04,0x20]),seed);
 const privateKey=await subtle.importKey('pkcs8',pkcs8,{name:'Ed25519'},false,['sign']);
 const publicKeyObject=await subtle.importKey('raw',publicKey(seed),{name:'Ed25519'},true,['verify']);
 pkcs8.fill(0);
 return {privateKey,publicKey:publicKeyObject};
}
// A failed attempt must leave this browser's approved key in place: only a login
// that actually signed in may replace what is stored.
export async function tryIdentity({use,login,loggedIn,commit,rollback}){
 use();
 try{await login();}
 finally{if(loggedIn())await commit();else rollback();}
}
