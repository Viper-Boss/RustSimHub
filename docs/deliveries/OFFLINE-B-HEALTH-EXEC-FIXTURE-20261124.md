# Offline B: bounded executable readiness in health test fixtures

This changes only the owned Linux child fixture in `crates/update/src/health/tests.rs`.
Before asking the production probe to inspect a child, the fixture now observes
the expected executable's device, inode and length at `/proc/<own-pid>/exe`.
The wait has a one-second monotonic deadline and refuses an exited child. Existing
Drop kill/wait cleanup and all original production probe rejection assertions
remain. There is no production probe retry, payload-bound change or service change.

Repository Viper-Boss/RustSimHub, own branch
`codex/offline-b-health-exec-barrier-20261124`, starting main
`de663743bd726e77f7c560139d0f91ea78e8fe57`. Actual tested code commit
`f3384b3028879a373658bf27ce2bf14a32711b93`, tree
`0d42b2f69f52de6a53703680daf999c7afefdc08`, clean before/after; the full source
digest is in the machine receipt below. The final documentation commit is recorded
in the PR body separately. Scope: B's test reliability under UPD-05 / SEC-03;
not trusted product health, release authorization or device acceptance.

## Evidence and limits of the diagnosis

Earlier, the unchanged original seven health tests on B source
`c951783c359a81cfc5334f4600e5e6656b934a0c` had six passes and one Invalid error
at the real child probe. Sixteen subsequent isolated repetitions all passed.
That source's production health files are byte-identical to the starting main.
This does not establish the first error's precise cause. The single failure and
its repetitions remain in PR270's recovery evidence; they are not rewritten as
latest-main results or as a confirmed production defect.

The specific fixture assumption addressed here is separately testable: a real
`/bin/sh` child blocks on a private stdin pipe before execing `/bin/sleep`.
Only after the readiness loop has observed a different executable does its
test-only callback release the pipe. The loop must then observe the intended
image before the actual production probe is called once. No sleep duration or
scheduler race is needed to arrange the wrong-image interval.

In a separate negative source clone, only the test fixture's readiness method
was disabled. The new test actually ran and failed with
`fixture returned before the delayed exec`: Cargo101, zero passed/one failed,
zero ignored. This is an explicitly dirty test-only mutation, never a release
candidate or a production implementation. It proves the regression detects
absence of the required barrier; it does not explain away the earlier Invalid.

## Actual exact-source validation

See [validated-report.json](../offline-b/health-exec-fixture-20261124/validated-report.json),
SHA256 `d77537a056bb77fa66d801e19f49abc7b9c448b17a516b9bfa7c41f4a55c1be4`.

| Actual command | Actual result |
|---|---|
| `cargo test --lib -p rustsimhub-update --locked --offline health::tests::probe_child_waits_for_delayed_exec_before_kernel_observation -- --exact --test-threads=1`, separate negative clone | Expected101; real assertion failed, cleanup complete |
| `cargo test --lib -p rustsimhub-update --locked --offline health::tests:: -- --test-threads=1`, clean f3384b3 | 0; all eight named cases passed, zero ignored, cleanup complete |
| `rustfmt --edition 2021 --check crates/update/src/health/tests.rs` | 0 |
| `cargo clippy --lib --tests -p rustsimhub-update --locked --offline -- -D warnings` | 0 |

Negative and positive builds used **different initially absent Cargo target
directories**. Source remained readonly and clean for the positive run. The
controller was B's exact c951783 runner, reusing the original mainline process
backend; both backend source files were compared byte-for-byte with the target
repository. Registry cache inputs were the existing locked offline vendor set,
with no dependency/edition/Cargo manifest changes.

Environment: own disposable Debian13/WHPX VM, Rust1.98, ordinary builder UID;
private network/tmp/IPC/devices, zero capabilities, NoNewPrivileges, strict
readonly source/tools and specific task write bindings. Unit768MiB plus1GiB swap,
64 tasks, one CPU,1000s; each compile/test/check backend retained its own fixed
deadline,1MiB output budget and original pidfd/subreaper cleanup. Core/AirSIM/media
libraries were compiled as existing transitive dependencies; only the listed
health tests executed, with no AP/BLE/native audio functional tests.

## Discarded harness attempt, retained honestly

[discarded-harness-report.json](../offline-b/health-exec-fixture-20261124/discarded-harness-report.json),
SHA256 `6eaba0e03bfbfc7d8c3c73ea7711955a7642b7651216f757e36786db6ac3e47e`,
is deliberately `passed=false`. The initial harness reused one Cargo target
directory across negative and positive clones. Cargo reused the negative test
executable, so that comparison is not accepted as a positive candidate run.
Also the first test change used a let-chain incompatible with the workspace's
Rust2021 edition; Clippy correctly rejected it. Both were fixed without changing
the workspace edition, disabling checks or changing production probe limits.
The final pass above comes from a new clean exact commit and fresh independent
targets, not from relabelling those failures.

## Handoff

This is independent of PR270's runner and PR279's timestamp fixtures; its final
diff is one test file and these B evidence documents against the recorded main.
No devices/NAS/private maintenance directories, real keys, real RPC/chain or
messages were used. Product Root/Core/Edge services, identity, default boot and
global progress remain unchanged. No signing, installation, activation or merge.
The pipeline's capability/health/install/hardware/network approval flags remain
false. Real UID/PID1/cgroup/nft product-chain proof and physical acceptance stay
separate; **81.5% unchanged**.

The newly requested latest-main audit uses its own clean de663743 checkout and
separate fresh targets; these candidate eight-case results do not stand in for
that audit. Review/merge belongs to the maintainer. Rollback removes the fixture
readiness method/call and its new regression, without user-data changes. Original
project code is AGPL-3.0-only; Codex assisted the stated implementation and tests,
without invented hours or author rewrites.
