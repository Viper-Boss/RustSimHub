# A16 WebSocket mask remediation candidate

This continues merged PR157/161's isolated SDK evaluation. It does not modify
their implementation, the product stdio frontend, a registry cache or deployed
services. Default invocation only prints a plan. The real SDK still has no
approved product backend, identity, Root policy or fixed network provider.

## The database range is insufficient

GO-2026-6278 names v1.5.3 as fixed. The official checksum-verified v1.5.3 archive
still implements `newMaskKey` using `math/rand.Uint32`. Its tag commit is
`ce903f6d1d961af3a8602f2842c8b1c3fca58c4d` (2024-06-14). The referenced upstream
fix `d67f41855da42d7bccd9ef050c49f7e54e783b95` is later (2024-07-01).

This is observed in actual frames, not inferred solely from the advisory:
`mask_test.go` performs the real Gorilla client handshake and captures sixteen
masked text frames over an owned `net.Pipe`. Both v1.4.2 and v1.5.3 match the
controlled legacy `math/rand` sequence. The backport below does not. The test
sets `randseednop=0` only in its own isolated process and first proves that Seed
control worked; SDK regressions and product flags retain ordinary settings.
This demonstrates the selected mask source; it is not a statistical RNG audit,
a production exploit or a guarantee about every environment's entropy failure.

Govulncheck1.9.0 on the actual SDK package reports the original call chain
`NewClient → connect → connectToNode → websocket.DialContext`. Its range hides
the v1.5.3 problem, so a scan with no finding cannot be the remediation test.
Scanning only the test-only wrapper can omit dependencies; this runner uses
the actual SDK root and rejects a missing SDK/WebSocket/x/crypto SBOM closure.
Binary symbol presence is distinct from a source call chain or runtime exploit.

## The precise candidate

`versions.json` pins three official Go module downloads by sums, archive digest,
source/notice digest and available Git origin. The later fix's pseudo-version
would add x/net/proxy. To preserve PR161's smaller closure, this candidate:

1. Evaluates the original v1.4.2 and nominal v1.5.3 separately.
2. Copies the verified v1.5.3 module to a new work stage, preserving every file
   and original BSD notice, then applies the two-file upstream mask backport
   with fuzz=0 and exact before/after/tree hashes.
3. Reuses the upstream `crypto/rand.Reader` mask implementation verbatim and its
   prepared-message test changes. Unrelated later proxy/module changes are not
   taken. Only the generated candidate uses an explicit local replacement;
   its derivative is not authenticated by the unmodified release's Go sum.

The patch preserves the upstream `_ = io.ReadFull` error handling. It adds no
new fail-closed entropy wrapper or wiping guarantee. That behavior, the exact
backport, release compatibility and dependency disposition require review.
No application encryption, grant, identity, NKN wire or Root format is frozen.

The runner compares all three variants with the actual frame test and the same
seven inherited SDK lifecycle/config/RPC cases under `-race`. It also runs the
four original SDK/NCP routing tests and the original Gorilla prepared-message
test on the candidate, retaining their authors. One new test across three
versions is one new case, not three new tests. It checks Go vet, own/production
patch gofmt, registry/source integrity, native/ARM64 package closures and real
ELF test probes. Only the native artifact executes. These are not engine release
artifacts or independent-cache reproducibility builds.

## Reproduce

Use Go1.27.3, Python3, `patch`, a native C compiler and non-home Go caches. First
prepare the fixed public SDK cache and old dependencies as described in the
previous evaluation. Explicit additional public preparation:

```sh
export GOMODCACHE=/workspace/toolchains/go-mod-cache
export GOCACHE=/workspace/toolchains/go-build-cache
export GOPATH=/workspace/toolchains/go-path
export GOTOOLCHAIN=local GOWORK=off GOENV=off
go mod download github.com/gorilla/websocket@v1.4.2 \
  github.com/gorilla/websocket@v1.5.3 \
  github.com/gorilla/websocket@v1.5.4-0.20240701034025-d67f41855da4
# Optional public vulnerability scanner, tooling only; not a product dependency.
GOBIN=/workspace/toolchains/security-eval-1.9/bin \
  go install golang.org/x/vuln/cmd/govulncheck@v1.9.0
mkdir -p work/sdk-security
python3 tools/nkn-build/sdk-websocket-security/evaluate.py  # plan only
python3 tools/nkn-build/sdk-websocket-security/evaluate.py --run \
  --source-cache "$PWD/work/sdk-source" --output "$PWD/work/sdk-security/run-1" \
  --online-scans --govulncheck /workspace/toolchains/security-eval-1.9/bin/govulncheck
```

Use a NEW output directory. Without both explicit scan options the runner does
not invoke the online scanner. SDK builds/tests use GOPROXY/GOSUMDB=off and
mod=readonly; exact-version module lookup uses only previously verified cached
archives, with downloads disabled. The registry's original tree is checked
unchanged after the candidate runs. Only the newly owned derivative's read-only
copy permissions are adjusted. No cache permissions or source are changed.

Success connections are only the owned pipe and inherited loopback fixtures;
other dials are refused before DNS/socket. No public NKN/RPC/STUN/transaction
occurs. Scanner requests use public modules/versions. Each child is bounded and
timeout kills only its own new process group. Exact Git/input hashes are checked
before and after evaluation; artifacts/logs belong under ignored work/.

x/crypto and x/sys module matches remain recorded. Their currently affected
packages are absent from these Linux closures; absence here does not certify
other targets, all SDK APIs or future feature changes. No production acceptance,
device run, Root/cgroup completion, deployment or global progress is claimed.

New tooling/tests are AGPL-3.0-only and OpenAI Codex assisted. Original SDK/NCP,
Gorilla BSD and Go tool/runtime notices and authors remain attributed. No vendor,
downloaded source, binary, cache or real identity is committed.
