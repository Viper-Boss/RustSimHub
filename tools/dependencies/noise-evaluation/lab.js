// SPDX-License-Identifier: AGPL-3.0-only
// Only served by the loopback evaluation harness; never a product asset.
import init, {EvalSession, verify_public_vectors} from '/pkg/noise.js';
const sessions = new Map();
let next = 0;
const decode = text => Uint8Array.from(text.match(/../g) || [], x => parseInt(x, 16));
const encode = bytes => Array.from(bytes, x => x.toString(16).padStart(2, '0')).join('');
try {
  await init({module_or_path: '/pkg/noise_bg.wasm'});
  window.noiseLab = {
    vectors: () => verify_public_vectors(),
    create: initiator => {
      if (sessions.size >= 4) throw new Error('fixture capacity');
      const session = new EvalSession(initiator);
      const id = ++next;
      sessions.set(id, session);
      return id;
    },
    call: (id, op, body = '') => {
      const session = sessions.get(id);
      if (!session) return {ok: false};
      try {
        if (op === 'finish' || op === 'close') {
          session[op]();
          return {ok: true, closed: session.closed()};
        }
        if (op === 'hash') return {ok: true, body: encode(session.handshake_hash())};
        if (!['write_handshake', 'read_handshake', 'write_transport', 'read_transport'].includes(op)) throw new Error('fixture op');
        return {ok: true, body: encode(session[op](decode(body))), closed: session.closed()};
      } catch {
        return {ok: false, closed: session.closed()};
      }
    },
    dispose: id => {const session = sessions.get(id); if (session) {session.close(); session.free(); sessions.delete(id);}},
  };
  window.noiseBoot = {ok: true};
} catch (error) {
  window.noiseBoot = {ok: false, error: String(error)};
}
