# Offline B: original recovery matrix in the bounded entry

## Resumed validation and publication, 2026-11-24

The earlier quota/publication and Windows failure paragraphs below are historical.
The approval route subsequently recovered. ARM evidence is now published as
[PR281](https://github.com/Viper-Boss/RustSimHub/pull/281), exact head
`da3fd37aa98b7459afdb124cf5a8d59decc2d2b1`, mergeable and unmerged. Main was
rechecked at `de663743bd726e77f7c560139d0f91ea78e8fe57`; it was not modified.

The GitHub-connected publication of this runner extension is code commit
`c951783c359a81cfc5334f4600e5e6656b934a0c`. Its complete tree
`3b488e1abba700e8f627f1e9bb984158f20f3eaa` matches local46d725f exactly.
An exact CLI push was attempted without interactive credentials and failed
before writing a ref; no private credentials were sought. The connected tool
created a distinct commit with its own real metadata, not a rewritten local
author or an assertion that the two commit IDs match. The validation branch
`codex/offline-b-recovery-validation-20261124` preserves that tested remote head.
This extension belongs in the existing runner PR270, not another PR duplicating
its complete pending runner diff. No main/other-member branch or merge was used.

Both commits were actually tested in separate clean Linux checkouts, not merely
declared equivalent. The published c951783 run returned0: all 15 original
real-production-library cases passed, zero ignored, source clean and unchanged,
owned descendants completely reaped. Duration182.791s; original full backend
report SHA256 `e947feeed51cd699ce8afd50faaee5b44b4443c268c6ddc9293a3cd36b648b1e`.
The local46d725f run also returned0 with all15 cases, separately preserved.
See [published-source receipt](../offline-b/recovery-entry-20261124/linux-recovery-c951783-summary.json)
and [local-source receipt](../offline-b/recovery-entry-20261124/linux-recovery-46d725f-summary.json).

At exact c951783, original runner self-tests returned0 on both platforms:
Linux32/32 passed with no skips; Windows31/32 passed with its one explicit
Linux archive test skipped. The original TCP proxy test passed in the normal
authorized host environment, without a code/assertion change. The prior sandbox
EACCES failure remains above/below and in its original log. See the actual
[self-test counters and log hashes](../offline-b/recovery-entry-20261124/self-tests-summary.json).
The newly extended complete34-check `all` selection was not rerun here; the
earlier33-check combined-source results retain their original6746473b source.

Independent selected Root health tests at c951783 found a retained failure:
six passed, one failed (`kernel_probe_reads_real_executable_and_refuses_missing_process`,
Invalid at tests.rs:232), Cargo exit101, cleanup complete, clean stable source.
Sixteen later isolated repetitions of that one original case all passed; this
does not erase the first failure or establish its cause. The fixture assumes
the child executable is observable immediately after spawn; an exec readiness
barrier is a separate test-only follow-up under investigation. Production probe
limits and fail-closed errors remain unchanged. See the
[failure receipt](../offline-b/recovery-entry-20261124/root-health-initial-summary.json)
and [bounded repetition results](../offline-b/recovery-entry-20261124/root-health-repro.json).
No Root service or full UID/cgroup/nft chain was started or certified.

The task VM was restarted only after its disk check reported no errors, with
the same outer Job/network restrictions. Test units used ordinary builder UID,
private network/tmp/IPC/devices, zero capabilities, readonly source/tools,
specific task cache/output bindings, 768MiB plus1GiB swap,64 tasks and1 CPU.
Real rustup1.29.3 resolved the already pinned Rust1.98 toolchain offline.
An initial root-owned swap-header read by builder was denied; the bounded
validation was rerun as VM Root without changing file permissions. An initial
invalid systemd property `BindReadWritePaths` was rejected before unit creation;
the supported `BindPaths` was used for those same specific task directories.
Neither failure was treated as an executed successful test.

PR281 CI runs45334768959/45334748234 failed with all four jobs having zero steps.
The connector cannot retrieve their annotation bodies, so the exact cause is
unconfirmed; they are not CI passes. No rerun, billing/protection change or gate
removal was performed. Docker tooling was only planned from four fixed packages
in the existing signed Debian index at this point; no Docker/service was installed.
Full profiles, statistics participation restore, real product health, physical
recovery and public-network acceptance remain unverified. **81.5% unchanged.**

## Original implementation record

This adds an executable Linux `recovery` selection to the existing B runner.
It invokes the unchanged real-library recovery controller, requires all 15
reviewed cases, and retains the original process/source/cleanup failure rules.
It does not implement substitute recovery logic or change business code.

## Source and ownership

- Repository: Viper-Boss/RustSimHub, own local branch
  `codex/offline-b-recovery-runner-20261124`.
- Starting main: `de663743bd726e77f7c560139d0f91ea78e8fe57`.
- This is a follow-up to PR270's exact runner/evidence head
  `dce773d38ac6db39cf8bd01b0a9c5f9e2b234379`, not a replacement main checkout.
- New code commit: `db34ec1929692a93a2129ad09303052aa45d4bba`, tree
  `b19b8a59ec91c678fc1fcfa3013024bb07b6743f`.
- REC-04 / REC-05 / SEC-02: synthetic identity, database and outbox recovery only.
  Other offline B task actions and acceptance weights are not changed.
- Git uses the already configured local author; no identity setting or historical
  authors were changed. No actual hours are invented. Codex assisted implementation
  and the stated checks; maintainer review remains required.

Changes are confined to `tools/offline-b/`, its CI path selection and these B
evidence documents. No new Cargo dependency, production service, Root target,
permission, schema, restore format or mainline progress change.

## Behavior and review points

```sh
python3 tools/offline-b/run.py --group recovery --budget-seconds 900 --output /tmp/offline-b-recovery-new
```

The fixed command is `python tools/test-business-recovery-matrix.py --run`.
Linux, cached locked Cargo inputs, Rust, real rustup/Clippy and rustfmt are
required. The producer builds actual storage/auth/recovery/forwarder Rust
libraries into a fresh temporary target and runs its existing 15 cases; it does
not restore real data. Its source, assertions and timeouts remain unchanged.

The B result reader independently enumerates those 15 case names. It rejects
plans, malformed/multiple JSON documents, duplicate keys, nonfinite numbers,
missing/duplicate/unknown cases, incorrect backend, noninteger counters,
changed no-device/no-send boundaries and additional claims. Process timeout,
nonzero exit, output overflow or unknown cleanup override any JSON completion.
The reviewed producer inventory must match the B inventory in CI; a removed
producer case cannot silently lower required coverage.

The selection participates in `all` (34 checks after this change). The existing
600-second default total budget is not increased; a cold recovery build may
exhaust it. A selected check has a 900-second ceiling within the user's total
1..1800-second budget. Missing prerequisites remain `not_run`/exit3, execution
failures exit2. The entry does not install tools, enable network isolation or
download dependencies. Use an independently isolated test environment.

Existing Ubuntu/Windows runner self-tests discover the seven new result-reader
tests. CI trigger paths now also cover the original recovery controller and its
fixture. This does not claim that CI executes the Linux production matrix.

## Actual validation and retained failures

Host: Windows AMD64, Python3.12.14 and existing bundled Node24.22.1. Temporary
files were placed in this task's own directory outside the source checkout.

| Actual command / scope | Exit / result |
|---|---|
| `python -m unittest discover -s tools/offline-b -p test_recovery_result.py -v` | 0; all seven new tests passed |
| `python -m unittest discover -s tools/offline-b -p 'test_*.py' -v`, task-owned TMP/TEMP | 1; 32 discovered, 30 passed, one failed, one explicit Linux-only skip |
| `python tools/offline-b/run.py --group recovery --budget-seconds 10 --output <fresh-external-directory>`, clean db34ec1 | 3; Linux/cargo/rustup/rustfmt absent, no recovery child started; source clean and unchanged |
| `git diff --check` before code commit | 0 |

The full host suite's failing original TCP proxy test reached a real loopback
connect and received sandbox `EACCES 127.0.0.1:<owned-port>`. Its owned Job was
cleaned. No socket error was suppressed, test skipped to make green, or sandbox
route changed. The initial attempt also encountered inaccessible sandbox-default
temporary directories; that failed result was not claimed as a pass. Repeating
with a new task-owned writable temp directory resolved those filesystem errors.
The complete suite is **not passing evidence** in this environment. The retained
second-run log SHA256 is
`dd9e60adb0f517aba92bf2c75b8775da31ad8f7e3a19fbc8867a05df1b39e751`.

The original developer-worktree prerequisite report is retained at
[host-prerequisites.json](../offline-b/recovery-entry-20261124/host-prerequisites.json);
its `dirty=true` and exact working digest are not relabelled as a clean commit.
[host-clean-prerequisites.json](../offline-b/recovery-entry-20261124/host-clean-prerequisites.json)
binds the later actual clean code commit db34ec1; both are `not_run` evidence.

Separately, before this wrapper change, the unchanged original controller and
its four artifact-selection tests executed in the task's disposable Linux VM
on clean combined source `6746473b4d0cd4a1314327693fd80023a0c33cbd`:
four controller tests passed, original real-library matrix returned exit0 with
all 15 cases passed, no ignored cases, `device_operations=0`, `transport_sends=0`.
The observed log remains in the task VM at
`/home/builder/offline-b/runs/business-recovery-20261124.log`.
Its raw bytes have not been exported into this delivery and no digest is invented.
This is prior producer evidence, **not a Linux execution of db34ec1 or this new
wrapper**. The VM used a real linked rustup toolchain, private network/IPC/tmp,
zero capabilities, strict readonly source bindings and bounded task caches;
no fake container marker or replacement clippy executable was used.

## Publication and next execution

Automatic approval review returned its usage-limit error when creating the
preceding ARM delivery branch/PR; those actions were not executed. This recovery
follow-up is saved locally and has not been pushed or opened as a PR. No alternate
publication/access route was attempted. Existing PR280 is the SDK delivery;
there is no claimed ARM PR number. Cloud CI for this new code has not run.
Readonly GitHub rechecks still report main at de663743; the exact ARM remote ref
returns404 and its open-PR query is empty. Its commit object and exact local
branch are preserved, not a published branch/PR.

After the completed guest tests, the task's own VM was stopped using its original
STOP/kill-on-close Job control. The keeper wrote exit125, both known keeper/worker
processes ended, and neither task loopback port23109/23110 remained listening.
This was a bounded Job shutdown, not a graceful guest poweroff or a board-level
power-failure test. The task disk and test logs were preserved. The final cycle's
package proxy recorded zero requests and zero transferred bytes. Restart must
remove only the verified own STOP marker, preserve prior logs and check the own
disk before use; no host boot/service setting was changed.

After approval service availability returns, resume with:

1. Publish the preserved ARM commit
   `da3fd37aa98b7459afdb124cf5a8d59decc2d2b1` on its own branch/PR; compare all
   four file blobs and tree `09a756ab6291097767682a58f5664690717313f5`.
2. Review this follow-up against exact PR270 HEAD. Run the new `recovery` selection
   from a clean committed checkout in the disposable Linux environment, with
   real cached libraries/rustup and private network. Export actual report/log
   digests; do not copy prior 674 results onto a new source label.
3. Publish a narrowly scoped follow-up PR (or coordinate integration with PR270
   after review). Keep maintainer merge authority; do not force-update main.
4. Continue Root health, actual UID/PID1/cgroup/nft negative paths and security/
   release handoff independently of unavailable device inputs. Preserve actual
   container guards; do not fake container/PID1 proof.

The tested recovery scope does not prove statistics Pending or independent
participation restoration, complete-product snapshot consistency, configuration
reconstruction, physical power failure or device recovery. Root health and full
product profiles remain separate. Complete profiles still need all actual
programs, matching approved sysroots and independently approved role input;
generic ARM component builds do not satisfy those requirements.

No device/NAS/private maintenance directories, real messages, business endpoints,
production keys or transactions were accessed. No default boot policy, existing
identity or global progress was changed. Hotspot/BLE/AirSIM/native audio remain
deferred/excluded. Signed/installable/health/hardware/public-network authority
remains false; overall acceptance stays **81.5%**. Rollback removes this wrapper
selection/result reader, its tests and CI path expansion only; no user database
is changed.
