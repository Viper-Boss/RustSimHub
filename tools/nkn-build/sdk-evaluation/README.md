# Fixed NKN SDK compatibility candidate (A13–16)

This evaluates the exact NKNGuard-local nkn-sdk-go/NCP fork at
`Viper-Boss/nknguard@c32952d7586676fec10f71a30b22969cb24881f3`, rather than
pretending an official v1.4.8 Go checksum authenticates local modifications.
`source-manifest.json` pins all 31 fork files by Git blob, SHA256 and length.
The original Apache notices and NKNGuard patch notes remain in each generated
work stage. No SDK implementation, cache, binary or private identity is vendored
in this repository. The source and patch are explicit review candidates.

It does **not** link an SDK into the existing stdio engine, allocate a UID/Root
policy, obtain credentials, grant network permission, or implement Core/Owner/
secure-session authentication. The actual product frontend still reports
unavailable. Default invocation prints a plan without downloads/builds/listeners.

## Real baseline and candidate tests

Only synthetic independent accounts and an explicitly owned ephemeral loopback
HTTP/WebSocket fixture are used. Other dial attempts are refused **before** DNS
or socket creation; no public NKN/RPC/STUN, transaction or chain operation occurs.
The native probe is finite. The parent bounds and kills only its own process
group on timeout; SDK `IsClosed()` is never treated as proof of process exit.

The same five actual SDK tests run twice, with explicit baseline/patched modes:

| Trigger | Unmodified fixed fork (observed) | Candidate patch (asserted) |
| --- | --- | --- |
| Initial connect denied | Constructor returns nil/error but leaves a reconnect worker without a returned handle | Constructor calls nil-safe Close; original reconnect worker exits |
| Wallet read after dial policy denies | Wallet RPC bypasses the client dial callback and reaches the loopback fixture | Wallet retains timeout/concurrency/dial callback; request is denied before a socket |
| Close during an active reconnect loop | `IsClosed=true` while more denied dial attempts continue | One lifecycle context cancels pending RPC/WebSocket and retry timers; no further attempts after worker exit |
| Malformed RPC body | Full synthetic body enters the upstream logger | Raw body logging removed; parse failure still fails |
| 2 MiB valid padded response | Entire response accepted without an application limit | Explicit **1 MiB RPC body** cap rejects before JSON parsing |

A baseline test pass confirms a limitation; it does not certify the old fork as
safe. The candidate is a **186-line compatibility diff**, not a rewritten SDK or
crypto algorithm. It also holds the SDK lock across reconnect enqueue to avoid
send/close-channel races and rejects a connection arriving after retirement.
The context/callback changes are internal Go lifecycle changes, not a new wire,
identity, Root lease or automatic retry authorization format.

The 1 MiB body limit is a proposed SDK compatibility choice, separate from the
2048B local T5/stdio record budget. Existing large subscriber/RPC uses may need
review. It is not an approval to enlarge local frames. The patch does not prove
that every SDK log/error is redacted, every cache janitor is synchronously gone,
or every secret buffer is wiped. An owned child process and reviewed complete
Root/cgroup cleanup remain necessary. Custom dialers must actually honor context.
WebRTC/STUN and public delivery are not evaluated or enabled.

Four original SDK/NCP in-memory routing tests run with `-race` before and after
the patch; their original authors and source are preserved, not counted as new
tests. Six Python source/patch tamper checks reject changed code, linked notices,
duplicate manifest entries, patch digest changes and unexpected patch targets.

## Reproduce

Requires Go1.27.3, a native C compiler for Go's race detector, Python3 and `patch`.
Keep all module/build caches outside the home directory and preserve the session
proxy and CA configuration. Example preparation, using a NEW directory:

```sh
# From repository root; only the explicit fetch option downloads fixed public source.
python3 tools/nkn-build/sdk-evaluation/evaluate.py
python3 tools/nkn-build/sdk-evaluation/evaluate.py --fetch-public-source --source-cache "$PWD/work/sdk-source"
export GOMODCACHE=/workspace/toolchains/go-mod-cache
export GOCACHE=/workspace/toolchains/go-build-cache
export GOPATH=/workspace/toolchains/go-path
export GOTOOLCHAIN=local GOWORK=off GOENV=off
mkdir -p work/sdk-prepare/source
cp -R work/sdk-source/third_party work/sdk-prepare/source/
cp tools/nkn-build/sdk-evaluation/{go.mod,go.sum,sdk_test.go} work/sdk-prepare/
(cd work/sdk-prepare && go mod download)
python3 tools/nkn-build/sdk-evaluation/evaluate.py --run --online-advisories \
  --source-cache "$PWD/work/sdk-source" --output "$PWD/work/sdk-evaluation"
```

Do not run `go test` directly in this template directory: its relative
replacements intentionally refer to source copied only inside the generated
work stage. Evaluation uses `GOPROXY=off`, `GOSUMDB=off`, `-mod=readonly` and
`go mod verify` after explicit, checksummed dependency preparation. It never
silently enables downloads, upgrades modules or patches the registry cache.

The script verifies source first, runs the unmodified tests, creates a second
owned stage, checks the patch digest/targets/before hashes, applies with
`--fuzz=0`, runs the patched tests and original routing regressions, then checks
all after hashes and unchanged files. It records exact candidate Git/input
hashes and refuses a revision/input change during evaluation.

It builds **test probes** for Linux amd64/arm64, checks the real ELF machines,
and actually executes the final native probe. ARM compilation is not ARM
execution. These are not engine release artifacts, independent reproducibility
builds, signed packages or installations. `-buildvcs=false` prevents a generated
work stage from falsely inheriting its parent's Git metadata.

Dependency evidence covers the **actually compiled native test closure**, not
every unselected module in the upstream historical graph. Go module sums cover
registry modules; local fork/patch provenance is verified separately. License
file hashes are evidence, not approval. Optional OSV queries use only public
module/version pairs; module matches do not prove function-level reachability
or validate local-fork patches. Known findings remain blockers for dependency
review; no mass upgrade is hidden in this candidate.

New harness/tooling is AGPL-3.0-only, OpenAI Codex assisted. Original SDK/NCP
notices, NKNGuard modifications, source versions and authors remain unchanged;
the generated patch stage is a separately hashed derivative for review. No
production device, NAS, Owner/SIM material, public NKN, signing or deployment.
