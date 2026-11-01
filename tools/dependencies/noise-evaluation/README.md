# A27 isolated Noise dependency evaluation

This is a standalone **test and review workspace**, excluded from the production
Cargo workspace. It is not SecureSessionPortV1, RecoveryRequestVerifierV1, a
registered-client authentication adapter, or an application wire protocol. Do not
import it into Core, BLE, the configurator, or a published PWA. No listener,
device/Owner key, signature domain, pin, grant or persistent state is provided.

The candidate evaluates upstream `snow = 0.10.0` and the fixed standard suite
`Noise_XX_25519_AESGCM_SHA256`. Ordinary sessions generate independent random
keys. The embedded, publicly known fixed keys are exclusively a maintainer KAT,
not default credentials. XX message one's payload is plaintext: the test proves
this explicitly. Never supply Wi-Fi passwords or product data to this harness.

## Actual upstream feature blocker

The proposed minimum feature set (Curve25519/AES-GCM/SHA-2/getrandom) fails to
compile snow 0.10.0 at `src/resolvers/default.rs:326,349`, E0599: `KeyInit` is
imported only behind its ChaCha20Poly1305 feature. Reproduce without our wrapper:

```sh
cd tools/dependencies/noise-evaluation/fixtures/minimal-features
cargo check --locked --offline
# Expected exit 101 with snow's E0599 / KeyInit errors, not a successful build.
```

The **evaluation-only** manifest explicitly adds `use-chacha20poly1305`, preserving
the unmodified upstream source. Its extra packages are in the separate lock and
inventory. The harness still exposes only the fixed AES suite, without suite
negotiation or fallback. This dependency tradeoff needs review; do not silently
copy it into production or patch the crypto source to make a report green.

## Reproduce

Requires Rust 1.98, its wasm32-unknown-unknown target, wasm-bindgen CLI 0.2.130
(must match the pinned crate), Python with Playwright 1.56.0, and local Chromium.
The build-tool installation and `cargo fetch --locked` are explicit preparation
steps from official sources; `evaluate.py` never installs tools or resolves new
versions. Tool-only dependencies are separate from the candidate's inventory.
Use `cargo install wasm-bindgen-cli --version 0.2.130 --locked` in an isolated
toolchain prefix, not a product installation directory.

```sh
cd tools/dependencies/noise-evaluation
cargo fetch --locked
cargo fetch --manifest-path fixtures/minimal-features/Cargo.toml --locked
rustup target add wasm32-unknown-unknown
# Show a plan, without builds, subprocesses, browser or network requests:
python3 evaluate.py
# Use a NEW output directory under the repository's ignored work/ directory:
python3 evaluate.py --run --output /path/to/RustSimHub/work/noise-evaluation
# Optional public name/version-only OSV queries, never product/network traffic:
python3 evaluate.py --run --online-advisories --output /path/to/RustSimHub/work/noise-evaluation-osv
```

Run from this directory so the target-specific `.cargo/config.toml` is applied.
It explicitly selects getrandom's `wasm_js` backend; no Math.random or fixed-key
entropy fallback exists. All builds/checks run `--locked --offline`. Browser
interop uses an ephemeral loopback server, and only synthetic data. The parent
owns, bounds and reaps each native probe process. A failure is nonzero and is
never converted into product success.

The Rust library tests the maintainer's known ciphertexts, completed-handshake
and input limits, freshness, tamper/replay/reorder/direction/foreign sessions and
local close. The Chromium fixture runs the same WASM against the native process
in both initiator roles and both record directions; it checks real browser RNG,
RNG denial and real CSP enforcement. Public vectors are checked on both native
and WASM. Expected bytes come from the checksum-pinned crate, not this wrapper.

The candidate fixture's allowing policy adds only `wasm-unsafe-eval`; a separate
page with strict `script-src 'self'` must reject compilation, with an observed
browser CSP violation. No product CSP is changed. Passing this experiment does
not approve that policy or add any artifact to the real PWA. Generated JS/WASM
are measured against the existing 512 KiB/file and 2 MiB total ceilings.

`report.json` records exact source, dirty status, artifact hashes, the upstream
feature failure and explicit false production/wire/grant flags.
`dependencies.json` verifies locked registry archives and reports declarations,
license-file hashes, native/WASM dependency graphs and optional dated OSV results.
Missing packaged license text is reported, never treated as reviewed. OSV no
matches is a time-specific query result, not a security or license approval.

Snow's README explicitly says it has not received a formal audit. Source review
does not establish reliable key-buffer wiping; neither `close()`, Drop nor JS
`free()` is a proof that all secret bytes were erased. The harness rejects after
local failure/close but does not implement product revocation, deadlines, pin,
signatures, permission or Core side-effect gates. Those remain A27/D integration
requirements, along with ARM, Android and second-device verification.

## Provenance and notices

The registry archive SHA256 is
`599b506ccc4aff8cf7844bc42cf783009a434c1e26c964432560fb6d6ad02d82`;
the archive declares Git commit `4bb43f50370bdb3e8b1b57814ac662864db2704f`
at <https://github.com/mcginty/snow>. `inventory.verify_sources()` verifies the
selected one-of-408 vector from `tests/vectors/snow.txt` and the unchanged MIT
and Apache notices directly against the downloaded, checksum-pinned archive.
It checks 3 handshake and 2 transport messages. These are upstream-maintainer
vectors, not a claim of an official security certification. No additional
cacophony vectors or their uncertain external provenance are copied.

The fixture subset retains snow's MIT OR Apache-2.0 terms; complete original
notices are in `fixtures/snow-LICENSE-*`. New harness code is AGPL-3.0-only,
OpenAI Codex assisted. No third-party implementation or compiled artifact is
vendored/committed, and no original authors or licenses are rewritten.
