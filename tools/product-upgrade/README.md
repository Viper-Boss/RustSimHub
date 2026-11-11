# Product data admission and original maintenance migrations

> 窗口8续接：本分支已经接入正式Core/updater、同一个原Snapshot的保留API、
> 仅writer交接、实际执行器Root操作证明和有限恢复健康profile。正式固定路径
> 与runner见[生产集成入口](../product-integration/README.md)。下文“由8接线/
> 尚未导出”描述原模块交付时的职责边界；本次实现与实测状态以新增
> [续接报告](../../docs/deliveries/web4-production-wiring-20261104.md)为准，
> 不将历史harness结果当作本轮正式CLI或设备验收。


The historical Window7 harness now reuses the exported updater admission module;
the original guard test is run from the updater LIB unit-test executable, keeping
private health/slot APIs private. Root migration tests use disposable offline
containers and synthetic keys/messages only. Do not run fixed-path fixture code
against a host installation. Production routes and retained-fence integration
are wired as linked above; candidate startup features remain explicit opt-ins.

## API and required shared wiring

- Core lib exports `runtime_compatibility`; `main::run()` handles exactly
  `--product-runtime-compatibility MODE` **before** either activation prefix.
  Call `runtime_compatibility::metadata(mode)`, emit its JSON or finite exit4.
  No path/schema input and no business database open on this branch.
- Updater lib exports `product_admission`. `SignedProductPrograms::inspect`
  first calls the ORIGINAL `SignedPrograms::inspect`; then probes the SAME
  signed Core bytes with the fixed mode option and worker with `--build-info`.
  Signature, payload hashes, committed source, mode, actual compiled features,
  manifest schema bounds and worker protocol must all agree. Root-installed
  trust selection, slot identity, epoch/floor and effective units remain the
  original maintenance action's responsibility, never package/browser inputs.
- `ProductDataAdmission::compare(&snapshot, selected, candidate, fallback)`
  borrows the ORIGINAL `MaintenanceSnapshot`; original DB/writer/Root facts are
  rechecked before and after comparison and when reading its report. There is
  no JSON constructor and the report never authorizes mutation.
- Updater lib needs the shared same-source module (Linux only):
  `#[path = "../../../tools/product-upgrade/recovery_child.rs"] pub mod product_recovery_child;`
  Its normal dependencies need storage with `recovery-grants,recovery-execution`
  plus rustix `net,time` in addition to existing `fs,process,thread`.
  Core can reuse that export or include the identical file under its own
  recovery feature. These workspace/shared Cargo edits belong to Window8.
- The candidate Core handles exactly `--migrate-product-recovery-child` and
  `--inspect-product-recovery-upgrade` before normal startup. The former calls
  `product_recovery_child::child()`; the latter calls `inspect()` and prints its
  finite read-only result. Unsupported feature/builds refuse; no extra path,
  target schema, shell command, public fixture or grant-writing options.
- `coordinator::migrate_email4` calls the ORIGINAL updater email adapter. It
  consumes the original snapshot, uses the original signed child/backup/R-G-D
  handshake and does not create a new sender or outbox engine. Its existing
  base6 constraint means email migration precedes recovery when both apply.
  The ORIGINAL adapter re-derives the actual installed baseline and requires
  baseline/candidate/baseline to match SMTP5/outbox4 contracts; an ordinary
  worker4 installed baseline remains refused, not silently upgraded by JSON.
  It consumes and does not return the snapshot: Window8 must extend the shared
  original API to return/retain the actual DataFence for a continuous combined
  operation. Re-observing after dropping it is not a substitute.
- `prepare_recovery9(snapshot BY VALUE, selected, candidate, fallback,
  request_id)` returns an opaque `RecoveryUpgradePlan` which **owns that very
  original guard continuously**; no replacement snapshot or serialized permit
  is accepted. `migrate_recovery9(plan, journal, &mut handoff)` calls the SAME
  candidate's sealed Core bytes at UID/GID986, empty groups/capabilities and
  NNP, over a direct Root-parent socket. Root revalidates before granting G.
- `RecoveryMigrationHandoff` has `revalidate_before_handoff`,
  `with_business_writer_handoff(snapshot, work)` and
  `revalidate_before_commit`. Window8 must implement it by retaining actual
  stopped-Core/Root/slot/outbox fences while transferring ONLY the old business
  writer to the ORIGINAL Store migrations. The existing snapshot has no safe
  public extraction API yet. Dropping the entire snapshot and observing a new
  one, ignoring Root changes, or constructing a guard from JSON is forbidden.
- Root installation changes invalidate the old complete Root observation.
  Window8 must retain the DataFence and accept only a proved signed installation
  write-set into the Root generation. It must not release business/queue locks
  across installation or silently resample unrelated Root changes.
- `UpgradeJournal::production(UpgradeKind::Email4|Recovery9)` opens separate
  fixed Root-owned0700 directories under the fixed system product-upgrade
  directory. Window8 provisions those exact directories before admission;
  journal locks/state are0600 and single-link, not package writable. Separate
  kinds allow the two explicit migrations without deleting successful records.
  Recovery child has its own data-owned private backup/journal. After interruption
  queries distinguish not_started / committed / unknown (and explicitly
  grants_committed for7). Unknown is never automatically retried or restored.
  A committed Root journal is an acknowledged migration record, not a renewed
  proof of current data or an install/health/floor authorization.

Shared wiring is intentionally unimplemented in this package. In particular,
current Core lacks the new child/metadata CLI; the production coordinator fails
rather than treating harness metadata or a JSON report as success. Window8 must
run signed ARM Core+worker end to end and retain all original maintenance gates.

## Same-source checks

Use Rust1.98 and the fixed tested commit's `git archive HEAD | sha256sum` as
`RUSTSIMHUB_BUILD_SOURCE_SHA256`; preserve a unique target outside the checkout.
Set `CARGO_INCREMENTAL=0 CARGO_PROFILE_DEV_DEBUG=0`. Run the commands below with
`--locked --offline`; repeat features for empty default, `product-providers-candidate`,
`product-providers-candidate,email-production-candidate` and `dev-fixtures`:

```sh
cargo check --manifest-path tools/product-upgrade/harness/Cargo.toml --lib --features FEATURES --locked --offline
cargo test --manifest-path tools/product-upgrade/harness/Cargo.toml --features FEATURES --locked --offline
cargo clippy --manifest-path tools/product-upgrade/harness/Cargo.toml --all-targets --features FEATURES --locked --offline -- -D warnings
```

One explicitly ignored test requires the fixed disposable Root fixture. The
container runner below executes that exact test; default test runs do not fake
Root or silently claim it passed. Signature tests use real Ed25519 verification
with synthetic ARM transport/metadata; Linux probe tests separately execute
sealed native bytes with bounds. Neither substitutes for signed ARM Core CLI.

## Real Root matrices

Prepare an owned temporary input directory (readable0755, not inside a shared
release tree). Copy inputs under these exact names, set binaries0555 and text0444:

1. Build harness `--features dev-fixtures --bin store6-fixture --bin recovery-child`.
   Copy those binaries as `store6-fixture` and `recovery-child`.
2. `cargo test -p rustsimhub-update --lib --no-run --message-format=json
   --locked --offline`. Copy the executable from the compiler-artifact for the
   updater LIB test as `product-admission-tests`. The standalone harness no
   longer recompiles crate-private production health internals. This runs the
   SAME actual original guard test with
   `--ignored --exact`; only the signed metadata transport in that test is synthetic.
3. Copy `firmware/system/overlay/etc/systemd/system/rustsimhub-core.service`
   as `core-base.service`. No host unit is installed or touched.
4. Build original forwarder `--example outbox3_fixture --features dev-fixtures`
   **without smtp**, copy as `outbox3-fixture` before changing features.
5. Build original forwarder `--features smtp-production-candidate`, copy native
   binary as `email-candidate`. Copy UNMODIFIED `tools/test-email4-migration.py`.
6. Use a locally present pinned Python image (runner never pulls) and execute:

```sh
python3 tools/product-upgrade/run-matrices.py --inputs OWNED_SYNTHETIC_INPUT_DIRECTORY
```

The runner pins the local daemon and image, removes only conflicting Docker
selector/TLS variables, preserves other environment/config/proxy settings, mounts
host inputs readonly and uses `--network=none`, NNP and bounded processes.
Recovery runs on a readonly rootfs with new tmpfs fixed paths. The old email driver
requires an owned writable disposable layer for its `/owned` copies. No host NSS,
systemd, production DB, real identity/key, external sending or device is accessed.
Root container capabilities support dropping to986/reading owned fixture bytes;
the maintenance children themselves require and verify empty effective caps.

Recovery16 scenarios include real6→7→9, sealed execution, revoked7→9, reject/EOF,
wrong source/binding, changed key/realm, busy writer, wrong peer, expiry/late G,
unknown/duplicate fields and symlink refusal. Every rejected case preserves
original bytes, every case preserves old protected rows and reaps the child.
The original email9 scenarios verify exact old jobs/Unknown semantics, old-worker
refusal of new outbox4 and child cleanup, using the original driver unchanged.
