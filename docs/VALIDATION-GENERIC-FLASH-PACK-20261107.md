# 通用刷机包验证记录（2026-11-07）

**结论：离线和隔离验证通过，没有写设备，也没有在真机上启动。** 用户在 UsbTool 里点击刷机后才能得到真机结果。

## 打包基线

用户 2026-10-29 决定等 PR #62 合并后从 main 打包。核对结果：[PR #62](https://github.com/Viper-Boss/RustSimHub/pull/62) 已关闭、没有直接合并，改动由 PR #148 正式整合进 main（见 #62 的关闭说明和 `docs/VALIDATION-AP-CONSOLIDATION-20261101.md`），条件已满足。

| 组成 | 版本 |
|---|---|
| 应用（factory，epoch 74） | main `55f9783910590b7b6ca4612af2ef9ccbd6c7c305`，复用已有官方构建和已签名的应用包，未重新签名 |
| 系统树组合 | 本分支 `739bc395106fb76d65417da6164957b4ae0d3b2e`（main 加通用首启） |
| status-led、network-agent | 按组合工具要求，从 `739bc395` 重新构建 |
| 基础系统 | `base-system-p6d-build-second`，锁文件摘要与仓库一致 |

## 产物

私有文件 `.private/rustsimhub-generic-ufi003-20261107-first.rshpack`，1,660,356,050 字节，SHA-256 `24906eccee3ad0b7e4fa17f0204d76e8f3904a553f53c5d7c82ea7bf785e2db2`。

| 镜像 | 字节 | 除以 16384 | SHA-256 |
|---|---:|---:|---|
| aboot | 1,048,576 | 64 | `a2aee67729dfb4ecaae1b500f892b28293ea131319d5c90046ae335c690ee719` |
| boot | 67,108,864 | 4096 | `d5790e07a8ee67c6d80ec24ea9b0cd4b5a78ab4440a3de2b8767fea58a03eaee` |
| NSHSTATE | 135,233,536 | 8254 | `e811ff4a2600b043499f8135ff5eaa749b2219838be7302bb8db68592b00f736` |
| rootfs | 1,456,963,584 | 88926 | `7ba84fe83870dd0f492861e64f4870d84afae9f325a1b938a69585e46a542440` |

rootfs 不超过 2845656 扇区（1,456,975,872 字节）。系统树 12394 项，树摘要 `84917701ae8ce710bef9007670e032afca7f54488251e575753c87a95dc50788`。

## 验证结果

| 项目 | 方法 | 结果 |
|---|---|---|
| `pack-create` | UsbTool `dist` 版 CLI（提交 4401cf9），用 `.private/flashpack-signing.key` 签名 | 退出 0 |
| `pack-import` | 同一 CLI 的副本，放在 D 盘临时目录，避免占用 C 盘 | 退出 0，签名和全部摘要通过 |
| 通用审计 | `build-system-image.py` 内置审计、只读挂载后逐项比对内容、权限、属主和 capability | 通过 |
| 程序运行 | `tools/test-base-runtime.py --system-tree`，qemu 下实际运行 | 7 个 ARM64 程序通过 |
| 单元文件 | `systemd-analyze verify`，针对两个新服务和固件服务 | 没有报告这些单元的问题或顺序环 |
| 板级测试 | `sudo python3 -m unittest discover -s tools/system -p 'test_*.py'`（WSL root，umask 077） | 204 项通过，1 项跳过 |
| 新增测试 | `test_firmware_extract`、`test_generic_state`，包含失败路径 | 21 项通过 |
| 提取逻辑 | 真实脚本，对新棒子 modem 和 persist 只读备份的副本运行 | 33 个文件全部与 2026-11-01 已校验的固件包一致；来源镜像前后摘要不变 |
| 哈希校验失败路径 | 翻转真实 modem 副本中一个分段的一个位 | 拒绝，不发布固件，loop 全部释放 |
| 首启演练 | 真实脚本在 NSHSTATE 副本上依次运行：状态守卫、firstboot、ssh-prepare、固件提取、固件加载，再运行一次模拟第二次开机 | 全部通过；第二次开机没有变化；演练后 e2fsck 干净；已用 46 MiB，可用 52 MiB |
| 身份生成 | 真实 ARM 版 Edge 和 Core（qemu），临时目录：`--prepare-local-media`、`--prepare-device-setup`、`verify-package` | 通过；origin 为 `https://rustsimhub-XXXX.local:8443`，附加 origin `https://10.55.0.2:8443`；Core 重复执行幂等；epoch 74 验签通过 |
| 单台设备数据 | NSHSTATE 逐项读回，只有 6 项，全部 root 私有；rootfs 无 device-name、无固件、无 SSH 主机密钥、无状态 | 通过 |

## 演练与真机的差异

- 分区信息和设备路径是替换后的固定路径，物理父盘和板型两行检查被去掉，因为 WSL 没有 `mmcblk0`。
- firstboot 的服务账户检查被去掉，因为 WSL 没有这些账户。账户表由组合工具和审计另外保证。
- `rustsimhub-install-initialize` 依赖 PID 1 和 `systemd-run`，演练时换成记录参数的替身，其中的 Edge 和 Core 步骤单独用 qemu 运行。整段流程 2026-10-23 在原设备上走通过（P6AE），但没有在首次开机的启动顺序中运行过。
- 真实内核的 vfat 只读挂载、真实 debugfs 读分区、rmtfs 和基带启动都没有在目标硬件上跑过。

## 真机刷完后要确认

1. USB 网卡出现，`10.55.0.2` 可达，维护 SSH 能用，主机指纹记录下来。
2. `systemctl status rustsimhub-firmware-extract rustsimhub-generic-install` 都成功；`.private-firmware` 存在。
3. 基带和 wcnss 的 remoteproc 处于 running，Wi-Fi 可用。
4. `ip link show wlan0` 的 MAC 在不同棒子上不同（lk1st 写入，未确认）。
5. 网页能打开并完成管理员创建。
6. 全程没有写 modemst1、modemst2、fsc、fsg、sec、persist、modem（UsbTool 的保护区比对加上系统内只读读取）。

## 进度

整体进度仍为 81.5%，行动台账 20/169，加权 10.6%。本次是离线生成刷机包，新棒子实际启动还没有发生，不增加验收点数。

## 20261108 包：USB 自动分配地址（离线）

新增 `rustsimhub-usb-dhcp.service`：只在 usb0 上运行的 dnsmasq，只做 DHCP（`--port=0`），只分配 `10.55.0.1/29` 一个地址，不给网关、DNS 和域名，租约不落盘。没有写设备，也没有真机启动。

| 项目 | 结果 |
|---|---|
| 隔离测试 | 组合树里的 ARM 版 dnsmasq 在 qemu 下运行，命令取自单元文件，两个网络命名空间用 veth 连接，客户端为 dhcpcd。客户端拿到 `10.55.0.1/29`，没有默认路由；设备端只监听 usb0 的 UDP 67，没有 DNS。测试为了在 qemu 下运行，去掉了降权参数 `--user/--group` |
| 系统树 | 组合 `9e884c773b226e7f9ff707ab6e2d25ac47f2a924`，应用仍为 main `55f97839`；12396 项，审计、只读挂载比对和 7 个 ARM64 程序运行都通过；`systemd-analyze verify` 没有报告这个单元的问题 |
| rootfs | 1,456,963,584 字节，SHA-256 `b3c7d9359eca5521e81fc2582a4e925f462e4d6febdb7d2e6961c5c3fb9b5ba4` |
| NSHSTATE | 沿用上一版，`e811ff4a…` |
| 刷机包 | `.private/rustsimhub-generic-ufi003-20261108-first.rshpack`，1,660,356,059 字节，SHA-256 `1734263c4ef0e7402cae4e7336f315b284acf93cae546b8a9deab5d3667155e0`；`pack-create` 和 `pack-import` 通过 |

UsbTool 配套改动在其仓库的 `claude/flash-two-boards` 分支，还没有合并：同一个包刷 UFI003 和 UFI001C；`tz` 与 `tzbak` 不同但都通过哈希校验时继续；自动加载刷机包；一键经 USB 读寿命；原厂棒子经 adb 读寿命（未在真机试过）。

## 多根棒子只读稳定性测试与 modem 刷新（2026-11-09）

用户提供多根棒子做只读稳定性测试：每根把 17 个关键分区各读三轮、每轮两遍，核对启动链 MBN 哈希，再用读到的 modem 和 persist 副本演练首次开机的固件提取。棒子用 `modemst1` 的摘要区分；`fsg`、`sec`、`fsc` 在同一批棒子上内容相同，不能用来区分。曾用芯片序列号和 `fsg` 区分，两次都认错了，已更正。编号和摘要只记在 `.private/stick-registry.json`。

| 编号 | 型号 | 结果 |
|---|---|---|
| #3、#4、#5 | UFI001C | modem 在绝对扇区 188570–188571（`modem.b18` 内）读取时好时坏，其他分区稳定 |
| #6 | UFI003 | 全部稳定，提取演练通过 |

三根不同的 UFI001C 都在同一位置读不稳，第一根坏棒子的 `modem.b18` 也有错误。推测是同一份基带数据多年反复被读、在相同位置衰减，但没有直接证据。

**用户 2026-11-09 明确批准一次例外：在 #5 上重写 modem 分区。** NV 和校准分区仍然禁写。

- UsbTool（`claude/flash-two-boards`，3c6d45b）新增受限 modem 刷新：只能整分区重写，载荷与设备当前内容最多相差 64 个扇区，原设备布局拒绝，fsg、modemst1、modemst2、fsc、sec、persist 必须列为保护区。工具测试 285 项通过。
- 载荷是 #4 第一轮读出的 modem 镜像，所有基带文件都通过自身签名哈希表。#5 的三次读数除了这两个扇区以外，都与它逐字节相同。
- 执行一次（步骤摘要 `216c738d…`）：写前核对差异为 2 个扇区（188570、188571）；三次回读全部等于载荷；18 个保护区段无变化；GPT 不变。状态 `verified`。
- 写后三轮稳定性测试：17 个分区全部一致，modem 也一致，提取演练通过，结论“可刷”。

这只说明刷新后立即读取是稳定的；这种稳定能保持多久，还需要过一段时间再测。
