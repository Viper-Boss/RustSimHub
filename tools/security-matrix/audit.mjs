// SPDX-License-Identifier: AGPL-3.0-only
// Reads only the bundled list of public project sources. Never queries systemd,
// polkit, D-Bus, credentials or a device. VM is a fixture for trusted rules only.
import fs from 'node:fs';
import path from 'node:path';
import crypto from 'node:crypto';
import vm from 'node:vm';
import {fileURLToPath, pathToFileURL} from 'node:url';

const ROOT = fileURLToPath(new URL('../../', import.meta.url));
const MAX_FILE = 65536;
const scalar = (fields, key) => {
  const values = fields.get(key) ?? [];
  if (values.length > 1) throw new Error('repeated_scalar');
  return values[0] ?? null;
};

function readPublic(relative) {
  if (!/^[a-zA-Z0-9_./-]+$/.test(relative) || relative.split('/').some(p => !p || p === '..' || p.startsWith('.'))) {
    throw new Error('invalid_public_path');
  }
  let current = ROOT;
  for (const part of relative.split('/').slice(0, -1)) {
    current = path.join(current, part);
    if (!fs.lstatSync(current).isDirectory()) throw new Error('unsafe_parent');
  }
  const fd = fs.openSync(path.join(ROOT, relative), fs.constants.O_RDONLY | fs.constants.O_NOFOLLOW | fs.constants.O_NONBLOCK);
  try {
    const before = fs.fstatSync(fd, {bigint: true});
    if (!before.isFile() || before.nlink !== 1n || before.size > BigInt(MAX_FILE)) throw new Error('unsafe_file');
    const buffer = Buffer.alloc(MAX_FILE + 1);
    let size = 0;
    while (size < buffer.length) {
      const count = fs.readSync(fd, buffer, size, buffer.length - size, null);
      if (!count) break;
      size += count;
    }
    const after = fs.fstatSync(fd, {bigint: true});
    if (size > MAX_FILE || size !== Number(before.size) || before.ino !== after.ino || before.dev !== after.dev || before.size !== after.size || before.mtimeNs !== after.mtimeNs || before.ctimeNs !== after.ctimeNs) {
      throw new Error('changed_or_oversized');
    }
    return new TextDecoder('utf-8', {fatal: true}).decode(buffer.subarray(0, size));
  } finally { fs.closeSync(fd); }
}

export function analyzeUnit(text, profile) {
  const service = new Map();
  let section = '';
  for (const line of text.split('\n')) {
    const value = line.trim();
    if (!value || /^[#;]/.test(value)) continue;
    if (value.endsWith('\\') || value.includes('\0')) throw new Error('unsupported_unit_syntax');
    if (/^\[[A-Za-z]+\]$/.test(value)) { section = value.slice(1, -1); continue; }
    const match = /^([A-Za-z][A-Za-z0-9]*)\s*=(.*)$/.exec(value);
    if (!section || !match) throw new Error('unsupported_unit_syntax');
    if (section === 'Service') service.set(match[1], [...(service.get(match[1]) ?? []), match[2].trim()]);
  }
  const findings = [], gaps = [];
  const declaredUser = scalar(service, 'User');
  const dynamic = scalar(service, 'DynamicUser');
  const identity = dynamic === 'yes' ? 'dynamic' : declaredUser ?? 'default_root';
  if (identity !== profile.identity) findings.push('unexpected_declared_identity');
  if (dynamic === 'yes' && declaredUser !== null) gaps.push('dynamic_named_identity_needs_review');
  for (const [key, expected] of [['NoNewPrivileges', 'yes'], ['ProtectSystem', 'strict'], ['ProtectHome', 'yes'], ['UMask', '0077']]) {
    if (scalar(service, key) !== expected) findings.push(`hardening_${key}`);
  }
  // Restricted subset only: inversion, quoting, specifiers and list resets are
  // never interpreted as an effective systemd authorization policy.
  const words = key => {
    const values = service.get(key) ?? [];
    if (values.length !== 1) return null;
    if (!/^[A-Za-z0-9_ ]*$/.test(values[0])) throw new Error('unsupported_list_expression');
    return values[0].split(/\s+/).filter(Boolean).sort();
  };
  const caps = words('CapabilityBoundingSet');
  const ambient = words('AmbientCapabilities');
  if (caps === null || JSON.stringify(caps) !== JSON.stringify([...profile.capabilities].sort())) findings.push('capability_set_differs');
  if (ambient === null || ambient.length !== 0) findings.push('ambient_not_explicitly_empty');
  const families = words('RestrictAddressFamilies');
  if (families === null) gaps.push('address_families_not_declared');
  else if (profile.families === null || JSON.stringify(families) !== JSON.stringify([...profile.families].sort())) findings.push('address_family_set_differs');
  const writes = service.get('ReadWritePaths') ?? [];
  if (writes.some(value => /["'~%\\]/.test(value))) gaps.push('write_path_expression_not_resolved');
  if (writes.some(value => value.split(/\s+/).some(p => ['/', '/etc', '/usr', '/var'].includes(p)))) findings.push('broad_write_path');
  const credentials = service.get('LoadCredential') ?? [];
  if (JSON.stringify(credentials) !== JSON.stringify(profile.credentials)) findings.push('credential_declaration_differs');
  const commands = [];
  for (const key of ['ExecStartPre', 'ExecStart', 'ExecStop', 'ExecStopPost']) {
    for (const value of service.get(key) ?? []) {
      const match = /^([-+!:@]*)(\/[A-Za-z0-9_./-]+)(?:\s|$)/.exec(value);
      if (!match || match[2].split('/').includes('..')) { gaps.push('command_expression_not_resolved'); continue; }
      if (/[+!]/.test(match[1])) findings.push('privileged_command_prefix');
      commands.push({directive: key, executable: match[2], prefix: match[1]});
    }
  }
  return {identity_declaration: identity, actual_uid_verified: false,
    group_declaration: scalar(service, 'Group'), supplementary_groups: service.get('SupplementaryGroups') ?? [],
    no_new_privileges: scalar(service, 'NoNewPrivileges'), capabilities: caps, ambient_capabilities: ambient,
    address_families: families, private_network: scalar(service, 'PrivateNetwork'),
    write_path_declarations: writes, credential_names: credentials.map(v => v.split(':')[0]),
    commands, findings: [...new Set(findings)], gaps: [...new Set(gaps)],
    effective_configuration_verified: false};
}

export function evaluateRules(sources, subject, action) {
  const context = vm.createContext({subject, action, rules: [], answer: 'NOT_HANDLED'});
  vm.runInContext('globalThis.polkit = Object.freeze({Result:Object.freeze({YES:"YES",NO:"NO"}),addRule:rule=>rules.push(rule)})', context, {timeout: 100});
  for (const source of sources) vm.runInContext(source, context, {timeout: 100});
  vm.runInContext('for (const rule of rules) { const result=rule(action,subject); if(result!==undefined && result!==null) { answer=result;break; } }', context, {timeout: 100});
  if (!['YES', 'NO', 'NOT_HANDLED'].includes(context.answer)) throw new Error('unexpected_rule_result');
  return context.answer;
}

export function policyCases() {
  const cases = [];
  const roles = ['rustsimhub-core', 'rustsimhub-edge', 'rustsimhub-media', 'rustsimhub-forwarder'];
  const allowed = ['org.freedesktop.ModemManager1.Messaging', 'org.freedesktop.ModemManager1.Voice',
    'org.freedesktop.NetworkManager.network-control', 'org.freedesktop.NetworkManager.checkpoint-rollback',
    'org.freedesktop.NetworkManager.settings.modify.system'];
  for (const role of roles) {
    for (const id of [...allowed, 'org.freedesktop.ModemManager1.Device.Control', 'org.freedesktop.NetworkManager.enable-disable-network']) {
      cases.push({role, id, unit: '', verb: '', expected: role === roles[0] && allowed.includes(id) ? 'YES' : 'NO'});
    }
    for (const unit of ['rustsimhub-forwarder-reconcile.service', 'rustsimhub-forwarder-status.service',
      'rustsimhub-core.service', 'rustsimhub-forwarder-reconcile.service/extra', 'other.service']) {
      for (const verb of ['start', 'stop', 'restart', 'Start']) {
        cases.push({role, id: 'org.freedesktop.systemd1.manage-units', unit, verb,
          expected: role === roles[0] && verb === 'start' && ['rustsimhub-forwarder-reconcile.service', 'rustsimhub-forwarder-status.service'].includes(unit) ? 'YES' : 'NO'});
      }
    }
  }
  // Negative controls: project rules should not claim to govern every OS user
  // or action namespace. NOT_HANDLED here is intentional, never a denial.
  for (const role of ['root', 'other', 'rustsimhub-core-extra']) cases.push({role,
    id: allowed[0], unit: '', verb: '', expected: 'NOT_HANDLED'});
  cases.push({role: roles[0], id: 'org.example.unreviewed', unit: '', verb: '', expected: 'NOT_HANDLED'});
  return cases;
}

export function audit(manifest) {
  if (manifest.schema !== 1 || !/^[a-f0-9]{40}$/.test(manifest.base_commit) || !Array.isArray(manifest.files) || !manifest.files.length || manifest.files.length > 32 ||
    manifest.files.some(item => !/^firmware\/system\/[a-zA-Z0-9_./-]+\.(?:service|rules)$/.test(item.path) || !/^[a-f0-9]{64}$/.test(item.sha256) || !['unit', 'polkit'].includes(item.kind)) ||
    new Set(manifest.files.map(item => item.path)).size !== manifest.files.length) throw new Error('invalid_bundled_manifest');
  const units = [], policies = [], sourceIssues = [];
  for (const item of manifest.files) {
    try {
      const text = readPublic(item.path);
      const digest = crypto.createHash('sha256').update(text).digest('hex');
      if (digest !== item.sha256) { sourceIssues.push({path: item.path, code: 'source_changed'}); continue; }
      if (item.kind === 'unit') units.push({path: item.path, sha256: digest, ...analyzeUnit(text, item.profile)});
      else if (item.kind === 'polkit') policies.push(text);
      else throw new Error('unknown_source_kind');
    } catch { sourceIssues.push({path: item.path, code: 'source_unreadable_or_unsupported'}); }
  }
  const matrix = [];
  if (policies.length === 2) {
    for (const item of policyCases()) {
      try {
        const action = {id: item.id, lookup: name => name === 'unit' ? item.unit : name === 'verb' ? item.verb : undefined};
        const actual = evaluateRules(policies, {user: item.role, active: false, local: false}, action);
        matrix.push({...item, actual, result: actual === item.expected ? 'matched' : actual === 'NOT_HANDLED' ? 'coverage_gap' : 'mismatch'});
      } catch { matrix.push({...item, actual: 'UNKNOWN', result: 'evaluation_error'}); }
    }
  } else sourceIssues.push({code: 'incomplete_polkit_sources'});
  const findings = units.reduce((n, row) => n + row.findings.length, 0) + matrix.filter(c => c.result === 'mismatch').length;
  const gaps = sourceIssues.length + units.reduce((n, row) => n + row.gaps.length, 0) + matrix.filter(c => ['coverage_gap', 'evaluation_error'].includes(c.result)).length;
  return {schema: 1, base_commit: manifest.base_commit, candidate_changed_files: manifest.candidate_changed_files,
    source_manifest_sha256: crypto.createHash('sha256').update(JSON.stringify(manifest)).digest('hex'),
    scope: 'isolated_public_project_rules_and_base_units',
    status: gaps ? 'incomplete' : findings ? 'findings' : 'reviewed_declarations_only',
    runtime_verified: false, production_approved: false, findings, gaps, source_issues: sourceIssues, units, polkit_matrix: matrix};
}

if (process.argv[1] && pathToFileURL(path.resolve(process.argv[1])).href === import.meta.url) {
  if (!process.argv.slice(2).length) {
    console.log(JSON.stringify({mode: 'plan', read_source_contents: false, run: 'node tools/security-matrix/audit.mjs --run',
      scope: 'bundled public base units and two isolated project polkit rules; no live authorization verdict'}));
  } else if (JSON.stringify(process.argv.slice(2)) !== '["--run"]') {
    console.error('Use no arguments for plan or --run for fixed public source audit.'); process.exitCode = 2;
  } else {
    try {
      const manifest = JSON.parse(readPublic('tools/security-matrix/sources.json'));
      const report = audit(manifest);
      console.log(JSON.stringify(report, null, 2));
      process.exitCode = report.status === 'incomplete' ? 2 : report.status === 'findings' ? 1 : 0;
    } catch { console.error('Public audit incomplete; no runtime verdict.'); process.exitCode = 2; }
  }
}
