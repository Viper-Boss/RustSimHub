# A22 actual SDK read-only status adapter

`tools/nkn-engine/sdkstatus` is compiled only with `nkn_sdk_candidate`. It takes
an already owned real `*sdk.Client` and a nonzero host launch generation. It
never constructs a client, generates/reads an identity, dials, signs, renews a
lease, toggles an Owner switch, or closes a shared client. `Retire` invalidates
only this observation context and drops its reference.

The actual SDK's `IsClosed()` supplies one limited observation. It does not
prove node reachability, process exit, Owner intent, Core authorization or a
healthy NKN path. The adapter therefore retains unknown enabled/service/path/
DHT, null counters/RTT/node count, and reports only the actual SDK closed flag
as the finite `engine_unavailable` error. Open means no error was provided by
that observation; it does not mean connected. Sample age covers this actual
local read, never a fabricated network or process observation.

Generation comes from the trusted host, not a request. Sequence is strictly
increasing and never reset within a source; a new bridge cannot silently reuse
old sequence one. Cancellation, retirement, clock rollback and late completion
reject output. The SDK mutex itself is upstream code; late-completion checks
are not a promise to preempt a stuck library lock. The host must retain an
independent bounded process/stop boundary, as required by the supervisor.

`bridge.ServeWithSource` consumes this in-process interface using the existing
status/stop framing. It rejects invalid, stale or private provider fields before
serializing. A source failure produces finite `status_unavailable`, with no raw
error text. `last_error=null` now represents T5's no-provided-error case; the
default unavailable response remains byte-shape compatible. Stop does not read
fresh state or close the SDK; the caller must observe its own bridge's exit.
Link metadata remains link metadata, never readiness or permission.

The existing `Serve`/main still select no source, and the normal module has no
SDK dependency. No new start command, launch/credential wire, production provider
or root permission is introduced. Attaching a source in the real host remains
conditional on reviewed SDK/Root/identity/egress and an authorized caller.

## Reproduce

Use Go1.27.3, Node, Python3 and the fixed module caches. Reuse the source stage
produced by the merged WebSocket security evaluation, or regenerate it using
that tool. This runner independently verifies all 31 SDK/NCP files against the
two pinned patches, all 51 Gorilla files against the official ZIP and backport,
the exact module templates and inherited fixture sources. It rejects links
before source traversal. No cached executable is trusted or copied.

```sh
export GOMODCACHE=/workspace/toolchains/go-mod-cache
export GOCACHE=/workspace/toolchains/go-build-cache
export GOPATH=/workspace/toolchains/go-path
mkdir -p work/sdk-status
python3 tools/nkn-build/sdk-status/evaluate.py             # plan only
python3 tools/nkn-build/sdk-status/evaluate.py --run \
  --verified-sdk-stage "$PWD/work/a13-security/run-final/backport-stage" \
  --output "$PWD/work/sdk-status/run-1"
```

Use a NEW output directory. Runtime builds/tests use GOPROXY/GOSUMDB=off and
mod=readonly. Explicit-version cache lookup also has downloads disabled.

Four new integration tests execute the actual SDK on its owned loopback fixture
and the actual adapter/bridge. They cover no added IO, SDK close versus process
evidence, cancellation, replay/cross-bridge context, retirement preserving the
shared client's explicit RPC, framing and privacy. Three new bridge tests cover
provider rejection, redaction, malformed inputs, cancellation and stop semantics;
the seven previous protocol/bridge tests remain. The inherited SDK helper is
reused with its original source identity, not counted as a new implementation.

Actual open/closed samples are passed through the production JS T5 parser and
text projection. The runner also compiles both ELF architectures and runs the
native test artifact. These are test probes, not a complete SDK engine release,
independent-cache reproducibility evidence, an ARM runtime test or deployment.
Every child is bounded and only its owned process group is killed on timeout.
Inputs/Git HEAD must remain unchanged during the run.

All identities, RPC height and messages are synthetic; positive connections
are only to the owned loopback fixture, with other dials denied before DNS/socket.
No NKN/chain/transaction/STUN, production device, Root, NAS, cellular, messaging,
signing or installation. New code/tests are AGPL-3.0-only, OpenAI Codex assisted;
original SDK/NCP/Gorilla sources and notices remain outside Git in the verified
work stage. New source code does not change global acceptance or the nine tasks'
real-provider/authorization/integration/production prerequisites.
