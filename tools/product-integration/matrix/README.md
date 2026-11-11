# Offline product combination matrix

These commands consume the original production updater, writer locks, migration
engines, WASM producer, bounded native crypto peer, and browser tests. Reports
are evidence only; they do not authorize installation, confirmation or publication.
The committed profile table keeps SMTP5/AP automatic health unsupported and keeps
the original same-Core, worker4, schema9, 5000ms/250ms health constraints.

Use a clean checkout of one full frozen commit. All output paths must be new
paths under this checkout's `work/`. Runtime programs must contain that commit's
`git archive --format=tar` SHA256 in their original build metadata. Each command
fails rather than substituting an old binary, mock crypto or missing runtime.

```sh
. /workspace/work/web4-env.sh
revision=$(git rev-parse HEAD)
python3 tools/product-integration/matrix/run.py --revision "$revision" wasm \
  --wasm-bindgen .local/web5-matrix/toolchain/wasm-bindgen --output work/matrix-wasm
python3 tools/product-integration/matrix/run.py --revision "$revision" modes \
  --programs .local/web5-matrix/programs --output work/matrix-modes
python3 tools/product-integration/matrix/run.py --revision "$revision" legacy --output work/matrix-legacy
python3 tools/product-integration/matrix/run.py --revision "$revision" email-recovery \
  --programs .local/web5-matrix/programs --test-driver .local/web5-matrix/update-email-tests \
  --no-email-tests .local/web5-matrix/update-default-tests \
  --worker4 .local/web5-matrix/worker4 --legacy-driver work/matrix-legacy/legacy-outbox \
  --runtime-image sha256:5ee1ac74e6bbbedad27abe1574d94faac1e73a12eab292dce78e40e718906ca8 \
  --output work/matrix-email-recovery
python3 -m unittest discover -s tools/product-integration/matrix -p 'test_*.py'
```

The WASM command builds the locked `wasm32-unknown-unknown` target with exact
wasm-bindgen 0.2.129, assembles the real configurator and CSP, builds the original
native peer, and executes both existing Chromium/WebCrypto test suites. A build
alone never produces a passing browser report. The peer is synthetic session
crypto, not a replacement for Core business authority. No radio is exercised.

The EmailRecovery command uses an immutable existing runtime image, disabled
container network, disposable volumes and actual isolated UID986. Its ignored
Rust tests require the isolated-root marker and cannot operate the host. Signed
fallback Core mismatch, SMTP5-Core/worker4 mismatch and a real updater built
without email support must fail before any live data or ledger mutation. After
the original email3-to4 migration commits, a test-only SIGSTOP exposes the same
original Snapshot holding both data writers and the application lease. The
parent proves real lock exclusion, kills the child, and uses a fresh explicit
inspection/recovery operation. The original business6-to9 engine then completes,
Core opens schema9, and SMTP5 automatic health refuses without confirming the
candidate. The stop hook is absent from production builds.

Keep build logs, raw fixture databases and generated temporary signing material
in ignored local output. Publish only source/artifact hashes and sanitized
machine-readable results. Record command failures and unexecuted combinations
in the delivery report; never infer hardware acceptance from a container run.

The `legacy` subcommand uses the existing locked separate format3 Cargo graph
with this package's `legacy-outbox.rs`. Original recent-Owner configuration,
claim/finish and Owner disable create confirmed/Unknown records with real
attempt IDs; durable OFF prevents later worker transmission. The full combined
chain checks these exact IDs/states/attempt IDs both after migration and after
Core opens schema9. An empty historical fixture cannot satisfy those assertions.
