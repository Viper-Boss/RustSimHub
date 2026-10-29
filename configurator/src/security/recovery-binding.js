// SPDX-License-Identifier: AGPL-3.0-only
// Independent non-exportable key binding. Grant metadata never grants Core authority.
const DATABASE = 'rustsimhub-recovery-bindings-v1';
const STORE = 'bindings';
const hex = value => /^[0-9a-f]{64}$/.test(value);
const uuid = value => typeof value === 'string' && /^[0-9a-f]{8}(?:-[0-9a-f]{4}){3}-[0-9a-f]{12}$/.test(value);
const fail = code => { throw new Error(code); };
function namespace(pin, realm) {
  if (typeof pin !== 'string' || !hex(pin) || !uuid(realm)) fail('invalid_binding');
  return `${pin}:${realm}`;
}
function generation(value) {
  return typeof value === 'string' && /^[1-9][0-9]{0,18}$/.test(value)
    && BigInt(value) < 9223372036854775807n;
}
function reference(value) {
  return value && Object.keys(value).sort().join(',') === 'active,generation,id'
    && uuid(value.id) && generation(value.generation) && typeof value.active === 'boolean';
}
function validate(record, pin, realm) {
  if (!record || record.format !== 1 || record.device_pin !== pin || record.realm !== realm
    || !Number.isSafeInteger(record.revision) || record.revision < 1
    || typeof record.public_key !== 'string' || !hex(record.public_key)
    || !(record.private_key instanceof CryptoKey) || record.private_key.type !== 'private'
    || record.private_key.extractable || record.private_key.algorithm.name !== 'Ed25519'
    || record.private_key.usages.join(',') !== 'sign'
    || (record.grant !== null && !reference(record.grant))) fail('corrupt_binding');
  return record;
}
function view(record) {
  return Object.freeze({ format: 1, device_pin: record.device_pin, realm: record.realm,
    revision: record.revision, public_key: record.public_key,
    grant: record.grant ? Object.freeze({ ...record.grant }) : null });
}
async function database() {
  if (!globalThis.isSecureContext || !globalThis.crypto?.subtle || !globalThis.indexedDB
    || typeof CryptoKey !== 'function') fail('unsupported');
  return new Promise((resolve, reject) => {
    const open = indexedDB.open(DATABASE, 1);
    open.onupgradeneeded = event => {
      if (event.oldVersion !== 0) { open.transaction.abort(); return; }
      open.result.createObjectStore(STORE);
    };
    open.onerror = () => reject(new Error('binding_storage_unavailable'));
    open.onblocked = () => reject(new Error('binding_storage_blocked'));
    open.onsuccess = () => {
      const db = open.result;
      if (db.objectStoreNames.length !== 1 || !db.objectStoreNames.contains(STORE)) {
        db.close(); reject(new Error('corrupt_binding_store')); return;
      }
      db.onversionchange = () => db.close();
      resolve(db);
    };
  });
}
// Callback is synchronous inside the actual IDB transaction, so CAS cannot cross an await.
async function transaction(pin, realm, mode, change) {
  const key = namespace(pin, realm);
  const db = await database();
  try {
    return await new Promise((resolve, reject) => {
      const tx = db.transaction(STORE, mode);
      const store = tx.objectStore(STORE);
      let result, failure;
      tx.oncomplete = () => resolve(result);
      tx.onabort = () => reject(failure || new Error('binding_storage_unavailable'));
      tx.onerror = () => {};
      const read = store.get(key);
      read.onsuccess = () => {
        try {
          const current = read.result === undefined ? null : validate(read.result, pin, realm);
          result = change(current, store, key);
        } catch (error) { failure = error; tx.abort(); }
      };
    });
  } finally { db.close(); }
}
export async function readBinding(pin, realm) {
  return transaction(pin, realm, 'readonly', current => current ? view(current) : null);
}
export async function getOrCreateBinding(pin, realm) {
  const existing = await readBinding(pin, realm);
  if (existing) return existing;
  let pair;
  try { pair = await crypto.subtle.generateKey({ name: 'Ed25519' }, false, ['sign', 'verify']); }
  catch { fail('unsupported'); }
  if (pair.privateKey.extractable) fail('unsupported');
  const publicBytes = new Uint8Array(await crypto.subtle.exportKey('raw', pair.publicKey));
  if (publicBytes.byteLength !== 32) fail('unsupported');
  const record = { format: 1, device_pin: pin, realm, revision: 1,
    public_key: Array.from(publicBytes, byte => byte.toString(16).padStart(2, '0')).join(''),
    private_key: pair.privateKey, grant: null };
  return transaction(pin, realm, 'readwrite', (current, store, key) => {
    if (current) return view(current); // A concurrent tab won; never replace its key.
    validate(record, pin, realm);
    store.add(record, key);
    return view(record);
  });
}
// Called only after the management adapter verified the actual Core/device response.
// Persisting this reference is not an authorize() result and cannot mint a capability.
export async function rememberGrant(pin, realm, expectedRevision, response) {
  namespace(pin, realm);
  if (!response || response.device_pin !== pin || response.realm !== realm
    || !hex(response.public_key) || !reference(response.grant)) fail('invalid_grant_reference');
  return transaction(pin, realm, 'readwrite', (current, store, key) => {
    if (!current || current.revision !== expectedRevision) fail('binding_conflict');
    if (current.public_key !== response.public_key || current.revision >= Number.MAX_SAFE_INTEGER) fail('binding_conflict');
    if (current.grant && (current.grant.id !== response.grant.id
      || BigInt(response.grant.generation) <= BigInt(current.grant.generation))) fail('binding_conflict');
    const next = { ...current, revision: current.revision + 1, grant: { ...response.grant } };
    store.put(next, key);
    return view(next);
  });
}
// A vetted secure-session implementation consumes a non-exportable CryptoKey handle.
// This module does not define a signature transcript, handshake, encryption or network port.
export async function loadSigningKey(pin, realm, expectedRevision) {
  return transaction(pin, realm, 'readonly', current => {
    if (!current || current.revision !== expectedRevision || !current.grant?.active) fail('binding_unapproved');
    return current.private_key;
  });
}
export async function forgetBinding(pin, realm, expectedRevision) {
  return transaction(pin, realm, 'readwrite', (current, store, key) => {
    if (!current || current.revision !== expectedRevision) fail('binding_conflict');
    store.delete(key);
    // The device grant still requires explicit Owner revocation through the trusted channel.
    return Object.freeze({ forgotten: true, device_revoked: false });
  });
}
