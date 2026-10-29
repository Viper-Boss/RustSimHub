// SPDX-License-Identifier: AGPL-3.0-only
export const BASE='/RustSimHub/configurator/';
export const FILES=['index.html','manifest.webmanifest','assets/icon.svg','sw.js','src/contracts.js','src/pending-store.js','src/recovery-controller.js','src/static-policy.js','src/app.js'];
export function isStaticRequest(request,origin){try{const u=new URL(request.url);return request.method==='GET'&&u.origin===origin&&!u.search&&!request.headers?.has('Authorization')&&(u.pathname===BASE||FILES.some(f=>u.pathname===BASE+f));}catch{return false;}}
