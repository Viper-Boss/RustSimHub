# Managed SDK candidate

This adds a separate `nkn-sdk-engine` executable. The existing default
`nkn-engine` remains offline and SDK-unavailable. No module dependency is added
to its default Go module. Build the candidate only from a new verified stage;
never run it against public NKN during these evaluations.

Activate the pinned Go 1.27.3 toolchain and set the documented workspace module,
build and GOPATH caches. Then:

```sh
python3 tools/nkn-build/sdk-managed/prepare.py \
  --verified-sdk-stage work/a13-security/run-final/backport-stage \
  --output work/managed-sdk-new
python3 tools/nkn-build/sdk-managed/build.py \
  --stage work/managed-sdk-new/stage --output work/managed-sdk-build-new
```

`prepare.py` first independently verifies the complete existing source and
three-patch chain, applies only this new before/after hash-pinned patch, and
copies the current engine source. Existing source stages are never modified.
`build.py` makes eight whole SDK/Root executable builds using four fresh caches
and compares both architectures/programs independently. ARM compilation is not
ARM execution. `notices.py` verifies the actual whole-program license files and
collects notices without treating them as dependency/security approval.

`managed_test.go` exercises the actual SDK parser, including an observed legacy
missing-field panic, short authenticated ciphertext/nonce rejection, explicit
finite dial policy, and disabling SDK-owned reconnect. Legacy mode is retained;
managed host mode never opts into legacy `GODEBUG` settings. Original regression
probes require their existing explicit test-only environment settings.

The production executable has only `--version` and `--root-stdio`. It cannot
accept credentials, hostnames, paths or launch requests on public stdio. Inherited
FD 3 is a single finite credential pipe, FD 4 an actual direct Root parent's
Unix stream. Actual kernel UID/PID, empty capabilities, NoNewPrivs, executable
hash, independent transport key, generation, nonce, credential digest and kernel
BOOTTIME expiry are checked before constructing the SDK. All startup work shares
ten seconds, scope at most 300 seconds. Root EOF/data/expiry, stdio EOF/stop and
signal cancellation close owned IO. Status remains honest about unknown Owner,
process, route, DHT, counters and end-to-end readiness.

`uid_fixture.py` requires a disposable pinned container with `--network=none`.
It runs a real UID 1002 executable against a Root parent and owned loopback node,
including ready/status/stop, descriptor loss, actual process reaping, malformed
and oversized WebSocket input, expiry, and zero-dial preflight rejections.
The separate `nkn_sdk_fixture` build is visibly marked in `--version`, accepts
only loopback and must never enter release assembly. This fixture does not prove
Root cgroup or fixed-interface egress enforcement, public-network operation,
Core authorization, ARM execution, production approval or deployment.

The relay only opens the fixed normal-NKN Core socket, checks the actual Core
UID, accepts a fresh purpose-2 ClientHello, and carries bounded original Core
packets without interpreting authority or changing ciphertext. One peer, one
descriptor, four handshake exchanges, strict request/reply kinds, finite
deduplication, original handshake deadline and no automatic replay. Core still
verifies the actual Noise/Ed25519/membership and normal Auth/media authority.

Root launch/egress provider integration remains a separate required candidate;
application IP validation and SDK flags alone never establish that proof. All
new wire/Root/crypto/dependency choices remain review candidates. No service,
account, secret, route or deployment is installed by these scripts.
