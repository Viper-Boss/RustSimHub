# UFI003 4 GB 引导排查 · 2026-11-28

签名整机候选 130 已完整刷写并回读共用 boot 和两个根槽。普通启动没有枚举 Linux USB，随后回读也没有发现根槽挂载。恢复该棒子自己的原厂主 aboot 后，证实可进入 Fastboot。当前尚无该板型可验证的 A/B 启动方案。

| 测试 | 实际观察 |
|---|---|
| 签名 130 的 boot、RSH_A、RSH_B | 完整回读与签名清单一致，普通重启仅记录 9008 离开。两槽 ext4 超级块与初装镜像一致，挂载次数均为 0 |
| 原厂 aboot 核对与恢复 | 主备 aboot 均为 ELF32 ARM。备用镜像前缀与本机刷写前保存的原厂摘要一致。恢复本机原厂主 aboot 后，两次完整回读相同，主备 GPT、备用 aboot 和启动账本保持原样 |
| 原厂 Fastboot | 枚举并报告 MSM8916。能接受 continue，但设备随后又回到 Fastboot |
| Android boot 中在内核尾部附加 FDT | 原签名候选及补齐硬件匹配字段的 gzip、未压缩两种候选均返回 `dtb not found` |
| Android boot 中使用独立 QCDT | 未压缩 ARM64 Image 与独立 legacy QCDT 的组合收到 `Booting OKAY` 后 USB 断开。此后未见 Linux USB 或 HTTPS。只读核对账本和两槽 ext4 超级块，无变化 |
| 固定 USB 外设模式、早期 initramfs 探针 | 内存镜像被 Fastboot 接收，一轮回到了 Fastboot，一轮只观察到断开。仍无 Linux 端点。无法区分 ARM64 交接失败、早期内核故障和更晚的 USB 初始化故障 |

RAM 启动试验没有写入镜像。当前持久恢复入口是本机原厂 aboot，其余底层引导未在这轮排查修改。LED 颜色和 Fastboot 返回均只作为排查线索。确认挂载需检查 ext4 超级块和启动账本，也可用内核串口补充证据。稳定槽启动时账本可能不改写，须结合其他观察。

## 复用这些发现

- 先核对 PCB 印字、板级设备树和实际 GPT。两种板型都是 MSM8916，板上引导组件、DRAM 初始化和 DTB 匹配条件可能不同。实测两板的 SBL1、TZ、HYP 摘要不同。
- Fastboot 接收镜像的可复用输入组合是 Android legacy boot 头、未经压缩的 ARM64 Image，以及独立的 QCDT 表。QCDT v3 的 SoC、板、子类型和 PMIC 匹配字段来自同板、已知可用的原厂参考，表中的 DTB 载荷选对应的 Linux UFI003 设备树。按 Android 页大小对齐内核、ramdisk、second 和 DT 表，更新头中的长度与摘要。这个组合已证实能过 UFI003 原厂 Fastboot 的 DT 查找。内核执行和正常启动仍需另证。
- aboot 的 ELF32 仅说明它的二进制架构。UFI001C 的实测保留了同一 LK，通过匹配的底层链启动 ARM64。因此，UFI003 也应优先验证实际引导链和交接，不能凭 ELF32 判断必须重写 aboot。保留每根设备可回读的原始引导与校准资料，UFI001C 的迁移候选只适用于匹配的 UFI001C。
- 下一步优先用板上 UART 抓开机日志，或用可分别指示内核入口、eMMC 枚举、初始化和切根的最小 RAM 探针取得证据。得到 Linux 端的确认后再推进 UFI003 手动构建候选的实机验收。[UFI001C 引导迁移实证](VALIDATION-UFI001C-USB-BOOT-20261126.md) · [手动构建步骤](BUILD-AB-FIRMWARE.md)
