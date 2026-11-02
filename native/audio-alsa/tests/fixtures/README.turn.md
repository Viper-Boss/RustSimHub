# Private TURN/RTC fixture

The ordinary native tests do not start a TURN server. Explicit Linux x86_64 run:

```sh
source /workspace/toolchains/activate-rustsimhub.sh
CARGO_INCREMENTAL=0 python3 native/audio-alsa/tests/run_private_turn.py --build-fixture
```

Use the configured HTTPS proxy and the normal system CA bundle; BuildKit mounts the bundle for package retrieval. Do not disable certificate verification. The runner checks the managed local Docker socket and preserves registry configuration. It builds the exact native lock offline, copies the actual test executable read-only, runs UID1000 with no capabilities/network outlet, 128M memory/12 tasks/2 CPUs and a bounded private tmpfs, then records the actual image/binary/log hashes and Git HEAD/dirty state under `work/turn-private-final`. It stops only its uniquely named container. Test credentials, certificates and audio are synthetic; no host package/account, device, Core database or external TURN is used.

The public base image and Coturn/OpenSSL/ALSA/Opus runtime versions are fixed in `Dockerfile.turn`. Coturn is an independent test server, not a server deployment recommendation. Its loopback permission, private user database and nonce/lifetime settings exist only in this isolated fixture. All TLS verification uses real trust chains, hostname and expiry validation. `--image` selects an explicitly supplied local fixture; the recorded actual image ID remains part of the evidence.

The manual target compiles the production `turn.rs`, `rtc.rs` and `remote.rs` sources directly, and uses the actual library Codec. Its 23 tests include 6 standard unit tests also run elsewhere; do not count those twice. The SDK originally added 3 feature tests to the original 34. With the opt-in v2 worker, the current ordinary native counts are default34 and feature42 (including reused source-file units); see [the remote worker fixture](README.remote-worker.md). The Core authorization primitive is real, with synthetic trusted Context; it is not an end-to-end normal Core/Worker v2 identity test.

ARM cross/loopback support is in `Dockerfile.arm`, based on the previous locally built pinned fixture. Use a tracked source archive, the matching Rust1.98 ARM standard library and read-only public registry/Git caches, a separate writable build target, and the ARM pkg-config/compiler settings. The test executable must be an AArch64 ELF and run under `qemu-aarch64 -cpu cortex-a53 -L /` inside the private ARM image. Do not call QEMU success a board/hardware or complete production resource acceptance result; report exact compiled HEAD, image, ELF hash and the individual tests actually run.
