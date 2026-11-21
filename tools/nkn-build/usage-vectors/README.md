# Offline NKN subscription vectors

This is an offline **reference probe**, not a production signer or transport. It
implements A1.1's prerequisite: check actual transaction bytes, signature and
chain digest before connecting the statistics state machine to a new provider.
The Root interface remains a [review proposal](../../../docs/offline-a/statistics-root-interface-v1.md).
No Root operation, permanent permission, service or UI availability is enabled.

`main.go` independently encodes the bounded Subscribe protobuf, the separate
unsigned-chain serialization, Ed25519 signing prehash and signed protobuf. It
compares each result with the fixed NKN constructor/serializer, actual SDK wallet
signer, core signature verifier, marshaler and chain hash. The independent side
uses Go standard library primitives, without SDK protobuf/serialization helpers.
The SDK side is the original source verified by the existing source manifest;
it does not include the network compatibility/managed patches because this probe
constructs no network client. Managed production integration remains separate.

The 15 committed vectors cover all three product topics and nonce 0, 1, 128,
2^32 and 2^64-1, with public synthetic entropy `000102...1f` and the existing
statistics identity domain. Maximum u64 is an encoding boundary, not a claim
that the SDK's signed-int convenience RPC accepts it. Identifier is empty,
fee is zero, attributes are explicitly cleared **before signing or hashing**,
and meta is the six-field canonical public intent JSON. Fixed constructors
otherwise generate 32 random attribute bytes. Candidate empty attributes are a
documented policy choice, not an assertion about a transaction accepted by a
live chain. No vector seed is secret; never use it with funds or production.

The signing hash is SHA256(unsigned-chain bytes). The transaction ID is
SHA256(SHA256(unsigned-chain bytes)), with the core's non-reversed hex order.
Neither protobuf bytes nor intent JSON are the unsigned-chain encoding. Each
vector also rejects a changed signature and a changed topic on **freshly decoded
objects**, avoiding upstream's cached verification flag and cached hash.

Four built-in negative modes must exit 1 through a behavioral assertion: intent
JSON hash, single SHA256 transaction hash, big-endian nonce, and leaving the
SDK's random attributes in place. They are test mutations, not fallback modes.
The positive probe guards the default HTTP transport and asserts zero HTTP
calls; the inspected code only constructs accounts/wallets/transactions and
does not start an SDK client. This is not OS network-namespace enforcement and
does not prove the future production adapter has no network in prepare.

## Reproduce with the original locked dependencies

Use ordinary-user Linux/WSL2, exact Go **1.27.3 linux/amd64**, the existing
`sdk-evaluation/source-manifest.json`-verified source cache and the original
`sdk-evaluation/go.mod`/`go.sum` module cache. Dependency preparation is explicit
and separate; the runner downloads nothing, uses `GOPROXY=off`, `GOSUMDB=off`,
`GOTOOLCHAIN=local`, disables ambient Go/proxy settings, and verifies modules
against the unchanged lock. No new module or dependency version is introduced.

```sh
python3 tools/nkn-build/usage-vectors/run.py \
  --go /absolute/go1.27.3/go/bin/go \
  --source-cache /absolute/verified-sdk-cache \
  --module-cache /absolute/locked-go-module-cache \
  --build-cache /absolute/owned-go-build-cache \
  --work-root /absolute/owned-linux-native-work
```

Each invocation creates a new native stage, preserves the original lock, verifies
31 SDK/NCP files with SHA256/Git blob SHA1, checks gofmt, runs `go mod verify`,
builds the probe, compares regenerated output byte-for-byte with `vectors.json`,
executes four negative controls, and runs `go vet -mod=readonly .`. Every child
has a 240-second deadline and a private process group. Results and bounded logs
remain in the printed run directory. Source hashes identify the exact input;
the delivery report separately binds the Git tree/commit. Existing unrelated
SDK network/race/Root tests are not claimed by this probe.

## Source and licenses

- SDK/NCP: `Viper-Boss/nknguard@c32952d7586676fec10f71a30b22969cb24881f3`,
  existing [source manifest](../sdk-evaluation/source-manifest.json), including
  original Apache-2.0 license files and nknguard fork notice. The runner copies
  and verifies them unchanged into its temporary stage; no vendored SDK is added.
- NKN core: `github.com/nknorg/nkn/v2@v2.2.1-0.20240509224846-24ade56074a3`,
  original go.sum `h1:HES8PokvbjW7xjcJK094C5nLfTe01WZpi48zLasM09o=`,
  Apache-2.0, top-level LICENSE SHA256
  `20707bee5430e2f5db0784438c1d5f697f3f91ec8b07eefc84a74ddb326a2353`.
  Consulted `transaction/{txBuilder,transaction}.go`, `pb/{transaction.go,transaction.proto}`,
  `signature/signature.go`, `program/program.go` and `common/serialization/serialize.go`.
- SDK consulted `wallet.go:SignTransaction` and `rpc.go:SubscribeContext`.
  The latter submits and is deliberately not called by the probe.
- The independent bounded encoder and runner are newly written for this repo,
  AGPL-3.0-only. Go handles Ed25519/SHA256; no custom curve implementation.

These vectors prove a fixed-source offline encoding comparison. They do not
prove public-node submission, chain confirmation, Owner/Root authorization,
durable Pending transport integration, ARM execution, device installation or
completion of the full A package. B can consume this committed source and run
the command independently without executing uploaded binaries.
