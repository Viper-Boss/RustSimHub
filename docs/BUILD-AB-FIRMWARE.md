# 手动构建 RustSimHub A/B 固件

当前实机基线是 **UFI001C 4 GB、整机固件 131、工厂应用 130**。在同一根棒子上已经分别启动并确认两个完整系统槽。仓库也支持 UFI003 和 8 GB 的镜像构建与分区规划，UFI003 本次只到 Fastboot 接收内存测试镜像，8 GB 尚无双槽实机验收。[UFI001C 验证](VALIDATION-UFI001C-USB-BOOT-20261126.md) · [UFI003 排查](VALIDATION-UFI003-BOOT-20261128.md)

## 1. 固定源码和构建材料

在有 Linux 文件系统的 Linux 或 WSL 工作区构建。需要 Root、Debian 归档密钥、mmdebstrap、ARM64 QEMU/binfmt、ARM64 GCC、Rust 交叉工具链、e2fsprogs、cpio、zstd、设备树工具和 Python 3 及 cryptography。根镜像核验需要 loop 设备及独立挂载命名空间。先提交源码，再用同一提交的文件构建。构建空间预留至少可同时容纳基础系统、组合树及两个完整未压缩的根槽。

- 以 [131 固件源码](https://github.com/Viper-Boss/RustSimHub/commit/7081fb89d64a85b97472003853241bcc6f32fba3) 为已验证基线。使用包含本指南的提交时，Rust 源码、原生音频和新签名应用应一并更新。配套工具固定为 [RustFlashKit bd408b6](https://github.com/Viper-Boss/RustFlashKit/commit/bd408b6c5f936d52997420bad6caa0395db72ea6)。
- 从受信任来源准备与 [Debian 基础锁](../firmware/base-system.lock.json) 摘要相符的 64 MiB 启动镜像和模块归档。两个文件分别作为 `BOOT_SOURCE`、`MODULES`。内核、原始制品和对应源码见[上游锁](../firmware/kernel/upstream-source.lock.json)。上游 Release 的原始 `boot.img` 与基础锁引用的本地完整分区镜像有各自的摘要，按相应的锁核对。
- 准备目标棒子读出的主备 GPT。UFI001C 初装需要校验底层引导兼容性，计算布局时使用同一设备经迁移规划后的单槽分区表。实际板型以 PCB 和已核实的设备树证据为准，基带标签只能辅助判断。按实际 eMMC 容量选择 `PROFILE=4` 或 `PROFILE=8`。
- 下列命令在 RustSimHub 仓库根执行。输入路径设为绝对路径。`WORK` 为新建的私有工作目录，`FLASHKIT` 为配套工具的检出目录。分开保管 SSH 维护密钥、工厂应用签名密钥、整机包签名密钥和设备备份。

```sh
umask 077
mkdir -m 700 "$WORK"
sudo sh tools/rootfs/scripts/build-rootfs.sh \
  --lock-file firmware/base-system.lock.json --setup-ap \
  --boot-img "$BOOT_SOURCE" --modules "$MODULES" --out "$WORK/base"

python3 tools/package-source.py
SOURCE_ARCHIVE="$PWD/.local/source.tar.gz"
REV=$(git rev-parse HEAD)
SOURCE_SHA=$(sha256sum "$SOURCE_ARCHIVE" | cut -d ' ' -f 1)
sh tools/wsl-prepare-cross.sh
sh tools/wsl-build-committed.sh "$SOURCE_ARCHIVE" "$REV" "$SOURCE_SHA"
ARTIFACTS="$PWD/.local/wsl-artifacts-$REV"
UPDATER="$ARTIFACTS"
```

本机交叉编译见 [ARM64 构建说明](local-cross-build.md)。用同一份源码归档和原生依赖构建音频工作进程，保留 `build.json`、二进制及版本回读，令 `NATIVE` 指向这套产物。准备 `AUDIO_MODS` 指向内核音频清单要求的 22 个模块。更新器产物目录 `UPDATER` 也要有 ARM64 的 `rustsimhub-update`、`rustsimhub-edge` 和 `build.json`。组合器会核对数据库版本、载荷摘要与工厂应用选择功能。原生环境与内核输入见 [媒体构建](VALIDATION-MEDIA-WORKER-BUILD.md) 和 [内核音频验收](VALIDATION-KERNEL-AUDIO-P6M.md)。

`APP` 指向已经签名的工厂应用载荷目录，`RELEASE_PUB` 指向相应的 `release.public`。载荷须与本次构建的程序、数据库格式和源码归档摘要匹配。自用试验可运行下面的开发签名脚本，令 `APP` 指向 `package1`，把生成的 `release.public` 用于这份全新固件。脚本会为独立测试信任链创建新密钥。已部署设备的维护升级应由原信任链签名。[应用格式和版本](APPLICATION-ROOT-VERSION-CONTRACT.md)

```sh
python3 tools/package-development-application.py \
  --artifacts "$ARTIFACTS" --native "$NATIVE/rustsimhub-audio-alsa" \
  --native-log "$NATIVE_LOG" --output "$PWD/.private/ab-test-signing" \
  --require-media-worker --with-forwarder
```

## 2. 生成共用 boot 和两个系统槽

```sh
aarch64-linux-gnu-gcc -static -Os -s \
  -o "$WORK/ab-select" native/ab-boot-control/select.c
python3 tools/system/build_ab_boot_candidate.py \
  --source-boot "$BOOT_SOURCE" --selector "$WORK/ab-select" \
  --out "$WORK/boot-base"
BOOT_DIR="$WORK/boot-base"
```

这一步生成 UFI003 基础候选，回读启动头、初始内存文件系统和静态 ARM64 选槽程序。构建 UFI001C 时，用同一内核源码准备真实的 `thwc,ufi001c` 设备树文件 `UFI001C_DTB`，起始 USB 角色为 `otg`。再准备 `VOICE_REFERENCE` 指向音频脚本内摘要所绑定的参考 DTB。设备树来自对应板型和内核，直接改 `compatible` 名称不能替代板级节点。

```sh
mkdir "$WORK/boot-ufi001c"
python3 tools/rootfs/kernel/audio/replace-appended-dtb.py \
  --boot "$WORK/boot-base/boot.ab-candidate.img" --dtb "$UFI001C_DTB" \
  --out "$WORK/ufi001c-dtb.img"
test "$(stat -c %s "$WORK/ufi001c-dtb.img")" -le 67108864 &&
  truncate -s 67108864 "$WORK/ufi001c-dtb.img"
python3 tools/system/build_ufi001c_usb_candidate.py \
  "$WORK/ufi001c-dtb.img" "$WORK/ufi001c-usb.img"
python3 tools/system/build_ufi001c_voice_candidate.py \
  "$WORK/ufi001c-usb.img" "$VOICE_REFERENCE" "$WORK/boot-ufi001c/boot.ab-candidate.img"

PYTHONPATH="$PWD/tools/system" python3 - "$WORK" <<'PY'
import hashlib
import json
from pathlib import Path
import subprocess
import sys
from build_ab_boot_candidate import boot_format

work = Path(sys.argv[1])
base = work / "boot-base"
final = work / "boot-ufi001c"
codec = boot_format()
source = codec.parse((base / "boot.ab-candidate.img").read_bytes())
image = (final / "boot.ab-candidate.img").read_bytes()
parts = codec.parse(image)
assert len(image) == 64 * 1024**2 and not any(image[parts["end"]:])
assert parts["fields"][12] == source["fields"][12]
assert codec.split_kernel(parts["kernel"])[0] == codec.split_kernel(source["kernel"])[0]
archive = subprocess.run(["zstd", "-dqc"], input=parts["ramdisk"], check=True, capture_output=True).stdout
selector = subprocess.run(
    ["cpio", "-i", "--to-stdout", "--quiet", "usr/bin/rustsimhub-ab-select"],
    input=archive, check=True, capture_output=True,
).stdout
assert selector == (work / "ab-select").read_bytes()
report = json.loads((base / "AB-BOOT-CANDIDATE.json").read_text())
assert hashlib.sha256(selector).hexdigest() == report["selector_sha256"]
report["boot_sha256"] = hashlib.sha256(image).hexdigest()
(final / "AB-BOOT-CANDIDATE.json").write_text(json.dumps(report, indent=2) + "\n")
PY
BOOT_DIR="$WORK/boot-ufi001c"
```

用最终 boot 的摘要组合根文件系统。两个槽使用同一份组合树，各有独立标签和 UUID。

```sh
sudo python3 tools/compose-system-tree.py \
  --base "$WORK/base" --artifacts "$ARTIFACTS" \
  --signed-application "$APP" --release-public "$RELEASE_PUB" \
  --updater-artifacts "$UPDATER" --native-artifacts "$NATIVE" \
  --audio-modules "$AUDIO_MODS" \
  --ab-boot-candidate "$BOOT_DIR" --ab-selector "$WORK/ab-select" \
  --out "$WORK/composed"
sudo python3 tools/build-system-image.py --source "$WORK/composed" \
  --out "$WORK/slot-a" --ab-slot A --ab-capacity "$PROFILE"
sudo python3 tools/build-system-image.py --source "$WORK/composed" \
  --out "$WORK/slot-b" --ab-slot B --ab-capacity "$PROFILE"
```

## 3. 计算布局并生成共享分区

`GPT_INPUT` 是本次只读核验的主备 GPT 所在目录，包含 `gpt-primary.bin` 和 `gpt-backup.bin`。以下脚本先校验两份 GPT 的 CRC、指针和范围，再输出构建器需要的几何。整个过程只操作文件。共享分区构建器用于初次初始化，已有 A/B 布局的设备走保留数据的槽更新流程。

```sh
PYTHONPATH="$FLASHKIT" python3 - "$GPT_INPUT" "$WORK/layout.json" <<'PY'
import json
from pathlib import Path
import sys
from usbtool import gpt

folder = Path(sys.argv[1])
primary = (folder / "gpt-primary.bin").read_bytes()
backup = (folder / "gpt-backup.bin").read_bytes()
layout = gpt.parse_gpt(primary, backup, gpt.disk_sectors_from_primary(primary))
header = gpt.parse_header(primary[gpt.SECTOR:2 * gpt.SECTOR], 1)
layout.update(first_usable=header["first_usable"], last_usable=header["last_usable"], entry_count=header["count"])
with Path(sys.argv[2]).open("x") as output:
    json.dump(layout, output, indent=2)
PY

# BOARD_NAME 取 UFI001C 或 UFI003。MAINT_PUBLIC 指向操作者确认的 SSH 公钥。
MAINT_SHA=$(sha256sum "$MAINT_PUBLIC" | cut -d ' ' -f 1)
sudo python3 tools/system/build_ab_data_images.py --board "$BOARD_NAME" \
  --layout-readback "$WORK/layout.json" --maintenance-public-key "$MAINT_PUBLIC" \
  --expected-public-sha256 "$MAINT_SHA" --out "$WORK/shared"
sudo python3 tools/system/verify_ab_flash_inputs.py \
  --boot "$BOOT_DIR" --slot-a "$WORK/slot-a" --slot-b "$WORK/slot-b" \
  --shared "$WORK/shared" --profile "$PROFILE" --output "$WORK/verified-inputs.json"
```

| 载荷 | 4 GB | 8 GB |
|---|---:|---:|
| 共用 `boot` | 64 MiB | 64 MiB |
| `RSH_A`、`RSH_B`，每槽 | 608 MiB | 1792 MiB |
| `RSH_RECORDS` | 64 MiB | 256 MiB |
| `RSH_CONFIG` | 64 MiB | 128 MiB |
| `RSH_ABUSE` | 32 MiB | 128 MiB |
| `RSH_KEYS` | 32 MiB | 64 MiB |
| `RSH_AUDIO` | 64 MiB 初始 ext4 | 64 MiB 初始 ext4 |
| `RSH_BOOTCTL` | 双份选择账本 | 双份选择账本 |

录音分区实占由安全可复用空间决定，首启扩展 ext4。整机包中的启动账本由 RustFlashKit 生成，安装器在根槽和数据镜像写入并回读通过后才启用槽。基带、校准数据及其他既有保护分区由 GPT 规划器保留。表中的 8 GB 数值是布局规格。

## 4. 签整机包，验证安装条件

`BOARD_ID` 用小写的 `ufi001c` 或 `ufi003`。从已证实可以启动相同板型、相同 boot 格式的引导链独立回读主 `aboot`，把完整分区摘要作为 `ABOOT_SHA`。`PACK_KEY` 为整机包 Ed25519 签名密钥，和应用签名是两个信任层。自用可通过 RustFlashKit 的 `pack-keygen --key` 生成，并仅在自用工具构建中登记对应公钥，标准发布工具使用它内置的信任列表。

```sh
sudo env PYTHONPATH="$FLASHKIT" python3 -m usbtool ab-pack-create \
  --output "$WORK/RustSimHub-$BOARD_ID-$PROFILE-AB.rshpack" \
  --name "RustSimHub $BOARD_ID A/B" --version "$VERSION" \
  --key "$PACK_KEY" --board "$BOARD_ID" --aboot-sha256 "$ABOOT_SHA" \
  --verified-inputs "$WORK/verified-inputs.json"
sudo chown "$(id -u):$(id -g)" "$WORK/RustSimHub-$BOARD_ID-$PROFILE-AB.rshpack"
PYTHONPATH="$FLASHKIT" python3 -m usbtool pack-import \
  "$WORK/RustSimHub-$BOARD_ID-$PROFILE-AB.rshpack"
sha256sum "$WORK/RustSimHub-$BOARD_ID-$PROFILE-AB.rshpack"
```

初装前用 [RustFlashKit](https://github.com/Viper-Boss/RustFlashKit/tree/bd408b6c5f936d52997420bad6caa0395db72ea6) 核对型号、容量、当前引导和初装计划，确认校准和恢复资料可读。初装会重建旧系统分区。工具需要适配本机的合法 Firehose 加载器，Windows 一键刷机路径使用 WinUSB。实机 UFI001C 原先的旧引导组合无法启动 ARM64，匹配的 OpenStick v3 迁移经过现场双读、区段范围验证和同设备回退资料核对，旧 `aboot` 得以保留。公开 131 包适用于工具确认兼容的引导链。原厂引导不匹配时，先参照[实机迁移记录](VALIDATION-UFI001C-USB-BOOT-20261126.md)核对 `chain-preflight` 和 `chain-migrate`；迁移目标区段按目标 GPT 验证。

## 5. 核对新固件是否真的启动

1. 刷机回读通过后正常冷启动。核对 Linux USB 复合设备、维护串口和 NCM 网卡，记录真正挂载的根槽。主机曾把重新枚举到的设备留在 Windows，使用虚拟机时同时查看主机。
2. 开机后执行 `findmnt -n -o SOURCE /` 和 `lsblk -o NAME,PARTLABEL,FSTYPE,MOUNTPOINTS`，核对当前根槽以及配置、记录、密钥和录音的独立挂载。检查启动账本进入 `stable`，登录设备 HTTPS，查看已确认的工厂应用与 Core、Edge、Media。
3. 保留当前正常槽，验证另一槽能实际挂载，自动健康确认后再次冷启动并核对数据和证书。把源码提交、包摘要、板型、容量、两个槽的结果和回退结果记录在一起。
4. Fastboot 的 `Booting OKAY` 只证明引导器接受载荷。若看不到 Linux USB 或业务 HTTPS，用可用的串口启动日志、双槽 ext4 挂载记录和选槽账本定位阶段。

131 的真机确认覆盖 4 GB UFI001C 的两个槽。公开包仅重新生成了密钥分区初始化镜像，离线验签和九项回读均通过，公开包原样冷启动和 Windows 完整初装还没有实机记录。
