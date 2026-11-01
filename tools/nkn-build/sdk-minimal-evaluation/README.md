# A13/A16 WebSocket-only SDK candidate

This continues the merged SDK compatibility evaluation in PR157. It applies a
second, separately hashed **125-line candidate diff** to that exact derivative.
The fixed 31-file NKNGuard source, original licenses and 186-line lifecycle/RPC
patch remain verified by the previous tool. Only `client.go` and `config.go`
change in this additional patch. No SDK source, binary or cache is vendored.

The SDK previously compiled its WebRTC/Pion/DTLS/ICE/STUN implementation even
with `WebRTC=false`. This candidate removes the peer field, implementation
import and WebRTC connection branches. Configuration fields and historical
module manifests remain for provenance. Both explicit `WebRTC=true` and an
enabled inherited default now fail at configuration merge, before client
allocation, reconnect workers or a dial; they do not fall back to WebSocket.
The normal WebSocket route retains the previous cancellation context, callback,
handshake timeout, wallet policy and response limit.

## What is actually checked

- The existing five real SDK lifecycle/RPC cases run with `-race` on both the
  PR157 derivative and this derivative, using the same synthetic owned loopback
  fixtures. They are inherited regressions, not ten new tests.
- Two new tests cover explicit and inherited WebRTC selection, each exercising
  the actual single-client and multiclient constructors, expecting a nil handle,
  explicit error and no callback/worker activity. No WebRTC/STUN probe runs.
- Four original SDK/NCP routing tests retain their upstream authors and pass
  with `-race`; three Python checks verify patch identity, chain tampering and
  rejection of extra targets even when their patch digest is recomputed.
- Actual package metadata for both Linux architectures must contain neither
  `github.com/pion/` nor NKN's `api/webrtc`. The native comparison uses the same
  `CGO_ENABLED=0` package selection before and after; race tests use CGO=1.
- Both architectures build real ELF **test probes**. The native probe executes;
  ARM64 compilation is not ARM execution or an engine release.

The observed compiled module closure shrank from 37 to 16 without a module
upgrade. This excludes 21 modules, including Pion, DTLS, STUN and x/net. It does
not delete historical `go.mod`/`go.sum` entries, certify the complete module
graph, or prove every remaining API safe. OSV still matches three compiled
module versions: websocket, x/crypto and x/sys. Module matches are not function
reachability results; duplicate GO/GHSA aliases are not separate vulnerabilities.

## Reproduce

Use Go1.27.3, Python3, `patch`, a native C compiler for race tests and the exact
module cache prepared for the preceding evaluation. Fetch source only through
that tool's explicit fixed-source option. The following run itself sets
`GOPROXY=off`, `GOSUMDB=off`, `-mod=readonly`; it does not download dependencies,
patch a registry cache or silently change versions.

```sh
python3 tools/nkn-build/sdk-minimal-evaluation/evaluate.py  # plan only
export GOMODCACHE=/workspace/toolchains/go-mod-cache
export GOCACHE=/workspace/toolchains/go-build-cache
export GOPATH=/workspace/toolchains/go-path
mkdir -p work/sdk-minimal
python3 tools/nkn-build/sdk-minimal-evaluation/evaluate.py --run \
  --source-cache "$PWD/work/sdk-source" --output "$PWD/work/sdk-minimal/run-1" \
  --online-advisories
```

Use a new output directory. Optional OSV requests contain only public module
names/versions. Without `--run`, invocation creates nothing and opens no network
or listener. Each child is bounded; timeout kills only its new owned process
group. Source hashes, patch chain, unchanged files, input hashes and Git HEAD
are checked before and after. All runtime network success is confined to the
owned loopback HTTP/WebSocket fixture; other dials are denied before DNS/socket.

This is an isolated extraction/compatibility candidate awaiting review, not
approval of either patch, the 1MiB RPC choice, crypto, deployment or any production
network path. The product stdio frontend still has no SDK backend and reports
unavailable. Root/identity/fixed egress, complete process/cgroup cleanup, secure
application sessions, Core bridges, transaction confirmation and real-device
acceptance remain separate prerequisites. No defaults, Root policy, product
wire, CSP, queue, credentials or global progress change.

New tools/tests are AGPL-3.0-only, OpenAI Codex assisted; original SDK/NCP
authors, licenses and NKNGuard modifications remain identified by the base
manifest. Generated patch stages are separately hashed derivatives for review.
