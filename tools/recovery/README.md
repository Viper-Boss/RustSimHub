# Fixed first-board EDL capture helper

This is an operator tool on the USB host, not a device network API or firmware flasher. `edl_readonly.py` exposes only a dependency check, fixed primary/backup GPT capture, a fixed-size eMMC user-area capture, and an explicit normal reset. There is no arbitrary EDL command, erase, program, patch, partition selection, loader selection, or XML input.

The separately supplied runtime is [B. Kerler's edl](https://github.com/bkerler/edl/tree/1cda1a616748d608667b2d13b73a28d3197ceb45), revision `1cda1a616748d608667b2d13b73a28d3197ceb45`, under its original GPLv3 license. Keep that project's license and copyright notices with its files. This repository contains its own wrapper; it does not bundle the upstream runtime, the Qualcomm programmer, or private device images. Supply the same previously used UFI003 programmer from the operator's private recovery materials. Pin both archive and loader by the SHA256 constants in the wrapper; verify extracted files against the archive, permitting only the upstream's three exact internal symbolic links.

Prepare the Python dependencies in an isolated virtual environment. The tested host uses Python3.11.2, pyusb1.3.1, pyserial3.5, docopt0.6.2, pycryptodome/pycryptodomex3.23.0, lxml6.0.2 and colorama0.4.6. Preserve `pip freeze` and the install log privately. Run `--action runtime-check` first; this checks source integrity, imports and the libusb backend without enumerating or opening USB. It does not prove Sahara, loader compatibility or recovery.

For a USB operation the local Root operator supplies a Root0700 tool directory and a new empty Root0700 output directory. Record the physical USB port while the known stick runs normally. `--usb-port` must match the only 05c6:9008 device; bus and address are rechecked and fixed for upstream enumeration. On the wire, configuration keeps SkipWrite enabled, only eMMC/512-byte user-area reads within the observed 3,909,091,328-byte disk are accepted, and mutating/arbitrary Firehose methods are disabled. The previously used programmer executes in RAM; source/command limits do not replace verifying that binary's provenance.

Both capture actions require `--expected-gpt-sha256`: SHA256 of the first34 and last33 512-byte sectors concatenated, from the independently verified private baseline. Validate both GPT header CRCs, both identical entry arrays, partition bounds/non-overlap, and the exact boot/rootfs/NSHSTATE indices and lengths. The observed GPT has sixteen 128-byte entries while reserving the usual 32-sector array area; validate the declared entry bytes, not the unused padding. An initial 128-entry assumption was rejected during baseline inspection, corrected against the actual headers, and the corrected primary/backup check passed. A full read must also match the current boot digest and both preliminary GPT reads. Failed/partial output is retained and cannot be reused. The separate eMMC boot0/boot1 regions are not captured by this helper; their earlier backups remain distinct evidence.

The runtime check passed on the NAS after correcting the wrapper to accept the pinned upstream's internal links. The first refusal and its original wrapper were retained. No EDL USB operation has yet run. The running kernel's actual device tree advertises only bootloader/recovery modes; no `reboot edl` command has been attempted. Physical entry must be agreed with the operator, after clean shutdown and with a prepared return path. A power loss does not by itself make a clean offline backup.

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
