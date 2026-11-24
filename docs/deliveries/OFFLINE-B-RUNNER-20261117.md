# B0/B4 bounded runner — 2026-11-17

Base: `de663743bd726e77f7c560139d0f91ea78e8fe57`.
Implementation/current full HEAD is bound in the PR body; this report does not
invent a self-referential final hash. Scope: `tools/offline-b/`, this report,
initial B inventory, original integration test fixture cleanup and the narrowly selected CI self-tests. No A business source,
BLE/AirSIM/native audio, workspace dependencies, author or global ledger changed.

## Behavior

33 fixed checks reuse original profile, integration, recovery, security,
dependency, performance and web validators. Missing capabilities/cases remain
`not_run`; failure, output/time limits, children remaining or changed source fail.
The CLI exposes groups, fresh output and a finite budget, not arbitrary commands.
Windows owned Job Objects bound 32 processes / 1 GiB; joining is mandatory before
test imports, descendants are killed on close/error. Linux reuses the existing
pidfd/subreaper runner. No online dependencies, installation or services start.

Reports are test evidence only. `signed`, `epoch_assigned`, `installable`,
`health_authorized`, `hardware_tested`, `public_network_tested` are false. Existing
product/application manifests remain the only build/package formats.

## Actual commands and outcomes

Environment: Windows AMD64, Python 3.12.14, existing Node 24.22.1 on PATH;
TEMP/TMP point at this task's owned workspace temporary directory. No new toolchain
download; prior download review was refused, then user chose existing runtimes.

```text
python -m unittest discover -s tools/offline-b -p test_*.py -v
  exit 0; 19 passed, 0 skipped; actual parent/child Job termination and CPU/RSS counters tested.
python tools/offline-b/run.py --output OUTSIDE_CHECKOUT_FRESH_DIR --budget-seconds 300
  report exit 3; 31 groups = 17 passed / 14 not_run / 0 failed.
  Source HEAD/tree and worktree byte digest match at both boundaries.
```

The passing groups are 15 source-verifier/owned-cleanup unit tests and 16 Node fixture programs
(their assertions cover many cases, not counted as individual unit tests).
The profile suite executes 18 cases: 16 pass, 2 Linux cases skipped, therefore its
group is `not_run`. These figures overlap internal runner smoke checks and must
not be summed into a new project acceptance/test total.

The later host-sampler extension in this same bounded-runner package adds two
regressions and four finite workload profiles. Actual Windows CPU, sampled RSS
and fixture wall time were measured for 8 iterations each; all four exited0,
had zero measurement gaps/errors and stable source byte digests. This is host
fixture evidence only, not Core/board speed or a Linux/PID1/72-hour result.
See `docs/offline-b/HOST-MEASUREMENTS-20261117.md`. Published HEAD/clean reruns
are bound in the PR body rather than manufacturing this report's own commit hash.

Raw runs are retained in the task-owned `.task-tools/offline-b-run-20261117-*`
directories outside the checkout. Public reports omit user paths and raw logs.
The implementation's SHA256/worktree digest is in each local run report; final
Git HEAD and published blob equality are checked at submission.

Preserved negative/earlier evidence:

- Initial Windows CRT `execve` bootstrap crashed before the check completed;
  runner recorded failure. Replaced by a waiting subprocess inside the same job,
  and added an actual production-module completion regression.
- System Node 16.20.2 produced real ESM/structuredClone errors; no product code
  changed to hide them. Existing bundled Node24 was used. Node<22 is now a missing
  prerequisite, tested explicitly.
- Original Git fixture temporary-directory cleanup failed with Windows sharing
  errors. Disabling background Git maintenance alone did not fix it: the first
  published HEAD b926af9e755e51b4bc0cdf6cd0821122b7359b0a reproduced one cleanup
  failure (16 passed groups/14 not_run/1 failed, exit2). That failure is retained.
  The original source-test fixture now waits at most 0.5s for WinError32 only;
  all other errors or a persistent lock still fail. Three regressions include an
  actual Windows directory handle released late, a held-error budget and immediate
  permission-error rejection. The source suite now executes all 15. This changes
  only owned fixture cleanup, not the verifier or any product assertion/config.
- Existing Node fixtures have both `PASS:` and prose `passed` completion messages;
  classification now recognizes those real markers and still refuses empty output.
- The dependency suite initially ran 5/6 with a missing Windows symlink privilege.
  Its actual capability probe now records that suite `not_run` before execution.

## Not run / next subpackages

The added B browser subset actually ran existing Windows Edge at 320/390/1280px,
real production ESM/CSP/IndexedDB, permission refusal, pending preservation,
logout/late reply and missing-module negative control. See
`docs/offline-b/BROWSER-20261117.md`; original Python browser matrix remains unrun.
The earlier 31-check result above predates this addition; current 33-check clean
HEAD results are bound in the PR body.

2026-11-18 combination testing caught an unhandled ECONNRESET in B's denying
CONNECT proxy (17 passed/14 not_run/1 failed, exit2). The fix handles expected
refused-client resets, retains unexpected errors as failures and closes owned
sockets. A real 18-request loopback regression exercises TCP RST, verifies 403
for HTTP/CONNECT, and fails nonzero with ECONNRESET if the old missing-handler
behavior is restored. The finite browser group and existing CI process smoke
test now execute it; no A/backend/product assertion or isolation limit changed.

No Linux runtime, PID1/cgroup/nft, root migration/ENOSPC/SIGKILL matrices, ARM/QEMU,
same-source full profile build, SDK stage, signed sample
installation or device/public-network tests ran here. These are independent
requirements, not implied by Node, cross-compilation or a successful parent exit.
Cloud CI has no result until read back at the submitted HEAD.

This PR completes the independent runner/inventory subpackage, not all of B1–B5.
CI at first published HEAD b926af9e755e51b4bc0cdf6cd0821122b7359b0a: original
test/native-audio and both new runner jobs were all blocked before starting by
GitHub payment/spending limits (runs 43931478658 / 43931478554). No rerun/billing or
workflow bypass was attempted. Later HEADs need separate check readback.

Next: refresh source inputs using exact latest A PR heads and actual ancestry;
then execute qualified Linux/build/profile and recovery subpackages with their
real inputs, preserving refused modes and missing inputs. Mainline signing,
epoch/Root approval, backup, install/readback/unique startup/current-Core health,
rollback, physical recovery and final acceptance remain separately authorized.

Project/source reuse is AGPL-3.0-only, including the imported existing Linux
backend. Resource code uses Python/Windows standard APIs; optional browser tests
use existing Playwright Apache-2.0 without downloads or copied external implementation.
Codex implemented and checked this subpackage;
no personal effective hours are fabricated. Overall acceptance remains 81.5%.
