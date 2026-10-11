# P0 硬件基线

采集日期：2026-10-11。连接 NAS 后通过 USB 网络及既有维护密钥只读采集。
原始日志放在本地 .private/，此处仅保留脱敏结果。

| 项目 | 实测 |
|---|---|
| 平台 | aarch64；thwc,ufi003 / qcom,msm8916 |
| 内核 | 6.12.49-msm8916-g93a71ee9468d |
| 系统 | AirSimHub，Debian 13 (trixie)，DEBIAN_VERSION_FULL=13.7 |
| 物理内存 | MemTotal 388900 KiB，约 380 MiB |
| eMMC | 3.6 GiB，15 个普通分区，另有两个 4 MiB boot 区域 |
| rootfs | mmcblk0p14，约 3.2 GiB，ext4 |
| 状态分区 | mmcblk0p15，129 MiB，挂载 /var/lib/nassimhub |
| boot | mmcblk0p12，64 MiB |
| 核心服务 | ModemManager、NetworkManager、rmtfs、nassimhub-agent 均 active |
| ModemManager | 1.24.0 |
| 音频 | ALSA card 0；CS-Voice 为 device 4，支持 playback/capture |
| 网络 | Wi-Fi connected，USB ethernet connected，gsm disconnected |
| NAS | ARM64，具备 Docker；数据盘剩余约 390 GiB；系统盘仅约 1.1 GiB |

系统分区名仅有一个 boot/rootfs，没有可直接使用的双槽布局。
当前不能声明支持整机 A/B OTA。NAS 编译缓存和备份应放数据盘，避免耗尽系统盘。

本次不更改蜂窝数据、GNSS、蓝牙、通话和旧业务服务。
boot SHA-256：`d5790e07a8ee67c6d80ec24ea9b0cd4b5a78ab4440a3de2b8767fea58a03eaee`。
模块文件清单 SHA-256：`1552358f4e2f63ae0aefc364ade342200269fb673c11c3ad4a4bc3e39f169779`。
清单算法见 tools/device-immutable-hashes.sh：从当前 /lib/modules/<release> 目录开始，
按相对文件名排序逐文件 sha256，再对整份清单 sha256。
boot 64 MiB 已在本机和 NAS 私有保存，均校验通过。
这仅覆盖 boot；完整 eMMC、rootfs/数据一致性备份和恢复入口尚未验收。
内核精确源码提交与补丁集合仍需固定，不能用模块清单哈希替代源码可复现性。
板号 UFI003_MB_V02 来自旧项目照片记录；本次设备树确认的是 UFI003/MSM8916。
