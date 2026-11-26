# UFI001C A/B 固件 130 开机 USB 排查

2026-11-26，整体验收 81.5%。

## 真机和回读

主、备 GPT 和九项镜像刷入后均通过回读，沿用原 LK。普通上电时 Windows 未识别到设备，9008 可重新枚举。通过 9008 核对，主备 GPT、boot 镜像头和两份启动账本均一致。A、B、配置及录音分区的 ext4 超级块与初装镜像一致，没有新的挂载记录。本轮未重启或写入棒子。

## 离线检查

- Android boot 头加载地址与旧通用包相同，两个镜像内的 `Image.gz` 哈希相同。新镜像附带 `thwc,ufi001c` 设备树，`usb@78d9000` 和 eMMC 节点均启用。
- 内核配置启用了 USB ChipIdea UDC、USB gadget、ConfigFS NCM 和 ACM、GPT 及 ext4。`cpio` 可解包初始内存文件系统，`qemu-aarch64` 运行包内 BusyBox 的分区名匹配测试成功。
- `qemu-aarch64` 运行包中的 ARM64 选槽程序，对账本镜像副本执行 `select-session-image` 返回 `A 1 stable`，前后 SHA-256 一致。该测试覆盖账本和选择逻辑，真机 eMMC 与早期引导不在此测试范围内。

## 结论

正常 USB 设备由 `rustsimhub-usb.service` 注册。它在 `rustsimhub-firstboot.service` 之后启动，后者依赖已挂载的 `RSH_CONFIG` 和共享分区。当前迹象指向这条依赖链之前的启动阶段。要区分 LK 交接、内核启动和根分区挂载失败，需要采集开机 UART 日志。系统启动、USB 枚举与双槽切换仍待实机验证。
