# Generic firstboot AP v1

This optional layer consumes the original `tools/setup_ap_package.py` eight destinations
and the unchanged AP material/channel/transaction contract. It is separate from the
old device-bound maintenance bundle. The layer never contains an identity, TLS key,
certificate, setup code, Wi-Fi credential or an Owner. Its output is unsigned and is
not a public installation approval.

Build the four actual, same-source AArch64 programs and a verified format-2 factory
application first. From the committed source checkout:

```sh
python3 tools/firstboot-ap/build.py --core CORE --edge EDGE --ap SETUP_AP \
 --launcher CORE_LAUNCHER --application SIGNED_FACTORY --public EXTERNAL_PUBLIC \
 --output NEW_OUTPUT --qemu QEMU_AARCH64 --sysroot ARM_SYSROOT
```

The separate verification entry requires an externally selected source and key:

```sh
python3 tools/firstboot-ap/verify.py --material NEW_OUTPUT \
 --application SIGNED_FACTORY --public EXTERNAL_PUBLIC --source EXPECTED_SOURCE_SHA256
```

Pass that output explicitly as `--firstboot-ap-artifacts NEW_OUTPUT` to the existing
`tools/compose-system-tree.py` invocation. No option means no AP change. The original
image builder rechecks this layer before producing an internal image. Its existing
flash/release gates remain false.

On a new generic installation only:

1. Original firmware extraction and private-directory/USB identity preparation run.
2. Original `generic-provision.request` and immutable layer allow `generic-install`
   to record `firstboot-ap.request` **before** original identity initialization.
3. Original `install-initialize` generates the Core identity/database/setup code and
   ordinary USB/LAN Edge certificate with the original numeric role sandboxes. The
   original Root-private six-file first Owner handoff is not modified.
4. `rustsimhub-firstboot-ap.service` runs after generic identity installation and
   before app selection. It requires the original complete marker, unchanged source,
   actual program metadata and signature verification, inactive services and the
   expected private files. It invokes the original Core AP binding, setup Edge TLS
   preparation and Root AP material preparation. Generated identity/credentials stay
   on that device; repeated preparation verifies the same material.
5. Only then are the original AP tmpfiles and two activation drop-ins atomically published.
   The tmpfiles are deliberately delayed: early boot must not recursively create
   public parents inside the original private state layout. The
   manager is reloaded before the durable prepared marker is published, so the
   queued Core job sees its original launcher drop-in. A separate fixed activation
   oneshot waits for Core and app selection, then queues only the two original
   consumers with a literal nonblocking systemctl invocation. Its request/prepared
   conditions and Requisite guard prevent old devices or failed preparation from
   queuing AP/NM jobs. Consumer drop-ins also require successful preparation. Ordered inactive boot jobs are allowed; live processes
   are refused. This also avoids relying on a late Wants drop-in to expand a
   transaction PID1 has already queued. The ordinary app target starts Core. The
   consumer cannot activate a network without the original finite live Core lease,
   idle/USB/NM checks and journals. No firstboot helper issues `nmcli`, resets Owner,
   starts a service, activates an APN, deletes data, or loops on a failed network action.
6. Original Core first-setup Ready/claim, confirmation/cancel/revocation, station
   trial/save and rollback semantics remain authoritative. A preparation JSON/status
   is never an installation, Owner or health capability.

Old devices without the new durable request retain their Core mode. Request creation
refuses an existing installation.complete, so an old generic request is not upgraded
into a new enrolment. Existing owned state is never cleared: original AP-binding
preparation preserves Owner/setup.used; original Core decides whether a device window
exists. USB and SSH preparation remain independent; AP service failure does not become
an app/USB dependency. Existing identity or material loss is refused, not regenerated
behind a prepared marker.

Fixed local Root status command: `/usr/local/libexec/rustsimhub-firstboot-ap --inspect`.
It exposes only requested/prepared/unknown and retained-failure presence. First failure
is preserved privately, with error class only; no credential or subprocess body is
logged. Interrupted file publication resumes only from its exact owned pending inode
and expected bytes. Unknown AP transaction state belongs to the original AP journal;
never remove it or reset the budget merely to repeat firstboot.

Validation uses `python3 -m unittest discover -s tools/firstboot-ap -p 'test_*.py'`.
Root ownership tests must run in an owned offline container with CHOWN, FOWNER,
FSETID, DAC_OVERRIDE; `isolated_actual.py` additionally needs private bind-mount
namespaces (SYS_ADMIN, SETUID, SETGID), Python, OpenSSL, setpriv, mount/umount,
Git and the explicit QEMU/sysroot/program/license inputs. Do not grant any access
to host system buses, devices or credentials. `/tmp` needs exec only for synthetic
fixture command stubs. The actual runner accepts `--programs`, `--qemu`,
`--sysroot`, `--license`, `--work` (fresh empty owned directory). It executes
original ARM CLIs, with PID1 status/reload explicitly adapted and no listener or
network activation. An actual boot of this new ordering remains a separate check.
