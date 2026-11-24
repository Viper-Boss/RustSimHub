# B0 inventory — resumed 2026-11-17

Task: `docs/team/OFFLINE-B-INTEGRATION-20261115.md`, including the 2026-11-16
USB-first update. This is the new B execution package, not historical member B.
This inventory does not mark the complete B package delivered or accepted.

Starting main and personal branch base:
`de663743bd726e77f7c560139d0f91ea78e8fe57`, tree
`897b7b485ce8b3108d359d859dd2384efba78df1`.
Branch: `codex/offline-b-20261115-fungi1220` in `Viper-Boss/RustSimHub`.
The previous `codex/usage-stats-clock-fallback-local` checkout was clean and
retained; its separate, unpublished Windows-lock probe remains outside this PR.
The first read on 2026-11-16 saw main `6e1cd19bc9dd05fd150157c422590c1a801cc0b0`;
resume used the later main above before starting B implementation.

## Source and environment

- Connected GitHub account: fungi1220. No other account/author/permission changed.
- Local Git fetch failed DNS; official repository codeload returned 404. No
  credential extraction or private-maintenance access was used. GitHub connector
  reads supplied immutable objects. All 1,797 unique current blobs, the complete
  tree and the signed main commit were checked against Git object hashes.
- 1,315 ancestor commit objects were reconstructed byte-for-byte, with no hash
  mismatch or missing parent. `git rev-list --count` returns 1315. Actual local
  `git merge-base --is-ancestor 2ae840fbffff04691ccf8cedd49d554093dd4032 BASE`
  exits 0. Original shallow boundary was removed only after its parents and the
  whole required ancestor chain were present; the old metadata was retained.
- Existing `tools/product-integration/inputs.json` contains 18 historical inputs.
  The original verifier sees all 18 as `included`, no missing product/report.
  Its `--require-complete` correctly refuses this development worktree as dirty.
  That old list does **not** exhaustively enumerate PR251+ or latest A candidates;
  updating the pinned source coverage remains a separate B1 subpackage.
- At initial inspection, PR263/264 were non-draft A work; 262/259/250/69/56/53/52
  were drafts. No draft, other branch or merge action was performed. The frozen
  base subsequently already contains A work; it is consumed only at this SHA.
- Windows AMD64; Python 3.12.14; existing bundled Node 24.22.1. System Node
  16.20.2 failed modern ESM/structuredClone fixtures and is not the final runtime.
  No new Node downloaded: user later explicitly chose existing tools.
- Rust 1.98 tooling/cache exists from earlier work. No usable WSL, Docker,
  Linux PID1, ARM sysroot, QEMU or fixed managed SDK build stage is available
  in this task environment. No device/NAS/production input was substituted.

## 29 system/release actions

`existing` means real repository implementation, not final acceptance.
`input/Linux` is an execution prerequisite; it is never counted as passed.

| ID | Existing implementation / reusable evidence source | Current offline gap |
|---|---|---|
| UPD-02 | product-install/package.py; update/product_admission; product-release roles | Linux signed synthetic group/Root matrix; no production signing |
| UPD-03 | product-release/release.py; product-integration/verify.py | Refresh source index; same-source ARM profiles need inputs |
| UPD-05 | core/update_health.rs; product-integration/production_chain.py | Actual Linux Core/UID/PID/late health process matrix |
| UPD-06 | product-upgrade harness; update migration/coordinator | Linux SIGKILL/ENOSPC/read-only/partial publication execution |
| UPD-07 | kernel-artifact-audit; rootfs firmware/kernel locks | Matching public boot/DTB/modules/sysroot inputs |
| UPD-08 | offline_delivery/initramfs.py; rootfs tools | Linux artifact matrix; actual boot stays with maintainer |
| UPD-09 | recovery/install-system.py; second-board/plan_gpt.py | Isolated image/partition samples; physical writes excluded |
| UPD-10 | product-integration/matrix/profiles.json; release-audit | Complete source/profile/artifact evidence; no new AP support |
| REC-03 | recovery/restore-system.py and restore_protocol.py | Linux isolated recovery inputs; physical recovery not accepted |
| REC-04 | product-upgrade harness; recovery/offline_business_matrix.rs | Linux data/identity/outbox/Pending failure preservation matrix |
| REC-05 | recovery README/preflight tools | Reproducible Linux run and actual-device prerequisites |
| STAB-01 | performance/measure.py fixed workloads | Run existing bounded workload on qualified Linux |
| STAB-02 | performance/measure.py CPU/RSS/latency | Host sampling still unrun; no board/power inference |
| STAB-04 | bounded performance worker; original long-stability prerequisites | Actual 72-hour combined run unrun |
| STAB-05 | performance/compare.py | Independent equal-function old-system baseline missing |
| STAB-06 | performance comparison/tests | Measured source-bound current/old pair and thresholds missing |
| SEC-01 | security-matrix; release-audit/root_materials.py | Run actual build/unit/source inventory on Linux |
| SEC-02 | real Auth/core tests; web synthetic cross-client/revocation fixtures | 16 selected web fixture groups run; Linux black-box Auth still needed |
| SEC-03 | product-install root matrix; update admission; unit materials | Actual UID/capability/path/PID1/cgroup execution needed |
| SEC-04 | security-inventory/scan.py; release-audit/public.py | Linux synthetic cases and actual publication tree needed |
| SEC-05 | dependencies/inventory.py; managed SDK notices | Actual linked module/build closure absent; lock != linked closure |
| SEC-06 | input-corpus; existing web malformed/race fixtures | Selected source fixtures run; Linux bounded protocol corpus remains |
| SEC-07 | release-audit; above independent failures | No unconditional security verdict; retain all gaps |
| REL-01 | six fixed product profiles; original manifests | Actual unsigned same-source build not produced here |
| REL-02 | second-board read-only GPT planner | Independent public sample then authorized second device |
| REL-03 | second-board/recovery tooling | Isolated full sample then authorized physical recovery |
| REL-04 | product integration/source verifier; new B test-evidence runner | Per-layer index exists; final device sign-off unrun |
| REL-05 | existing release/rootfs/recovery documentation/packagers | Candidate inputs and source-bound manual still need B1/B5 completion |
| REL-06 | original acceptance ledger; inventory and explicit refusals | Global final audit remains maintainer responsibility |

## A business coverage and exclusions

| Module | B-side existing fixtures selected | Still not proven |
|---|---|---|
| FWD | forwarding, forwarding-history | Real webhook/Telegram/SMTP, Linux process/cancellation |
| USB/NET | network, network-policy | Actual USB tool/device trial/confirm/cancel; AP deferred |
| SMS/NAS | daily-sms, sms-management, nas-bridge, event-sync | Real MM/SIM/NAS, Linux data recovery |
| CALL/REMOTE | daily-phone, call-test | Real call/lease/media/TURN/NAT; no native audio touched |
| WEB/LED | indicator, exact app imports and missing-binding negative | Real browser/CSP/IndexedDB/three widths and physical LEDs |
| NKN/STAT | usage-stats, usage-participation, nkn-product | Current A encoding/RPC/Root/SDK full combination, public chain |

The B runner executes existing modules, not mirror implementations. It does not
start a second reporting engine or change `CheckInTransport`, 300-second Root
authority, `coordination.rs`, shared identity, database or protocol contracts.
Hotspot-specific development, BLE/AirSIM/VoBLE/native audio/configurator,
production services, devices, signing/installation and global ledgers are untouched.
Overall acceptance remains 81.5%; delegation/build/test counts do not increase it.
