// SPDX-License-Identifier: AGPL-3.0-only
// Work only in a newly created private temporary copy. Keep both positive
// original-assertion output and missing-wire failures; remove only our copy.
import assert from 'node:assert/strict';
import {access,cp,mkdir,mkdtemp,readFile,rm,writeFile} from 'node:fs/promises';
import {tmpdir} from 'node:os';
import {join} from 'node:path';
import {fileURLToPath} from 'node:url';
import {spawnSync} from 'node:child_process';
const repo=fileURLToPath(new URL('../',import.meta.url));
const logs=join(repo,'.local','web3-web');await mkdir(logs,{recursive:true});
const directory=await mkdtemp(join(tmpdir(),'rustsimhub-web3-import-negative-'));
const names=['daily-phone','daily-sms','event-sync','session','test-center','wifi'],inventory=[];
try{
 await mkdir(join(directory,'crates','edge','src'),{recursive:true});await mkdir(join(directory,'tools','fixtures'),{recursive:true});
 await cp(join(repo,'crates','edge','src','web'),join(directory,'crates','edge','src','web'),{recursive:true});
 await cp(join(repo,'crates','edge','src','linux.rs'),join(directory,'crates','edge','src','linux.rs'));
 await cp(join(repo,'tools','fixtures','web3-web'),join(directory,'tools','fixtures','web3-web'),{recursive:true});
 for(const name of names){
  const script=`test-web-${name}.mjs`,path=join(directory,'tools',script),source=await readFile(join(repo,'tools',script),'utf8');
  assert.equal(source.split('installAppImports(context);').length,2,'Exactly one loader connection expected');
  await writeFile(path,source.replace('installAppImports(context);',"console.log('VM_IMPORTS:'+JSON.stringify(installAppImports(context)));"));
  const positive=spawnSync(process.execPath,[path],{cwd:directory,encoding:'utf8',timeout:30000});
  await writeFile(join(logs,`copy-positive-${name}.log`),(positive.stdout||'')+(positive.stderr||''));
  assert.equal(positive.status,0,`Positive temporary copy failed: ${name}`);
  const observed=[...new Set(positive.stdout.split('\n').filter(s=>s.startsWith('VM_IMPORTS:')).map(s=>s.slice(11)))];
  assert.equal(observed.length,1,'A fixture must have a stable import injection set');inventory.push({script,missing:JSON.parse(observed[0])});
  // Load every current production binding, then remove only the dependency
  // under test. Removing the whole loader can fail first on an unrelated new
  // import and never exercise the original NKN assembly assertion.
  await writeFile(path,source.replace('installAppImports(context);','installAppImports(context);delete context.NknProductPanel;'));
  const negative=spawnSync(process.execPath,[path],{cwd:directory,encoding:'utf8',timeout:30000});
  await writeFile(join(logs,`copy-negative-${name}.log`),(negative.stdout||'')+(negative.stderr||''));
  assert.equal(negative.status,1,`Missing dependency must fail: ${name}`);
  assert.match(negative.stderr,/ReferenceError: NknProductPanel is not defined/,`Wrong negative failure: ${name}`);
  assert.equal(await readFile(join(repo,'tools',script),'utf8'),source,'Original script changed during the negative control');
 }
 await writeFile(join(logs,'import-inventory.json'),JSON.stringify(inventory,null,2)+'\n');
}finally{
 await rm(directory,{recursive:true,force:true});
 await assert.rejects(access(directory),{code:'ENOENT'});
 await writeFile(join(logs,'negative-cleanup.json'),JSON.stringify({temporary_copy_removed:true,source_tree_unchanged:true})+'\n');
}
console.log('PASS: all six temporary positive copies execute original assertions; removing only the real NknProductPanel binding fails all six with its ReferenceError; temporary copy removed, original logs retained.');
