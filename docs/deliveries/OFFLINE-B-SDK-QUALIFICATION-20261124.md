# OFFLINE-B SDK build and license qualification — 2026-11-24

This is an executed B1/B4/B5 subpackage, not production or device acceptance.
Starting main is `de663743bd726e77f7c560139d0f91ea78e8fe57`, tree
`897b7b485ce8b3108d359d859dd2384efba78df1`. The independent review branch is
`codex/offline-b-sdk-qualification-20261124`; its complete review HEAD is in the
PR body. This PR adds evidence and original license notices, not another SDK,
wire contract, service, runtime permission or build implementation.

## Actual fixed source

Executed source: **6746473b4d0cd4a1314327693fd80023a0c33cbd**, tree
`7108da944a5a21edead8c0a9ed1de6b163caf343`, real Git archive SHA256
`7bcfcd41694721f6cb2be106803429c15e13a461315f987f62aa5e45978040bc`.
It is the already verified PR270 function head 2bfe257 + PR271 b5a455e + PR278
2df2e811 combination; no PR was merged into main. This evidence PR is later and
must not relabel those artifacts as built from its own documentation HEAD.
Actual Git comparison confirms `tools/nkn-engine`, `tools/nkn-build` and
`docs/dependencies` are byte-identical between that source and starting main.

The original `tools/nkn-build/sdk-evaluation/source-manifest.json` pins 31 fork
files (273776 bytes) in `Viper-Boss/nknguard` at
`c32952d7586676fec10f71a30b22969cb24881f3`. Authorized connector reads fetched
only those files. Each length, Git blob SHA1 and SHA256 was checked on Windows
and independently again in the new Linux guest. Original Apache licenses and
NKNGuard modification notes remain in the generated source stages. The source
transport tar SHA256 is
`909a1a509c3c541816bc24c43798c553aeae93eb1335a4eee4ac9f64cd6deede`.
The official SDK version's Go checksum is **not** used to authenticate this fork.

Existing scripts independently validate and apply the committed compatibility,
WebSocket-only, upstream mask-backport and managed patches with exact before/
after file hashes and `--fuzz=0`. No patch, expected vector or A implementation
was edited. [Prepared source receipt](../offline-b/sdk-qualification-20261124/managed-inputs.json)
records every stage file; actual bytes were rehashed after all builds/tests and
equal that receipt. All eight actual binaries were reread to check byte length,
ELF architecture and SHA256 before exporting this evidence. Reports are not
substitutes for source or executable bytes.

The SDK checkout contains a newly generated, untracked `work/` directory, as the
original tools require. Tracked source and producer-scoped source paths are
unchanged; this is **not** a claim that the entire SDK working directory has no
untracked files. Generated work is not included in the Git archive. The separate
ARM Rust component build uses another entirely clean checkout.

## Toolchain, cache and isolation

The user allowed necessary tool installation. A new task-owned Debian 13/WHPX
VM, Linux 6.12.117 and actual systemd PID1 is used: two vCPUs, 512 MiB RAM and a
task-private 2 GiB swap file. The outer Windows Job caps memory at 3 GiB,
processes at 32 and lifetime at two hours per explicit session; expiry collected
the previous VM session before this continuation. No host filesystem/device,
original browser, private maintenance directory or production credentials is
attached. New synthetic management keys and strict known-host SSH use only
127.0.0.1:23109. General guest egress is blocked by QEMU; no install proxy tunnel
was connected during these tests.

The original evaluators require actual **Go 1.27.3**, which was separately
installed in the guest task directory. Official Linux archive: 70588678 bytes,
SHA256 `65efbc80fdc1fa2cd2631cad5b481777a68e8725f34e89f50fc6ac714e379b40`,
verified against Go's official release JSON before extraction and again inside
the guest. Compiler, runtime and original module versions are not silently
upgraded. Native C compiler is available for Go's race detector.

Dependency preparation fetched only the unchanged SDK `go.sum` entries and the
original fixed WebSocket version manifest from Go's official module proxy:
230 files, 18890291 bytes (43 ZIPs, 144 go.mod files, 43 version-info files).
The actual Go Hash1 algorithm was checked against the installed official Go
source. go.mod and ZIP sums match the original h1 values; the three WebSocket
archives also match their pinned raw archive SHA256. Subsequently official Go
independently verified all 43 ZIP/module sums, and `go mod verify` passed. Cache
transport SHA256:
`0b35bd810c94bd1fd8c79fe5911a5772bc25da2cde64deab44b0d913bab80a2a`.
These cached records are not described as the linked module closure.

Every execution sets `GOPROXY=off`, `GOSUMDB=off`, `GOTOOLCHAIN=local`,
`GOWORK=off`, `GOENV=off`; original module/lock bytes are preserved. Existing
scripts enforce task `/workspace` caches and repository `work/` stages. Test
units use an ordinary builder UID, no capabilities, NoNewPrivileges, private
network/tmp/IPC/devices, read-only system and tracked source, writable task
caches/generated work only, 384 MiB memory, 64 tasks, one CPU and a finite
900–1800 s unit lifetime. Original per-probe 15/20/45 s and command 120/180/240 s
deadlines remain unchanged. Whole-cgroup cleanup collects descendants.

## Actual original evaluations

All commands below exited 0; paths are absolute task paths under the original
source's `work/`. They are executed in the restricted units above, with the
verified source cache, module/build caches and pinned Go on PATH. No online
advisory/scanner flags, public SDK run or real Root service is used.

```sh
python3 tools/nkn-build/sdk-evaluation/evaluate.py --run \
  --source-cache SOURCE/work/sdk-source --output SOURCE/work/sdk-evaluation-20261123
python3 tools/nkn-build/sdk-websocket-security/evaluate.py --run \
  --source-cache SOURCE/work/sdk-source --output SOURCE/work/sdk-websocket-20261123
python3 tools/nkn-build/sdk-status/evaluate.py --run \
  --verified-sdk-stage SOURCE/work/sdk-websocket-20261123/backport-stage \
  --output SOURCE/work/sdk-status-20261123
python3 tools/nkn-build/sdk-managed/prepare.py \
  --verified-sdk-stage SOURCE/work/sdk-websocket-20261123/backport-stage \
  --output SOURCE/work/sdk-managed-20261124
python3 tools/nkn-build/sdk-managed/build.py \
  --stage SOURCE/work/sdk-managed-20261124/stage \
  --output SOURCE/work/sdk-managed-build-20261124
python3 tools/nkn-build/sdk-managed/notices.py \
  --stage SOURCE/work/sdk-managed-20261124/stage \
  --build SOURCE/work/sdk-managed-build-20261124 \
  --output SOURCE/work/sdk-managed-notices-20261124
```

The basic evaluator verifies all 31 source files and its six source/patch
negative tests, then executes five real SDK cases and four original upstream
cases before and after the compatibility patch, plus strict vet, package/
license inventory and native test artifacts. A baseline pass confirms an
observed limitation; it does not certify the unpatched fork. [Original report](../offline-b/sdk-qualification-20261124/sdk-base-report.json).

The WebSocket evaluator consumes the original compatibility/minimal patch chain;
the separate minimal-evaluator command was not claimed as run. It compares
actual frame masks for v1.4.2, v1.5.3 and the pinned v1.5.3 backport, runs seven
SDK cases per version, four original SDK cases, original prepared-message
regression, vet and native/ARM test builds. The controlled mask test alone opts
into `GODEBUG=randseednop=0`; old versions' 16 frame masks match controlled
math/rand, while the backport does not. Ordinary SDK tests/builds do not adopt
that legacy setting. This is an observed fixed-source result, not a current
unconditional advisory verdict. [Original report](../offline-b/sdk-qualification-20261124/sdk-websocket-report.json).

The status evaluator runs four actual SDK-source tests, ten bridge/protocol
tests, two original production-JavaScript projection samples, vet and native/
ARM test builds. It preserves unavailable/unknown state and does not infer
Owner authorization, real route/DHT or production readiness. [Original report](../offline-b/sdk-qualification-20261124/sdk-status-report.json).

## Complete SDK and Root builds

The original whole-program builder creates **eight** actual executables: SDK
and Root × amd64/ARM64 × two independent rounds, using four new clean caches.
It checks actual ELF identities, linked-package inventory, build tags, SDK
pure-version metadata and source binding; each program/architecture's two
binary SHA256 values are equal. These are complete programs, distinct from
the earlier test probes. [Actual build report](../offline-b/sdk-qualification-20261124/managed-build-report.json).

| Program | Architecture | Bytes | SHA256 (both rounds) |
| --- | --- | ---: | --- |
| nkn-sdk-engine | amd64 | 11387004 | `a4d15bcfd4ba8e1887ada45230f142b9316739f55e5544ff797530bc8b17a892` |
| nkn-root | amd64 | 3670140 | `afb3b6b8be47a18962cc433bcfd2681914aed01b67b7141cd1e0719c03ec16f7` |
| nkn-sdk-engine | arm64 | 10616956 | `5044e68da3d7747d6763f54b4ed9ac4bdabd11b23fd4b9d62c744444b8b72505` |
| nkn-root | arm64 | 3473532 | `42e7b7484d089d9976bdd33cf88dc9086bc7e1005c6831ce320e53795604e4b5` |

The build artifacts use the original `nkn_sdk_candidate` tag. They are not the
separate transaction-tag artifact build, are not `nkn_sdk_fixture`, and are not
signed or installable. Native SDK `--version` reports the actual source HEAD,
linked SDK, required Root launch and fixture=false. The Root program has no
`--version` path and was not launched; ARM executables were not run by this
builder. Compilation, version output and binary repeatability do not prove
live Root authorization or runtime health.

## Two actual SDK test modes and independent vectors

In that exact managed stage, original tests run separately with
`nkn_sdk_candidate` and `nkn_sdk_candidate,nkn_usage_transaction_candidate`:

```sh
go test -tags=MODE -race -mod=readonly -count=1 -p=1 -timeout=45s -json \
  github.com/Viper-Boss/RustSimHub/tools/nkn-engine/... github.com/nknorg/nkn-sdk-go
go vet -tags=MODE -mod=readonly -p=1 \
  github.com/Viper-Boss/RustSimHub/tools/nkn-engine/... github.com/nknorg/nkn-sdk-go
```

Each mode really passes **61 top-level tests, zero test failures/skips**, and
vet exits 0. Counts overlap and are not added as 122 distinct tests. This is
separate from the prior default SDK-unavailable Go module's 55 tests. All
transaction behavior is synthetic/in-memory or explicitly owned local fixtures,
not a real chain/RPC endpoint. [Mode inventory and byte recheck](../offline-b/sdk-qualification-20261124/modes-summary.json).

The original `usage-vectors/run.py` independently rebuilds the **unmodified
fixed fork's** pure encoder from the 31-file cache, with the original Go module
and sum. Fresh output is byte-identical to the committed 15 vectors (SHA256
`4e3b33a2beee10c61c3ba08df0f6938ddf6c73e33ddf2890f02c368e6242550e`), verifies
30 fresh-signature rejections and four intentionally failing encodings:
intent hash, single hash, big endian and SDK attributes. The program asserts
zero HTTP calls; four negative commands exit exactly 1 with the expected
behavioral assertion. Format, module verification, build and vet exit 0.
[Actual vector result](../offline-b/sdk-qualification-20261124/vectors-result.json).

Reproduce with the original script's explicit `--go`, `--source-cache`,
`--module-cache`, `--build-cache` and Linux-native `--work-root` paths; run it as
an ordinary user. No committed expected bytes were regenerated or replaced.

## Actual license closure and retained failures

The notices tool checks every one of the eight actual artifacts and their
whole-program module inventories. The union is **17 modules including this
repository**, with no newly introduced external modules. Per-architecture
inventories differ, as recorded; this is not the 230 cache records or the
earlier 37-module legacy probe closure. It verifies original license file
hashes and the actual Go runtime/standard-library license.
[Module/license evidence](../offline-b/sdk-qualification-20261124/modules.json) and
[complete original notices](../offline-b/sdk-qualification-20261124/THIRD-PARTY-NOTICES.txt),
SHA256 `655533f3569189d93d21cab1980f6337d5cd02f50f40c5e040ff7440f07e9286`.

`dependency_or_production_approval` remains false. Previous x/crypto, x/sys and
recorded advisory dispositions remain for review. No fresh online advisory or
govuln scan was run. Linux/netfilter/nftables, Rust components, ALSA/Opus and a
complete product release's system-library closure are outside this Go notice
verdict; there is no unconditional security or license approval.

Original failure evidence is retained outside the public source:

- Twelve initial fixed dependency transfers failed TLS EOF/timeout. The first
  manifest was retained; one bounded retry fetched only missing records, with
  all TLS and original sum checks intact.
- The first compile filled the private `/tmp` filesystem before tests ran; it
  was not marked successful. Go build temporaries moved to task-owned disk
  cache, and official Go initialized/rechecked all 43 zip hashes before the
  original evaluators. No module/lock was changed.
- The first additional SDK-mode run had six Unix-socket `bind: invalid
  argument` failures because the long GOTMPDIR-derived paths exceeded AF_UNIX
  limits. A wrapper's `${...}` expansion also produced a non-specific log name;
  that failed run was preserved and the second mode had not run yet. Two fixed
  commands and a new short, private, builder-owned `/b` scratch directory
  corrected those environment preconditions. Original source, assertions and
  timeout values stayed unchanged; both complete modes then passed separately.
- Export initially failed on an internal newline-escaping syntax error before
  reading/copying evidence. It was corrected; all nine permitted public files
  were transferred over task-only SFTP with fresh source length/SHA256 and
  destination rechecks. No raw SDK logs, runtime private data or keys are added.

## Handoff and unfinished scope

The original managed source stage and actual binaries remain in the task-owned
VM for the original release builder to consume after independent review. These
JSON snapshots and license texts are audit material, not a substitute for that
builder's source/hash/module/role checks. No new application/install format is
invented. No A/SDK/Core/Edge/Root production code, identity, role/UID assignment,
default boot, permission duration, workspace dependency/lock or global progress
was modified. No public NKN/RPC/Bark, real messages, chain submission, NAS or
production signing/deployment occurred.

This closes the stated SDK build/license offline subpackage, not all 29 B
acceptance actions. ARM Rust components/QEMU observations, actual Root/UID/
cgroup/nft/health and recovery failure matrices, and remaining safety/release
material continue separately. Full original profiles still require their
matching ARM/audio sysroots, complete same-source program set and independently
approved NSS/role input; a generic new cross sysroot is not that approval.
Physical devices, power failure, second board, real network and 72-hour
acceptance remain with the maintainer. All production/install/health/hardware/
public-network authorization flags remain false; **overall 81.5% unchanged**.

Previous Windows/Linux runner and Root fixture delivery stays in PR270/271/278/
279. Current CI is read separately after publishing this PR; no local result is
called a cloud pass, and no retry/billing/protection changes are made. This
subpackage is submitted for review without self-merge. AGPL-3.0-only project
tools and original third-party notices are retained. Codex executed the stated
checks without inventing authors or hours. Rollback removes this evidence
subpackage; it does not restore or alter business data.
