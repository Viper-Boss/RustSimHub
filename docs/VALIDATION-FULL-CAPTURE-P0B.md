# 全设备只读捕获与检查副本

2026-10-14，P0 增加 1 点，总体约 57%，完整恢复入口仍未验收。

从同一已固定 SSH 主机身份的 UFI003 只读读取 /dev/mmcblk0，共 3,909,091,328 bytes；mmcblk0boot0/boot1 各 4,194,304 bytes。文件仅在新建 NAS 私有目录保存，0600、目录 0700，不能覆盖已有文件。源端没有写入、冻结、重启或修改 NV/校准。

复制过程计算 SHA256，NAS 独立重新读取全部文件并核对长度/哈希，最终 fsync 文件和目录。实际读取约 691 秒，源是运行中的块存储，不声明离线一致性。
主/备 GPT header 和条目 CRC 相符，分区无重叠，boot 64 MiB；boot SHA256 与 firmware/board.toml 已固定值相同。

独立提取 rootfs/NSHSTATE 检查副本，原始捕获不改写。仅在断网/空能力/非 Root 容器挂载单个副本，不提供主机块设备。
初次只读 fsck 跳过日志并返回 4；日志回放后仍有 free-block/orphan metadata 问题。只对副本进行允许的自动修复，rootfs 与 NSHSTATE 最终强制只读检查均 exit=0。不能把经过修复的派生副本当作原始捕获天然一致。
对派生 rootfs 用只读 debugfs 提取已知核心/媒体到容器 tmpfs，二进制 hash 与实测 Wi-Fi core 和 RTC native 完全一致。修复后原始全盘 hash 仍相同。

私有资料 .private/full-device-live-capture.json、full-device-layout.log、full-device-filesystem-readonly.log、full-device-journal-validation.log、full-device-derived-copy-validation.log、full-device-state-derived-validation.log、full-device-captured-programs.log。全部 raw image、分区副本、GPT 内容和检查日志留 NAS 私有目录，未进入 GitHub。
工具 tools/capture-full-device.py 仅完成 read-only live capture 与回读；它不刷写、不自动修复、不证明恢复。

边界：还需应用数据完整性、真正离线一致备份/恢复入口、引导/硬件恢复实测与安装清单。recovery_verified=false、system_ab_verified=false、release_ready=false 继续保持；禁止用本记录作为立即替换 boot/rootfs 的依据。
