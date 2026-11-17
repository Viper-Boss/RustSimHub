# A5 fixed-source validation

These tools record test observations. They do not build an installable release,
merge a PR, deploy, sign, or replace B's CI/packaging entry points. Run them from
the reviewed A5 commit and review `SOURCES` in `validate_a5.py` first. Each PR is
tested separately; this is **not** evidence that the unmerged PRs work together.

## Prerequisites

- Linux/WSL2, Python 3, git, util-linux (`unshare`, `setpriv`, `mount`), iproute2.
- The repository's fixed Rust toolchain, its locked offline Cargo cache, and
  `aarch64-unknown-linux-gnu` standard library. Rust tests use `CARGO_INCREMENTAL=0`.
- Node (this run: v24.22.1). No package download or browser installation is done.
- SDK lane: Go **1.27.3**, the original 31-file pinned fork cache, pinned
  WebSocket v1.5.3 zip and the locked Go module cache. All original compatibility,
  minimal, mask-backport and managed patches are independently verified/applied.
- Runtime USB lane: dbus-daemon. NAS lane: Go and CGO/race compiler support.
- Adequate disk/memory. Never fill the host disk to simulate ENOSPC.

Use an ordinary non-root test account. The root wrapper only creates the private
network/mount namespace and the disposable tmpfs, then drops all groups and
capabilities and sets NoNewPrivs. It refuses a root test UID. There is no arbitrary
root command or binary argument. Product source comes from exact Git commits in
a fresh local clone, without network fetches. Missing Git objects/caches/toolchains
are failures; install prerequisites separately rather than weakening verification.

## Rust/Node observations

Example from the desired repository root (paths are local examples, not device
paths). Choose a native Linux work/cache directory, not a Windows-mounted target.

```sh
RSH_REPO=$(pwd -P)
RSH_WORK="$HOME/.local/share/rsh-a5"
RSH_NODE=/absolute/path/to/node
for profile in main a2 a3 a4; do
  sudo python3 tools/offline-a/run_a5_isolated.py --uid "$(id -u)" --mode "$profile" -- \
    --repo "$RSH_REPO" --output "$RSH_REPO/.local/a5-$profile" \
    --work-root "$RSH_WORK" --cargo-target "$RSH_WORK/target" --node "$RSH_NODE" || break
done
```

Every output directory must be new. To retry, use another output directory and
retain the original failure. `result.json` has the actual source head/tree,
commands, exits, ignored counts, log SHA256 and copied test-binary SHA256. A zero
exit with zero selected Rust tests is rejected. Intermediate observations remain
`passed=false`; do not accept a partial report as the whole suite. Do not sum
overlapping suites into a unique-test or acceptance percentage.

The chosen subsets cover ordinary network/Auth/IPC, business persistence,
NKN/statistics, forwarding, and non-hotspot web modules. Hotspot-only,
BLE/AirSIM/audio, release/B and device suites are not run. `--all-features` in
Clippy compiles existing dependencies and test code; it does not execute those
excluded suites. ARM `cargo check` is compilation only, not QEMU or device proof.

## SDK chain and Go

```sh
sudo python3 tools/offline-a/run_a5_isolated.py --uid "$(id -u)" --mode sdk -- \
  --repo "$RSH_REPO" --output "$RSH_REPO/.local/a5-sdk" \
  --work-root "$RSH_WORK" --cargo-target "$RSH_WORK/target" --node "$RSH_NODE" \
  --go /absolute/path/to/go1.27.3/bin/go \
  --sdk-cache /absolute/path/to/original-31-file-cache \
  --websocket-zip /absolute/path/to/v1.5.3.zip \
  --go-path "$RSH_WORK/gopath" --go-mod-cache /absolute/path/to/locked-modules \
  --go-cache "$RSH_WORK/go-build"
```

`GOPROXY=off`, `GOSUMDB=off`, `GOTOOLCHAIN=local`, fixed source manifests,
archive hashes, module sums, before/after patch trees, readonly generated locks
and exact vector/engine copies remain enforced. Original licenses/notices stay
in generated stages. The SDK is upstream Go code, not a Rust reimplementation.
Native managed regressions and both build-tag combinations use race/vet. SDK/Root
ARM ELF programs are built, hash checked and **not executed**. Public RPC, public
NKN, transaction submission and real accounts are not exercised.

## Explicit isolated runtime checks

After successful `a3`/`a4` observations, point to their actual result files. These
commands use the freshly copied test binaries and verify every binary's SHA256.
Do not copy unknown executables from a test package; B builds from fixed source.

```sh
for mode in sms enospc nas; do
  sudo python3 tools/offline-a/run_a5_isolated.py --uid "$(id -u)" --mode "$mode" -- \
    --observation "$RSH_REPO/.local/a5-a3/result.json" \
    --output "$RSH_REPO/.local/a5-runtime-$mode" \
    --go /absolute/path/to/go1.27.3/bin/go --go-cache "$RSH_WORK/go-build" || break
done
sudo python3 tools/offline-a/run_a5_isolated.py --uid "$(id -u)" --mode usb -- \
  --observation "$RSH_REPO/.local/a5-a4/result.json" \
  --output "$RSH_REPO/.local/a5-runtime-usb"
```

- `sms`: two real Auth/IPC readers, cursor/trash/revocation/restart boundaries.
- `enospc`: actual bounded writes at five send boundaries, on a new **16MiB**
  tmpfs in a private mount namespace. Both runner and original Rust test check
  the namespace/filesystem/size. The wrapper unmounts and removes its empty
  generated directory. No host disk exhaustion is permitted.
- `nas`: original Core and Edge plus the real Go adapter, auth/control/revoke and
  original operation IDs. Only owned local services/fixtures are used and stopped.
- `usb`: private D-Bus only, actual Core actor and NM adapter, default/production
  combinations each with 17 ordinary/hidden cases. Original IDs/principals and
  Unknown semantics remain enforced. The private daemon is stopped afterward.

The earlier A2 UID/cgroup/SDK/Noise kernel fixture requires its original disposable
container and separate inputs; these tools do not run it on the host. Its prior
fixed-source result must be identified as historical evidence, not a new A5 run.
The same applies to real-browser evidence and original negative controls when
reused from an earlier fixed source. Rust/Go/Node success does not substitute for
GitHub Actions, release assembly, physical USB/Wi-Fi, cellular, or device acceptance.

Observer guard regression tests:

```sh
python3 tools/offline-a/test_a5_validation.py
```

They deliberately generate exit 42 and a zero-test result and assert rejection;
those intentional controls are not product failures or product test counts.
