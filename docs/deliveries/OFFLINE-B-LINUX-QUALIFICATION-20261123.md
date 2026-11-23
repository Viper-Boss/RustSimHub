# OFFLINE-B Linux qualification, 2026-11-23

This records real execution, not installation permission or completion of B.
Starting main remains `de663743bd726e77f7c560139d0f91ea78e8fe57`. The measured
three-PR source is **6746473b4d0cd4a1314327693fd80023a0c33cbd**, tree
`7108da944a5a21edead8c0a9ed1de6b163caf343`:

| Input | Exact HEAD |
| --- | --- |
| PR270 runner | `2bfe257e8ed05c9d363f23ca8125ff1363ef82e6` |
| PR271 source coverage | `b5a455e0cfb46c98d72919d4dc79160984507b44` |
| PR278 standalone install harness | `2df2e81157d13081d68fd86439d1478ef72723bb` |

Its two actual merge-tree results were independently compared to GitHub objects.
No PR was merged and no main or remote combination branch was moved. This report
is appended after those tests; later documentation commits must not relabel the
tested HEAD. The separate PR279 Root evidence below is not part of this merge.
Machine evidence: [actual results](../offline-b/linux-qualification-20261123.json).

## Environment and inputs

The user subsequently permitted necessary tool installation. The task owns a
new Debian 13 COW VM, QEMU 11.1/WHPX, Linux 6.12.117, systemd PID1, two vCPUs,
512 MiB RAM and a private 2 GiB swap file. An independent Windows Job limits the
VM to 3 GiB, 32 processes and two hours. No host filesystem/device/USB or user
browser profile is attached. QEMU restricts general networking; management is
only task-owned SSH over 127.0.0.1:23109 with new synthetic keys, no agent or
user SSH configuration. The install-only Debian proxy was not connected to the
guest during these tests. No production/NAS/real NKN/RPC/Bark/carrier access.

Fixed toolchain inputs, verified against official checksum records:

| Input | SHA256 |
| --- | --- |
| Rust 1.98.0 Linux archive | `ed8ee2df70909c88cbaf87a6cfa3920dac00b537de12a6abe6906641e0f5952f` |
| Go 1.25.7 Linux archive | `12e6d6a191091ae27dc31f6efc630e3a3b8ba409baf3573d955b196fdf086005` |
| Node 22.15.0 Linux archive | `dafe2e8f82cb97de1bd10db9e2ec4c07bbf53389b0799b1e095a918951e78fd4` |
| Python Playwright 1.62.0 manylinux wheel | `ba33bae6a13b3d9d354c751cb618af357d20fe1d57767cbcce52079bbef17ad3` |

Python 3.13.5 is from the signed Debian image/repositories. All four Python
wheels (also pyee 13.0.1, greenlet 3.5.6 and typing_extensions 4.16.0) were checked
against PyPI JSON digests before local `pip --no-index` installation. Their
transfer tar SHA256 is
`bdd3fb687463870c2d63ad4553287f144094e01137d7a728216d3d27d351be0a`.
Cargo uses an external task-owned vendor cache, every crate checked against the
unchanged source locks; no lock upgrade or automatic module download. The
standalone upgrade lock's two additional exact crates were separately verified.

The actual browser is Chrome for Testing **151.0.7922.34**, revision 1234 selected
by that exact Playwright wheel's `install --dry-run chromium`. Its official URL:
`https://storage.googleapis.com/chrome-for-testing-public/151.0.7922.34/linux64/chrome-linux64.zip`.
The 193282658-byte archive matches the official Google storage CRC32C
`kEA1rQ==`; its recorded SHA256 is
`ae8736ac28bc69278551500f219fc749575648263c43ec5990749eff43b9fcf8`.
The SHA256 is a locally recorded digest, **not a publisher signature or an
independently published cryptographic checksum**. Official HTTPS, the pinned
wheel's version/URL, official storage CRC and exact length are the provenance.
Guest SHA256 and every ZIP entry CRC were checked before use; traversal, links,
duplicate names, special files and extraction-size excess are refused.

## Final bounded regression

From the clean source above, `umask 022`, fixed local PATH, external Cargo target
and offline cache, run the original entry under the installed Python venv:

```sh
export CARGO_NET_OFFLINE=true GOTOOLCHAIN=local GOPROXY=off GOSUMDB=off
export CARGO_INCREMENTAL=0 CARGO_PROFILE_TEST_DEBUG=0 CARGO_PROFILE_DEV_DEBUG=0
export WEB3_WEB_CHROMIUM=<task-tools>/chrome-151.0.7922.34/chrome-linux64/chrome
<task-venv>/bin/python tools/offline-b/run.py --group all \
  --budget-seconds 600 --output <new-directory-outside-checkout>
```

**33 groups: 32 passed, 1 not_run, 0 failed; runner report exit 3.** The only
not_run is the separate Windows Edge/Node-Playwright subset, whose prerequisites
are deliberately absent on Linux. Its old Windows evidence remains bound to its
original HEAD; it is not counted as a Linux pass. The first PowerShell wrapper
mapped a nonzero remote native exit to 1. Explicit `exit $LASTEXITCODE` preserves
3; an independent harmless remote `exit 3` verified that wrapper correction.

Both source boundaries are clean, HEAD/tree equal, worktree SHA256
`1e1f0ec2692b0c6565bf36df7967a4fa6506692d88a6cc70cc5fe50e2b86ab2a`, no source
change and every original owned-process cleanup complete. The original source
verifier reports 32 inputs included and exits 0. Source assembly completeness
does not grant build/CI/install/health authority.

The original full Python browser matrix really passes at **320/390/1280 px**:
production homepage/ESM/default CSP, real IndexedDB, synthetic signed Owner
login, permission denial, original network-policy journal/lost acknowledgement
and reload/query paths, read-only/passive statistics, participation, corrupt
Pending, offline response, logout/late reply and missing actual module negative
control. The original script and backend are unchanged. Final browser group
exits 0 in 17.01 s, under its unchanged 60 s deadline. A synthetic count of 42
is not a measured installation count; local acceptance is not chain confirmation.
Fresh contexts only; non-origin page routes abort, and QEMU blocks general
guest egress. The original fixture uses `--no-sandbox` inside this disposable VM;
it does not establish native browser sandbox or user-device acceptance.

Original Linux materials all pass: profiles 18, source-inputs 23, modes 4,
release-audit 18, scanner 18, dependencies 6, kernel input 21, second-board 12,
rootfs 36, recovery tools 7, offline materials 24 and performance tools 11.
There are 17 original web groups including the actual browser, installation
harness 2, upgrade harness 5 and real TCP proxy regression. Internal module
tests overlap; these counts are not summed into project completion. Upgrade's
real clean archive stamp is
`7bcfcd41694721f6cb2be106803429c15e13a461315f987f62aa5e45978040bc`.

## Additional original checks at the same source

Default Go module: `go test -race -count=1 -p=1 -timeout=45s -json ./...` and
`go vet -p=1 ./...`, each exit 0, using actual Go 1.25.7. **55 top-level tests
pass, zero top-level skips/failures**; two command packages have no test files.
This module has no third-party requirements. It is the default, SDK-unavailable
build; managed/SDK/tag combinations, upstream licenses and ARM SDK builds retain
their own prerequisites. Existing actual Ed25519/encoding, bounded pipes,
kernel peer checks, local TLS/RPC negatives and nonce/query gates are exercised,
not a live NKN node or transaction.

The guest systemd transient test unit runs as builder with private network/tmp/
IPC, read-only system/home, no capabilities, read-only public sources/toolchain,
only a task cache writable, NoNewPrivileges, MemoryMax=384M, TasksMax=64,
CPUQuota=100%, RuntimeMaxSec=600 and whole-cgroup cleanup. First startup failed
because systemd-run checks a bound executable before mounting; entering via the
existing `/bin/sh` fixed only that launch precondition. No original test changed.

The unchanged public scanner also ran on exactly four exported public blobs:
`nkn-product.js`, `nkn-status.js`, `usage-stats.js`, `network.js`. Exit 0,
`no_findings_in_scope`, zero findings/issues. Its input/output were in new /tmp
directories, respecting the scanner's home-path refusal. An initial export of
a nonexistent `network-policy.js` failed before scanning and was not called a
pass. This is not a scan of a complete release or a blanket secret-free verdict.

## Bounded Linux resource samples

The original `tools/performance/measure.py`, no implementation changes:
`--run <sms|call|forwarding> --seconds 3 --warmup .5 --interval .1 --output <new-file>`.
Each exits 0, completes its full window, has 30 samples, unchanged clean source
and verified owned-process cleanup. Each has one initial unavailable CPU delta;
no invented zero fills it. Raw samples are in the linked JSON.

| Synthetic profile | Completed iterations | Mean valid CPU % | Observed peak tree RSS, bytes |
| --- | ---: | ---: | ---: |
| sms | 70 | 75.42545911335914 | 73502720 |
| call | 5 | 10.172385038744629 | 73785344 |
| forwarding | 50 | 79.6254981188057 | 76181504 |

The zero-capability, private-network transient unit additionally bounds each
measurement to 256 MiB/48 tasks/one CPU/30 s, exposes only the public source and
new report directory, and is collected on exit. The earlier 5 s SMS run reached
the fixed 100-iteration cap after 4.675 s and correctly returned
`window_incomplete` (not a pass). It is preserved; the separate 3 s windows are
labelled as such. Sampling includes runtime/fixture startup, can miss short-lived
children and is not peak memory accounting for exited processes. No device
idle/power/temperature/72-hour/old-system comparison or approved threshold claim.

## Separate Root regression and retained failures

PR279 is independently based on main, HEAD
`2f2fe2aed726fc21b9221ef8a4e22adf932d2d03`, tree
`feb8012b574db2cc14ff1cf207963c11ed6d5df5`, archive SHA256
`434cf22ce6e4d6df8f4a45e4af1c133df48a285fffe30cdcea33275deb7b6a81`.
It only qualifies an observable ctime precondition in two original tests.
Clean exact-HEAD Rootio 11 normal tests and package 16 pass; the two original
dedicated 1 MiB tmpfs ENOSPC/read-only tests each separately pass. Strict Clippy
for both changed targets and rustfmt pass. Original permission/ctime failures,
same-tick observation limits and the restricted unit properties are detailed in
its separate report. This does not merge PR279 into source 6746473b.

New browser environment failures are preserved. Debian Chromium 156 with the
installed driver first asserted inside the CDP driver then timed out; its source
stayed clean and all descendants were reaped. The matched Chrome then reported
Target crashed. Guest kernel logs identified rejected 512 MiB allocations:
the task's old swap file had not been enabled after reboot. After checking its
owner/mode/size/swap signature, enabling that same file restored 2 GiB swap;
no fstab/boot/host paging policy changed. The original matrix then passed and
the final full run above confirmed it. This does not prove the initial CDP
failure was solely a browser-version problem.

The first matching-browser transfer was 362312 bytes short. Exact length/CRC
verification rejected it before extraction. A precise official HTTP206 range
completed the tail, then the whole official CRC/length and recorded SHA256 were
verified. No TLS/certificate/hash check was bypassed. The limited Debian proxy's
180 s request budget earlier truncated its large Chromium package; a fixed
official package was downloaded separately, matched the signed APT index's
SHA256 and installed with `apt-get --no-download`. That package was not the
final matched-browser result.

All original earlier source-object/umask, install dependency, missing archive
stamp, Windows fixture cleanup and TCP reset failures remain in their reports.
No safety validator, original assertion, source identity or deadline was relaxed.

## Remaining boundaries and handoff

No main/other branch/author changes, private maintenance reads, A/Core/Edge/Root
production edits, global progress changes, production signatures/epochs,
installation, real communication or chain submission. All signing/install/
health/hardware/public-network flags stay false. The install-only proxy has
zero guest requests in this final VM cycle; temporary test units are collected.

Full SDK/sysroot/ARM profiles and actual unsigned product artifacts, complete
health/UID/PID/cgroup/nft product chain, physical recovery/second device/power
loss/72-hour/real network acceptance remain separate inputs and work. Normal
guest PID1 sandbox evidence does not certify actual product unit activation.
Hotspot, BLE/AirSIM/native audio/configurator remain outside this window.

Cloud CI is not represented by these local results. Earlier fixed-HEAD runs had
billing-before-start annotations. PR279 current runs are failure with zero steps;
the connector currently refuses annotation bodies, so its exact reason is not
independently confirmed this round. No reruns, billing/protection changes or
gate deletion. Review/merge remains with the maintainer; **81.5% unchanged**.

The new files contain only public/synthetic evidence, AGPL-3.0-only project test
results and original tool output; no third-party implementation is copied.
Codex performed the stated checks without inventing hours. Rollback removes
these two evidence documents only; no business data is affected.
