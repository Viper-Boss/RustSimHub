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
仍需完成 boot/kernel/modules 内容哈希、不可变引导材料归档、可恢复备份及恢复入口验收。
板号 UFI003_MB_V02 来自旧项目照片记录；本次设备树确认的是 UFI003/MSM8916。
