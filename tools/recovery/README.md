# Fixed first-board EDL capture helper

This is an operator tool on the USB host, not a device network API or firmware flasher. `edl_readonly.py` exposes only a dependency check, fixed primary/backup GPT capture, a fixed-size eMMC user-area capture, and an explicit normal reset. There is no arbitrary EDL command, erase, program, patch, partition selection, loader selection, or XML input.

The separately supplied runtime is [B. Kerler's edl](https://github.com/bkerler/edl/tree/1cda1a616748d608667b2d13b73a28d3197ceb45), revision `1cda1a616748d608667b2d13b73a28d3197ceb45`, under its original GPLv3 license. Keep that project's license and copyright notices with its files. This repository contains its own wrapper; it does not bundle the upstream runtime, the Qualcomm programmer, or private device images. Supply the same previously used UFI003 programmer from the operator's private recovery materials. Pin both archive and loader by the SHA256 constants in the wrapper; verify extracted files against the archive, permitting only the upstream's three exact internal symbolic links.

Prepare the Python dependencies in an isolated virtual environment. The tested host uses Python3.11.2, pyusb1.3.1, pyserial3.5, docopt0.6.2, pycryptodome/pycryptodomex3.23.0, lxml6.0.2 and colorama0.4.6. Preserve `pip freeze` and the install log privately. Run `--action runtime-check` first; this checks source integrity, imports and the libusb backend without enumerating or opening USB. It does not prove Sahara, loader compatibility or recovery.

For a USB operation the local Root operator supplies a Root0700 tool directory and a new empty Root0700 output directory. Record the physical USB port while the known stick runs normally. `--usb-port` must match the only 05c6:9008 device; bus and address are rechecked and fixed for upstream enumeration. On the wire, configuration keeps SkipWrite enabled, only eMMC/512-byte user-area reads within the observed 3,909,091,328-byte disk are accepted, and mutating/arbitrary Firehose methods are disabled. The previously used programmer executes in RAM; source/command limits do not replace verifying that binary's provenance.

Both capture actions require `--expected-gpt-sha256`: SHA256 of the first34 and last33 512-byte sectors concatenated, from the independently verified private baseline. Validate both GPT header CRCs, both identical entry arrays, partition bounds/non-overlap, and the exact boot/rootfs/NSHSTATE indices and lengths. The observed GPT has sixteen 128-byte entries while reserving the usual 32-sector array area; validate the declared entry bytes, not the unused padding. An initial 128-entry assumption was rejected during baseline inspection, corrected against the actual headers, and the corrected primary/backup check passed. A full read must also match the current boot digest and both preliminary GPT reads. Failed/partial output is retained and cannot be reused. The separate eMMC boot0/boot1 regions are not captured by this helper; their earlier backups remain distinct evidence.

The runtime check passed on the NAS after correcting the wrapper to accept the pinned upstream's internal links. The first refusal and its original wrapper were retained. A subsequent supervised physical-entry exercise passed actual EDL GPT/full user-area reads, one normal reset and separately verified return to the normal system; see [the validation record](../../docs/VALIDATION-EDL-READ-P0B.md). The raw capture required journal/metadata recovery; preserve it unchanged and distinguish it from the independently repaired, filesystem-checked derivative. The running kernel's actual device tree advertises only bootloader/recovery modes; no `reboot edl` command has been attempted. Physical entry must be agreed with the operator, after a normal shutdown request and with a prepared return path. A shutdown request or power loss does not by itself make a clean offline backup.

Invocation shape, with private operator-supplied paths:

```sh
python3 edl_readonly.py --tools /absolute/private/tools \
  --output /absolute/private/new-output --action runtime-check
# Only once physical entry and the trusted GPT digest are ready:
python3 edl_readonly.py --tools /absolute/private/tools \
  --output /absolute/private/new-gpt --action gpt \
  --usb-port BUS-PORT --expected-gpt-sha256 PRIVATE_DIGEST
```

Every hardware action is bounded. Read failure leaves the stick in its observed mode for inspection; it does not automatically restart or redial anything. A reset acknowledgement alone does not prove return to the normal system: check boot hash, identity continuity, services, PCM and non-cellular network state afterward. This helper does not establish `recovery_verified`, authorize raw writes, or provide a released recovery workflow.

`verify-offline-backup.py` checks the frozen regular image on the backup host as Root. Supply independently trusted full-image and concatenated-GPT digests and a new output directory under a Root0700 parent. The input must be a canonical, single-link Root0600 file of the fixed disk length. The verifier pins a read-only file descriptor, checks the complete digest, both GPT copies and the current boot digest, then maps only rootfs and NSHSTATE using read-only bounded loops. It verifies each mapping before running `e2fsck -f -n`, detaches on failure, checks both filesystems even if the first is dirty, and checks the original digest and metadata again. It never mounts or repairs the image and never opens USB. Failed reports/logs are retained and their directory cannot be reused. Keep all inputs, digests and reports private. A successful check proves backup consistency; it does not prove restoration.

```sh
python3 tools/recovery/verify-offline-backup.py \
  --image /absolute/private/capture/mmcblk0.img \
  --output /absolute/private/new-offline-check \
  --expected-sha256 PRIVATE_CAPTURE_DIGEST \
  --expected-gpt-sha256 PRIVATE_TRUSTED_GPT_DIGEST
```

`plan-system-restore.py` creates a reviewable private JSON plan from that exact clean raw backup, its capture report, its independent check report, and the previously trusted GPT digest. It verifies the pinned capture runtime, report ownership/permissions and both filesystem ranges, re-reads the complete backup twice while hashing each range, and rejects changed metadata, paths, digests, boot content or incomplete evidence before creating output. Only partition14/rootfs and partition15/NSHSTATE appear as write targets. Their complement covers every other byte, including GPT, boot, bootloader, modem/NV/calibration and gaps. It does not extract payloads, open USB, change any device, authorize execution or establish recovery.

Protected-range hashes in the plan describe the **backup snapshot**. A future restore executor must read the device's current protected ranges before writing and verify those exact current bytes afterward. Do not demand that current modem/NV bytes equal a historical snapshot, and never write that snapshot into those regions. Both system partitions need matching restore data: reverting only rootfs can leave legacy state ownership or a new system identity inconsistent. Plans are single new Root0700 directories with Root0600 JSON files; preserve all existing plans. Their private source path and hashes must not be committed.

```sh
python3 tools/recovery/plan-system-restore.py \
  --image /absolute/private/capture/mmcblk0.img \
  --capture-report /absolute/private/capture/report.json \
  --offline-report /absolute/private/offline-check/report.json \
  --expected-gpt-sha256 PRIVATE_TRUSTED_GPT_DIGEST \
  --output /absolute/private/new-restore-plan
```

`restore_protocol.py` is a transport policy building block, with no executable hardware entry point. It requires a re-derived fixed layout, full preflight proof and a frozen source digest before permitting one exact write configuration. Each of the two program commands needs an explicit raw-mode ACK before any payload; exact byte counts and payload digests plus explicit final control-mode ACKs are required. Only successful target readback and unchanged **current** protected-range digests permit one reset. Repeated, out-of-order, malformed or unknown commands permanently fail the session. The original read-only helper and its on-wire policy remain unchanged.

The bulk helper checks the exact returned USB byte count and never retries a short/unknown write, including zero-length packet failures. ACK reads use a bounded direct bulk endpoint and parse explicit XML responses, reject ambiguous/multiple/nested/entity responses and never infer ACK from a truthy result or log text. Inspection of the separately pinned upstream source showed why these checks are necessary: its program helper can return true without a positive initial response, its USB write loop advances by the requested chunk size rather than the reported count, and its read-timeout escalation is unsuitable for a separately bounded restore operation. The upstream code is not modified or bundled by this module.

`restore-system.py` supplies the separate fixed executor. Its default `validate` action verifies the exact selected plan digest, original planner/capture sources, private evidence, pinned backup FD, complete image and all range digests, and pinned runtime/loader files. It does not discover or open USB, consume an attempt, reset a device or write a partition. Supply the five recovery Python modules together; retain the independently pinned upstream tools separately.

```sh
python3 tools/recovery/restore-system.py --action validate \
  --tools /absolute/private/pinned-runtime \
  --plan /absolute/private/restore-plan/plan.json \
  --plan-sha256 PRIVATE_EXACT_PLAN_FILE_DIGEST \
  --capture-report /absolute/private/capture/report.json \
  --offline-report /absolute/private/offline-check/report.json \
  --output /absolute/private/new-validation
```

The explicit `inspect` and `restore` actions additionally require the recorded physical `--usb-port` and an exclusive Root operator lock. Both first capture the complete current device into a new private output and independently check its GPT, boot and both filesystems. `inspect` keeps programming disabled and requests one normal reset only after that preflight succeeds. `restore` also requires `--confirm-restore-plan-sha256` equal to the exact selected digest; this local selection does not replace operator review or the hardware recovery gates. It atomically consumes a durable plan attempt before enabling writes, retains the preimage, and programs only rootfs then NSHSTATE through the strict endpoint policy. A consumed or uncertain attempt cannot be retried automatically. Restoring state reverts it to the selected backup snapshot; current protected bytes are retained rather than replaced with old modem/NV content.

After programming, full disk readback, exact target hashes, unchanged **current** protected hashes, independent filesystem checks and unchanged source are all required before one normal reset. Failures preserve private captures, report and attempt marker, release USB without resetting, and require inspection. Even an acknowledged reset does not establish normal-system return, recovery, cold boot or release readiness. **Do not connect this executor to the existing read-only watcher**, whose reset-after-capture-failure behavior is inappropriate after uncertain programming.

Executor tests use synthetic disk data and endpoint fixtures, with filesystem verification stubbed for those fixtures. They exercise normal and failed transactions but do not prove compatibility with the real programmer or a successful hardware restore. Actual hardware inspection, writes/readback and separately verified normal return remain pending. Neither a generated plan, local validation nor these tests authorize a hardware write or satisfy recovery/boot/release gates.

The first real executor inspection captured the complete current disk and passed both independent filesystem checks, but its final strict reset received NAK. That failed report is retained; a separately recorded use of the original read-only return tool restored the old system with identity/boot/services preserved. Control serialization now matches the pinned upstream XML declaration and compact empty elements, and private reports retain each explicit control response. Isolated tests pass; the changed envelope still needs a short hardware check before any write exercise. Do not treat the original inspection as an end-to-end pass.

The short hardware check subsequently passed fixed GPT/boot reads, one strict reset ACK and independently verified normal return. This covers the control envelope; it does not cover write configuration, raw programming or post-write readback. Those gates remain pending.

The planner also recognizes the **verified preimage** from the specifically pinned 9947a0f read-only inspector whose final reset was refused. Supply its original failed inspection report as `--capture-report`, the exact `current-before.img`, and its passed independent `current-before-check/report.json`. It checks the exact producer digest, no possible partition writes, the precise completed-preflight/reset-refusal state, complete range evidence, fixed GPT/boot and independent clean filesystems. It rehashes the complete image/ranges and preserves `capture_action=inspect` with `capture_execution_passed=false` in the new plan. No report is rewritten or relabeled as passed; unrelated failures, writers, unknown producers or partial checks cannot use this path. This permits a plan for the newer frozen snapshot rather than reverting blindly to an older backup. Existing plans and their original frozen planner bundles must be retained, since planner source digests are part of provenance.

`watch-edl-capture.sh` is the separate operator coordinator for a supervised physical-entry exercise. Install an integrity-checked copy into a new Root0700 output directory, use the Root0700 pinned tool directory and Root0600 expected-device file, and verify the waiting process before shutting down the idle normal system. The coordinator waits up to ten minutes for 05c6:9008 on the recorded port, then runs the fixed GPT check and full user-area read. It explicitly requests one normal reset afterward, including after a failed capture once EDL was observed. The newly created `reset` directory records that attempt; an unknown result must be inspected rather than retried. All logs, images and phase markers remain private. This optional coordinator does not change the helper's read-only operations or make a partial capture valid. A USB disappearance after shutdown and a later enumeration alone do not establish clean filesystems: independently check the frozen image and verify the returned normal system.
