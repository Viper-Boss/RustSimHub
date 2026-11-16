# Offline B regression entry

This runs a finite, reviewed selection of existing production-module fixtures and
material validators. It is not a product builder, installer, signing service or
replacement for `profile-build.json`, `build.json` or the application format.
The test-evidence JSON cannot authorize health, epoch allocation or installation.

Requirements: Python 3.9+, Git, Node 22+. Use an already installed runtime on PATH.
It never installs dependencies, downloads SDKs, pulls containers or starts hardware
services. Go and Cargo module resolution is explicitly offline. The reviewed
fixtures use synthetic inputs; network namespace isolation is **not** established
by this entry. Public business tests remain excluded.

```sh
python3 tools/offline-b/run.py --list
python3 -m unittest discover -s tools/offline-b -p 'test_*.py' -v
python3 tools/offline-b/run.py --group all --budget-seconds 600 --output /tmp/offline-b-new-run
```

`--output` must be a new directory outside the checkout, with an existing parent.
Existing output, source directories, `.private`/`.local` paths and linked/reparse
output paths are refused. Reports include start/end Git HEAD/tree and a digest
of tracked/nonignored working files, including untracked development code.
Changing tested source makes the run fail. Ignored/private trees are not scanned.
For final delivery, run from the exact committed source, then put a compact
public summary in the report; keep raw synthetic logs outside the checkout.

Exit status:

| Code | Meaning |
|---|---|
| 0 | Selected checks executed successfully; no install/device authority |
| 2 | Execution/assertion/cleanup/source-stability failure or invalid input |
| 3 | At least one prerequisite/case was not run; partial results are retained |

Unittest zero-test success is rejected. Skips and Rust ignored cases do not
become full coverage. Node fixtures must reach their existing assertion completion
messages. Nonzero exits, timeouts, output overflow and remaining children fail.
There are no external command manifests, arbitrary argv or environment overrides
in the CLI. The total run budget is 1..1800 seconds, each log is capped at 1 MiB.

Linux reuses `tools/mainline-checks/run.py`'s existing pidfd/subreaper cleanup.
Windows creates a private Job Object, sets kill-on-close, a 32-process limit and
a 1 GiB job-memory ceiling. A minimal bootstrap joins that job **before** loading
test modules, and then waits for the reviewed check. Failure to establish the
job is `not_run`, never an unguarded fallback. Even normal completion checks for
leftover children. Ctrl-C/errors close the parent handle and kill that owned job.
Windows job isolation is not Linux UID/cgroup/nft/PID1 evidence.

Automatic Git maintenance is disabled only in the child test environment, so
fixture runs do not request background Git maintenance. The original integration
test fixture separately waits at most 0.5s for Windows sharing violations; a
persistent sharing violation or any other cleanup error still fails. User config,
authors and production policies are not changed. UTC is a fixture setting, not
a product clock/format change.

Dedicated AP/firstboot-ap work is deferred. Shared network policy tests remain.
BLE, AirSIM, VoBLE, native audio and configurator work are excluded. A passing
Node fixture is not Chrome/IndexedDB/CSP/three-width acceptance. Actual browser
assembly, Linux recovery/fault matrices, ARM/QEMU, full profiles and signed
maintenance inputs each retain their own prerequisites and evidence.

See [initial inventory](../../docs/deliveries/OFFLINE-B-INVENTORY-20261115.md)
and [delivery evidence](../../docs/deliveries/OFFLINE-B-RUNNER-20261117.md).
