# OFFLINE-B ARM components and QEMU metadata — 2026-11-24

Starting main: `de663743bd726e77f7c560139d0f91ea78e8fe57`. This independent B1
subpackage records actual builds of existing components; it does not change
Core/Edge/Root code, dependencies, features, identities, roles or boot policy.
The complete review HEAD is recorded in the PR body. All 29 B acceptance
actions and the overall 81.5% acceptance retain their existing boundaries.

## Source and tool inputs

Actual build source is **6746473b4d0cd4a1314327693fd80023a0c33cbd**, tree
`7108da944a5a21edead8c0a9ed1de6b163caf343`. It is the verified three-PR B
combination used by PR280, retained at the own validation reference
`codex/offline-b-validated-6746473b-20261124`. A separate fresh checkout was
created for Rust compilation; SDK-generated work is not mixed into it.

Before and after compilation: dirty=false, same HEAD/tree, same full tracked/
nonignored source SHA256
`1e1f0ec2692b0c6565bf36df7967a4fa6506692d88a6cc70cc5fe50e2b86ab2a`.
The build driver computes the real bounded `git archive --format=tar HEAD`
bytes, obtains SHA256
`7bcfcd41694721f6cb2be106803429c15e13a461315f987f62aa5e45978040bc` and injects
that computed value through the existing compile-time source interface. It
does not copy an inherited or fabricated stamp. Original Cargo.lock and all
crate sources remain unchanged; Cargo is locked and offline, using the already
lock-verified task vendor directory.

The user allowed necessary tool installation. Rust 1.98.0 and its actual
aarch64-unknown-linux-gnu standard-library component are used. Official ARM std
archive SHA256:
`a36f7ac98af20ef0ba6368aace0345efabbebaef7962eba99f47190fd256162d`.
Its archive/component/paths were checked before installing into the task VM's
own Rust prefix, with ldconfig disabled. No host Rust profile or system linker
configuration changes.

The actual cross compiler is Debian GCC 14.2.0-19, target
`aarch64-linux-gnu`. Twenty-one packages were selected from the guest's already
signature-verified Debian index. Each fixed package SHA256 and length was
verified on download and again in the guest. The tar transport SHA256 is
`e52a53b77968de1d4748f8f661152bfbcf2ef4df25ad834cc80598c8ba63776c`.
Installation used `apt-get --no-download`; there were zero upgrades, and no
product account/service or boot configuration was changed.
[Exact input records](../offline-b/arm-components-20261124/tool-inputs.json).

The public cross libc is **2.41-11cross1**. It is a reproducible new generic
cross sysroot, **not an independently approved device/audio sysroot**. Actual
runtime symbol requirements are listed below; this evidence cannot establish
that an uninspected device can load these programs.

## Actual three-component build

```sh
export CARGO_NET_OFFLINE=true CARGO_BUILD_JOBS=1 CARGO_INCREMENTAL=0
export CARGO_PROFILE_RELEASE_OPT_LEVEL=z
export CARGO_TARGET_AARCH64_UNKNOWN_LINUX_GNU_LINKER=/usr/bin/aarch64-linux-gnu-gcc
export CC_aarch64_unknown_linux_gnu=/usr/bin/aarch64-linux-gnu-gcc
export AR_aarch64_unknown_linux_gnu=/usr/bin/aarch64-linux-gnu-ar
# RUSTSIMHUB_BUILD_SOURCE_SHA256 is computed from the clean archive above.
cargo build --release --target aarch64-unknown-linux-gnu --locked --offline -j 1 \
  -p rustsimhub-core -p rustsimhub-edge -p rustsimhub-update --message-format=json
```

Actual command exit 0; original Linux pidfd/subreaper backend confirms complete
owned-process cleanup and unchanged source. Default features only, normal
release LTO/strip rules and the original SHA2 optimization remain. No test
fixture or SDK/Root provider feature is silently enabled.

| Actual ARM program | Bytes | ELF machine | SHA256 |
| --- | ---: | ---: | --- |
| rustsimhub-core | 5986600 | 183 | `074a238414fe13581e2f66abb0f99e066894af8bfdec82781ae24f3a93d8409d` |
| rustsimhub-edge | 3412992 | 183 | `901fe2b8945f29cf0d1a47d72d2222238903c2b4c4cce7fdb6cdcab23a0df993` |
| rustsimhub-update | 2444992 | 183 | `350fd27e53abf32b26afcc2ffd31819710a3e306536df00c863eeb44dc8cc41c` |

[Derived public build receipt](../offline-b/arm-components-20261124/build-summary.json)
retains the actual source/command/result/artifact fields and the SHA256 of the
complete original backend report; the full process-identity list and compiler
log remain in the task VM. The public receipt does not pretend to be the raw
backend log or a production release manifest.

The build runs as ordinary builder in the task-owned Debian 13/WHPX VM, with
private network/tmp/IPC/devices, no capabilities, NoNewPrivileges, read-only
source/toolchain/vendor/system, only explicit task cache/target/output binds
writable, 896 MiB cgroup memory + 1 GiB swap limit, 64 tasks, one CPU, 1800 s
backend / 1860 s unit budget. The outer VM remains capped by its own Windows
Job and two-hour lifecycle. No host directory/device, original maintenance
state, user browser or production key is attached.

## Actual QEMU observation

QEMU user emulator: **10.0.13**, Debian `1:10.0.13+ds-0+deb13u1`. The observer
first rereads each own compiler output and checks length/SHA256/ELF, then
creates a new independent single-link 0555 byte snapshot. It reuses the original
bounded program/strict JSON/ELF/GLIBC readers and executes only:

```sh
qemu-aarch64 -L /usr/aarch64-linux-gnu SNAPSHOT --build-info
```

All three actual Rust programs exit 0 and report the computed source archive.
Core/Edge report component identity, aarch64 and development_fixtures=false;
Core/updater report matching database schema 6. Actual readelf symbol-version
sets have maximum GLIBC **2.39 for Core/updater**, **2.34 for Edge**. Requirements
must not be omitted or inferred from the host libc version.

The independently built, source-bound `nkn-sdk-engine` ARM candidate from PR280
also exits 0 under QEMU with its pure `--version`: source HEAD matches, actual
SDK linked, Root launch required, fixture=false. Its Root counterpart is only
rechecked for actual byte length/SHA256/ARM ELF. `nkn-root` has no pure version
entry point, so `--serve-fixed` is never invoked. The SDK builder's earlier
arm_executed=false report remains an accurate report of that earlier phase;
this later QEMU metadata observation is recorded separately.

[Actual metadata observations](../offline-b/arm-components-20261124/metadata-report.json).
The observation unit is ordinary-user/private-network/no-capability/read-only
inputs, 256 MiB/32 tasks/one CPU/120 s, and the original per-command 15 s/output
budgets remain. No Root service, database operation, hardware/NAS/network
provider, signing or install action is run. These are metadata executions,
not full ARM functional tests or health confirmation.

## Retained failures, gaps and handoff

The first offline tool installation failed before install because the two GCC
meta packages contain epoch `4:` but Debian's URI basenames omit it. APT's
`--print-uris --download-only` confirmed the exact missing cache names
`*_4%3a14.2.0-1_amd64.deb`; identical SHA256-verified small files were provided
under those aliases. `--no-download` installation then passed. Index, package
bytes and checks were not relaxed; original failure log remains.

The first build unit failed at NAMESPACE before starting a compiler, because a
broad writable path below hidden home did not exist when mounts were set up.
Precreating new empty outputs and binding only their exact directories fixed
setup. No source write permission or broad home access was added. Both the
failure and the later actual successful compile are retained.

This component set is **not the original complete 12-program + audio manifest**
or a complete legacy/product/email/recovery profile. The original full-set
validator was not weakened or called successful on three programs. Default
Rust features and the separate SDK candidate are not a validated product
service assembly. No AP feature tests, BLE/AirSIM/native audio tests, production
service/Root permission changes, new UID assignment or global progress changes.

Full original release profiles still need the complete same-source program
set, matching ARM/audio sysroots and an independently approved NSS/role input.
No proposal is promoted to approval, no epoch/signature is allocated and no
new install format is invented. All signed/installable/health/hardware/public
network flags stay false. Real devices, second board, power failure, physical
restoration and 72-hour acceptance remain with the maintainer.

No cloud CI pass is inferred; current-head workflow status is read after PR
publication. No reruns or billing/protection edits. The separate SDK evidence
is PR280; original runner/source/install/Root fixture work remains PR270/271/
278/279. Codex built and checked the stated source with the existing project
AGPL-3.0-only tools, without copying external implementation or fabricating
authors/hours. Rollback removes these evidence files only; no business data is
restored or changed. Review/merge remains with the maintainer, **81.5% unchanged**.
