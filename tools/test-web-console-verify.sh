#!/usr/bin/env bash
# SPDX-License-Identifier: AGPL-3.0-only
# Offline local verification; no device, existing browser profile, or real key.
set -euo pipefail
cd "$(dirname "$0")/.."
mkdir -p .local/web5-console
export CARGO_BUILD_JOBS=2
export CARGO_TARGET_DIR="${CARGO_TARGET_DIR:-$PWD/.local/web5-console/rust-target}"
export CARGO_PROFILE_RELEASE_OPT_LEVEL=z CARGO_PROFILE_RELEASE_LTO=false CARGO_PROFILE_RELEASE_PANIC=unwind
cargo test --release -p rustsimhub-storage --lib --locked --offline -- --test-threads=1
cargo test --release -p rustsimhub-storage --lib --all-features --locked --offline -- --test-threads=1
cargo clippy --release -p rustsimhub-storage --lib --tests --locked --offline -- -D warnings
TZ=UTC node tools/test-web-console-fencing.mjs
# Frozen old production, independent of current branch changes.
git show 7535535bc5a95c42ff1ea70743b97be561f2049a:crates/edge/src/web/console.js > .local/web5-console/baseline-console.js
TZ=UTC node tools/test-web-console-fencing.mjs .local/web5-console/baseline-console.js
node tools/test-web-password.mjs
node tools/test-web-session.mjs
node tools/test-web-daily-phone.mjs
node tools/test-web-daily-sms.mjs
node tools/test-web-console-browser.mjs
