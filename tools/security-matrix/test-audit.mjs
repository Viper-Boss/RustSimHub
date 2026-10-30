// SPDX-License-Identifier: AGPL-3.0-only
import test from 'node:test';
import assert from 'node:assert/strict';
import fs from 'node:fs';
import {spawnSync} from 'node:child_process';
import {analyzeUnit, evaluateRules, audit, policyCases} from './audit.mjs';

const manifest = JSON.parse(fs.readFileSync(new URL('sources.json', import.meta.url)));
const profile = {identity: 'synthetic-worker', capabilities: [], families: ['AF_UNIX'], credentials: []};
const unit = `[Service]
User=synthetic-worker
Group=synthetic-worker
NoNewPrivileges=yes
ProtectSystem=strict
ProtectHome=yes
UMask=0077
CapabilityBoundingSet=
AmbientCapabilities=
RestrictAddressFamilies=AF_UNIX
ReadWritePaths=/run/synthetic
ExecStart=/usr/libexec/synthetic --token SYNTHETIC_SECRET_ARGUMENT
Environment=SYNTHETIC_PRIVATE=SYNTHETIC_SECRET_ENV
`;

test('bounded unit subset records declarations without arguments, environment or runtime claims', () => {
  const row = analyzeUnit(unit, profile);
  assert.deepEqual(row.findings, []);
  assert.deepEqual(row.gaps, []);
  assert.equal(row.actual_uid_verified, false);
  assert.equal(row.effective_configuration_verified, false);
  assert.deepEqual(row.commands, [{directive: 'ExecStart', executable: '/usr/libexec/synthetic', prefix: ''}]);
  assert(!JSON.stringify(row).includes('SYNTHETIC_SECRET'));
});

test('identity, capability, family, hardening, write, credential and privileged command changes are findings', () => {
  for (const [from, to, finding] of [
    ['User=synthetic-worker', 'User=root', 'unexpected_declared_identity'],
    ['CapabilityBoundingSet=', 'CapabilityBoundingSet=CAP_SYS_ADMIN', 'capability_set_differs'],
    ['AmbientCapabilities=', 'AmbientCapabilities=CAP_NET_ADMIN', 'ambient_not_explicitly_empty'],
    ['RestrictAddressFamilies=AF_UNIX', 'RestrictAddressFamilies=AF_UNIX AF_PACKET', 'address_family_set_differs'],
    ['NoNewPrivileges=yes', 'NoNewPrivileges=no', 'hardening_NoNewPrivileges'],
    ['ReadWritePaths=/run/synthetic', 'ReadWritePaths=/', 'broad_write_path'],
    ['Group=synthetic-worker', 'Group=synthetic-worker\nLoadCredential=unreviewed:/run/other', 'credential_declaration_differs'],
    ['ExecStart=/', 'ExecStart=+/', 'privileged_command_prefix'],
  ]) assert(analyzeUnit(unit.replace(from, to), profile).findings.includes(finding), finding);
});

test('unsupported systemd expressions and missing family restriction never become effective-policy success', () => {
  for (const modified of [unit + 'User=other\n', unit + 'ReadOnlyPaths=/run/ \\\n next\n',
    unit.replace('CapabilityBoundingSet=', 'CapabilityBoundingSet=~CAP_SYS_ADMIN')]) assert.throws(() => analyzeUnit(modified, profile));
  const row = analyzeUnit(unit.replace('RestrictAddressFamilies=AF_UNIX\n', 'PrivateNetwork=yes\n'), profile);
  assert(row.gaps.includes('address_families_not_declared'));
  assert.equal(row.effective_configuration_verified, false);
});

test('trusted polkit callback fixture preserves first decision and distinguishes fallthrough from NO', () => {
  const allow = 'polkit.addRule((a,s)=>s.user==="core"?polkit.Result.YES:undefined)';
  const deny = 'polkit.addRule(()=>polkit.Result.NO)';
  assert.equal(evaluateRules([allow, deny], {user: 'core'}, {}), 'YES');
  assert.equal(evaluateRules([allow], {user: 'other'}, {}), 'NOT_HANDLED');
  assert.equal(evaluateRules([deny, allow], {user: 'core'}, {}), 'NO');
  assert.throws(() => evaluateRules(['polkit.addRule(()=>"allowed")'], {}, {}));
  assert.throws(() => evaluateRules(['polkit.addRule(()=>{while(true){}})'], {}, {}));
});

test('actual project rules explicitly deny all seven forwarding hardware requests after the fix', () => {
  const report = audit(manifest);
  assert.equal(report.units.length, 12);
  assert.equal(report.polkit_matrix.length, policyCases().length);
  assert.equal(report.polkit_matrix.length, 112);
  assert.equal(report.findings, 0);
  assert.equal(report.status, 'incomplete');
  const gaps = report.polkit_matrix.filter(row => row.result === 'coverage_gap');
  assert.equal(gaps.length, 0);
  const hardware = report.polkit_matrix.filter(row => row.role === 'rustsimhub-forwarder' && row.id !== 'org.freedesktop.systemd1.manage-units');
  assert.equal(hardware.length, 7);
  assert(hardware.every(row => row.actual === 'NO' && row.result === 'matched'));
  assert(report.polkit_matrix.every(row => row.result === 'matched'));
  assert.equal(report.units.filter(row => row.gaps.includes('address_families_not_declared')).length, 2);
  assert.equal(report.gaps, 2);
  assert.equal(report.runtime_verified, false);
  assert.equal(report.production_approved, false);
});

test('changed SHA, malformed/duplicate manifest and missing sources cannot reuse the old conclusion', () => {
  const changed = structuredClone(manifest);
  changed.files[0].sha256 = '0'.repeat(64);
  const report = audit(changed);
  assert.equal(report.status, 'incomplete');
  assert.equal(report.source_issues[0].code, 'source_changed');
  assert.equal(report.units.length, 11);
  assert.throws(() => audit({...manifest, files: [...manifest.files, manifest.files[0]]}));
  assert.throws(() => audit({...manifest, files: [{...manifest.files[0], path: '.private/no.service'}]}));
  const noPolicy = audit({...manifest, files: manifest.files.filter(item => item.kind === 'unit')});
  assert.equal(noPolicy.polkit_matrix.length, 0);
  assert.equal(noPolicy.source_issues.at(-1).code, 'incomplete_polkit_sources');
});

test('actual CLI defaults to plan and returns incomplete, not clean, for the full fixed audit', () => {
  const script = new URL('audit.mjs', import.meta.url);
  const plan = spawnSync(process.execPath, [script.pathname], {encoding: 'utf8', timeout: 10000, maxBuffer: 1024 * 1024});
  assert.equal(plan.status, 0);
  assert.equal(JSON.parse(plan.stdout).read_source_contents, false);
  const run = spawnSync(process.execPath, [script.pathname, '--run'], {encoding: 'utf8', timeout: 10000, maxBuffer: 1024 * 1024});
  assert.equal(run.status, 2);
  assert.equal(JSON.parse(run.stdout).status, 'incomplete');
  assert.equal(run.stderr, '');
  const invalid = spawnSync(process.execPath, [script.pathname, '--file', '/etc/shadow'], {encoding: 'utf8', timeout: 10000});
  assert.equal(invalid.status, 2);
  assert.equal(invalid.stdout, '');
});
