# Read-only WEB5 release preflight

Run with ordinary Python (assertions enabled), Git, readelf and OpenSSL installed.
No candidate executable, shell, device, system bus, service manager, signing key,
installer, epoch or confirmation action is invoked. Inputs are read only; bounded
private snapshots used by the original archive/ELF/signature helpers are deleted.
Exit 0 means the requested offline checks passed; exit 2 means refused. JSON output
always keeps install/sign/health/confirm/flash authority false.

Component checks deliberately require an explicit scope:

```sh
python3 tools/release-audit/audit.py \
 --repository REPOSITORY --source FULL_COMMITTED_SHA --archive SOURCE_TAR \
 --build-root ACTUAL_PROFILE_BUILD --profile product-recovery-health-v1 \
 --roles-policy INDEPENDENT_APPROVED_ROLES --scope component
```

They read the actual original `profile-build.json`, source archive/Git blobs,
AArch64 programs/readelf versions, exact application-input payloads, source-fixed
Root templates, finite roles/default OFF, licenses and original Go module closure.
They cross-check byte inventories and source markers (BLE host has no embedded
source marker), without claiming compiler/origin attestation or installed NSS.
`release_input_complete` remains false for component scope.

A complete-input preflight is one command, with default scope `release`:

```sh
python3 tools/release-audit/audit.py \
 --repository REPOSITORY --source FULL_COMMITTED_SHA --archive SOURCE_TAR \
 --build-root ACTUAL_PROFILE_BUILD --profile product-recovery-health-v1 \
 --roles-policy INDEPENDENT_APPROVED_ROLES \
 --signed-application SIGNED_FORMAT2_GROUP --release-public EXTERNAL_PUBLIC_32_BYTES \
 --manifest-sha256 EXPECTED_EXACT_APPLICATION_MANIFEST_SHA256 \
 --root-materials SIGNED_PRODUCT_MAINTENANCE_V1 \
 --root-manifest-sha256 EXPECTED_EXACT_ROOT_MANIFEST_SHA256 \
 --system-root EXPLICIT_PUBLIC_DELIVERY_ROOT --system-index EXTERNAL_SYSTEM_INDEX
```

Both exact original signature domains are verified with OpenSSL, using the external
public key. The application/Core/Root/source/profile/role-policy bindings and every
finite Root payload are cross-checked against the audited build; unsigned Root
inputs are refused by complete scope. No production signing happens here.

The explicit audit index is **additional offline inventory**, not a replacement
flash manifest or production authority. Its exact JSON schema:

- `format:1`, `kind:"web5-system-audit-v1"`, `source_commit`,
  `source_archive_sha256`, `profile` match the externally pinned build.
- `artifacts` has exactly `boot`, `modules`, `dtb`, `rootfs`, `firmware_policy`,
  `identity_policy`, `default_off`; each row is `{path,bytes,sha256}` relative to
  the public root. Boot/modules hashes must equal the committed base-system lock.
  Android v0 header and bounded kernel banner expansion, DTB size/magic and ext4
  magic are checked. Kernel execution/modules loading are not tested.
- `staged_files` has exactly the relative paths in `audit.STAGED`, each mapped
  to its SHA256. The actual files live at `staged/<path>` and must match the
  source commit's copied overlay bytes and original 0755/0644 modes. This covers
  firmware extraction/identity initial installation/USB/app-selection hooks;
  it does not prove those files were packed into the supplied ext4 image.
- `firmware_policy` JSON is exactly `{format:1,kind:"extract-own-firmware-v1",
  extract_at_first_boot:true,contains_device_firmware:false,source_commit:SOURCE}`.
  `identity_policy` is `{format:1,kind:"generic-identity-initialization-v1",
  generate_on_device:true,identity_in_image:false,fixed_owner_password:false,
  source_commit:SOURCE}`. `default_off` is `{format:1,forwarding:false,nkn:false,
  ble:false,cellular_data:false,source_commit:SOURCE}`. These declarations are
  checked alongside the source-fixed scripts; they do not attest opaque image contents.
- `public_files` must enumerate **every** regular file in the public delivery root
  except the four opaque boot/modules/DTB/rootfs artifacts. Enumeration is limited
  to this explicitly supplied publication tree. Any private directory name, link,
  extra/omitted file, special node or budget excess refuses before a green result.
- `maintenance_trust` is `{included:false,public_key:null}` for public distribution.
  A deliberate self-use pack may use `{included:true,public_key:RELATIVE_PUBLIC_KEY}`
  **only** with `--distribution self-use`. SSH Ed25519 wire length/type are checked;
  the key is still public trust, not a private key or distribution authorization.

The scanner covers actual component programs/source-fixed materials/licenses and
all explicit nonopaque delivery files. It detects private-key containers, sensitive
filenames and nonempty known credential/SIM/message fields. It never opens a real
user private directory or prints matched contents. A renamed random 32-byte seed,
unknown encoding, compressed image contents and arbitrary personal prose cannot
be proved absent by pattern matching: `public_secrecy_proven` and
`staged_tree_matches_image` remain false. Inspect image contents in a separately
qualified isolated build before release; this preflight does not mount images.

```sh
python3 -m unittest discover -s tools/release-audit -p 'test_*.py' -v
```

Tests generate only owned temporary Git archives, Ed25519 signers, ARM header
fixtures and public inventories. The frozen legacy report fixture is public build
metadata from task8 source847, not a new successful build or installation permit.
