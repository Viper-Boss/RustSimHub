// SPDX-License-Identifier: AGPL-3.0-only
import assert from 'node:assert/strict';
import {appImports,appImportInventory,importInventory,assertAppImports,appVmSource} from './fixtures/web3-web/app-imports.mjs';
import {readFile} from 'node:fs/promises';
import * as nkn from '../crates/edge/src/web/nkn-product.js';
import * as nas from '../crates/edge/src/web/nas-bridge.js';
const app=await readFile(new URL('../crates/edge/src/web/app.js',import.meta.url),'utf8');
assert.equal(appImports.NknProductPanel,nkn.NknProductPanel);
assert.equal(appImports.nknPendingInput,nkn.pendingInput);
assert.equal(appImports.nknArchiveInput,nkn.archiveInput);
assert.equal(appImports.readNasJournal,nas.readJournal);
assert.equal(appImports.clearNasJournal,nas.clearJournal);
assertAppImports({...appImports});
for(const {local} of appImportInventory){const broken={...appImports};delete broken[local];assert.throws(()=>assertAppImports(broken),new RegExp('Missing app import: '+local));}
for(const name of ['NknProductPanel','nknPendingInput','nknArchiveInput'])assert.throws(()=>assertAppImports({...appImports,[name]:()=>{}}),/real production export/);
assert.throws(()=>importInventory("import Fake from './fake.js';"),/No app imports|Unsupported/);
assert.throws(()=>appVmSource(app+"\nimport { extra } from './extra.js';\n"),/inventory differ/);
assert(!/^import\b/m.test(appVmSource(app)));
console.log(`PASS: ${appImportInventory.length} real app import bindings from ${new Set(appImportInventory.map(i=>i.module)).size} modules; exact aliases, all missing-binding negatives and no empty NKN replacement.`);
