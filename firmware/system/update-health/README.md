# Explicit update-health candidate profile v1

This directory is an opt-in schema6/forwarding profile, owned by WEB3-UPD. It is
not installed in the default overlay. Window4 may consume this contract only
when the Core and Root implementations and the signed baseline are present.
No production key, epoch allocation, deployment or automatic installation is
provided here.

Install the following exact, Root-owned 0644 materials in the candidate Root
image, together with the existing complete forwarding Root material set:

| Source | Destination |
|---|---|
| `55-update-health.conf` | `/etc/systemd/system/rustsimhub-core.service.d/55-update-health.conf` |
| `tmpfiles.conf` | `/etc/tmpfiles.d/rustsimhub-update-health.conf` |
| `rustsimhub-update-health.service` | `/etc/systemd/system/rustsimhub-update-health.service` |
| `rustsimhub-update-health.timer` | `/etc/systemd/system/rustsimhub-update-health.timer` |

The only Core drops are the original `30-forwarding.conf` and the explicit
`55-update-health.conf`. AP/provider/recovery overrides, pending daemon reloads,
foreign fragments and uncovered drops are unsupported by this profile. The
original maintenance compatibility path remains available. This profile does
not migrate schema7/9 or outbox formats.

Root tmpfiles creates safe ancestors and UID986 0700 runtime directory before
Core startup; Core creates and owns the 0600 `core.sock` and lifetime lock in
`/run/rustsimhub/update-health`. Core enables it only with the leading
`--enable-update-health-candidate` prefix. Root's one fixed entry is
`/usr/libexec/rustsimhub/rustsimhub-update confirm-running-candidate`, with no
extra arguments. Nonroot and malformed arguments never enter writable Store
recovery. Source archive metadata must be present and agree with the installed
signed, running Core executable.

Both the running candidate and confirmed fallback must support health v1 and
this retained startup prefix. Their signed build-info and real data must pass
the original Root/data compatibility comparison. Upgrade an old baseline using
the original explicit maintenance path first; do not silently turn on this
profile for an old Core. Desired forwarding OFF is valid even for signed format2
and needs no worker. ON requires Core's admitted generation and the independently
verified live worker. Unknown/orphan/mismatched states are refused.

The timer is deliberately not enabled or attached to the normal target. An
explicit candidate activation may start it once: it fires after 20 seconds,
uses a six-second service budget, has no retry/restart or recurring timer, and
never resets attempted, identities, floor or rollback allowance. The live
observation retains its original absolute five-second deadline; frames remain
bounded to 8192 bytes and 250ms. Failure needs review, not repeated triggering.

Lock order: updater existing shared lock → original Core Auth mutex → existing
forwarding management mutex → original Storage actor/read transaction → upgrade
the same updater flock to exclusive and revalidate every original source.
Another reader may make conversion fail; a failed conversion or any changed
inode/ledger/slot/signature/process/Root/data guard consumes the capability.
There is no detached JSON health token or externally supplied path/PID/hash.

Before the final health gate, prepared confirmation bytes are fsynced to
`confirmation.next` in the separate pre-created Root 0700 persistent
`system/update-health` directory. Store::open never promotes this file. The
final same-connection Core Recheck occurs after preparation, with Auth/controller/
SQL leases still held. Root rechecks all guards and the original deadline, then
publishes the authorized intent into the ORIGINAL `app/state.next` namespace
and performs the existing rename/fsync ledger transition. This is a finite
commit decision, not a promise of future hardware availability.

A pre-intent crash/failure leaves attempted and state.json unchanged; leftover
scratch is refused and retained for explicit maintenance inspection. No automatic
scratch cleanup is provided. A crash/error AFTER intent publication may have
committed and may be recovered by the original Store semantics. In particular,
a missing final Core release acknowledgement is uncertain, not proof that the
ledger stayed unchanged. Query `inspect-state` under the original Root policy;
never rotate identity, rerun choose, delete intent or retry blindly to resolve it.

Native isolation test (after building a recorded-source updater lib unit-test
executable and native CLI) is `tools/test-update-health-native.py --tests <test
executable> --updater <native updater>`. It uses no host mounts or network and
never runs UID tests on the host. The cfg(test) native ELF adapter, synthetic
unit/cgroup membership and signed build-info output are explicitly excluded
from ARM/PID1 acceptance. Production always enforces ARM payloads and executes
sealed, signature-verified metadata bytes. Real Core producer/Auth/Store/Unix
and original ledger confirmation are exercised, not mocked as a whole.
