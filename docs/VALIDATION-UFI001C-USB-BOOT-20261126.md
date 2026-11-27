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

## 2026-11-27 早期 USB 对照试验

普通拔插后指示灯亮，Windows 未识别设备。重新进入 9008 后，再次只读确认 GPT、boot 镜像头和四个 ext4 超级块与原镜像一致，仍无挂载记录。

经授权，将复用相同内核和设备树的早期 USB 诊断镜像仅写入 `boot`，整分区回读摘要一致，重启已确认。诊断镜像仅在内存中运行，检查 eMMC 分区可见性并尝试注册一个 USB 诊断设备。模拟验证覆盖分区齐全、缺少配置分区和无 eMMC 三种情况。重启后 Debian 虚拟机只收到 9008 断开事件。Windows 主机侧的结果待确认。诊断镜像仍在测试棒子上，原 boot 镜像的恢复需要重新接入 9008。整体验收保持 81.5%。

## 2026-11-27 恢复原启动镜像

Debian 虚拟机已配置仅针对 Qualcomm `05c6:9008` 的 udev 规则。当前维护账号以普通权限打开 USB，并通过 Sahara 重新核对同一根棒子。复核主备 GPT 与诊断镜像的完整摘要后，仅写回签名固件 130 的原 `boot`。整分区回读 SHA-256 与原签名清单一致，设备确认重启。正常启动和 USB 枚举仍待验证。整体验收保持 81.5%。

## 2026-11-27 固定 USB 设备角色试验

在原内核与 A/B 选槽逻辑上生成第二个启动候选。UFI001C 设备树固定 USB 为 `peripheral`，保留 VBUS 检测。内核启动后红灯设置为 heartbeat，初始内存文件系统开始选槽时点亮蓝灯，选出根槽后切换到绿灯。源码见 `tools/system/build_ufi001c_usb_candidate.py`。

候选仅写入 `boot`，整分区回读摘要一致且重启已确认。Debian 未收到新的 USB 枚举，实机观察为蓝灯快速闪烁且没有绿灯。该现象和诊断设定的蓝灯常亮不符，无法据此确定选槽是否执行。再次进入 9008 后，GPT、候选启动头一致，两槽与配置、录音分区的 ext4 超级块均未变化。候选仍在测试棒子的 `boot`，设备当前在 9008。

最初识别依据是基带固件标签。已有 [另一板型的实机记录](https://github.com/OpenStick/OpenStick/issues/92) 出现相同标签。应使用 PCB 丝印或独立硬件证据确认型号后再选择设备树。整体验收保持 81.5%。

## 2026-11-27 PCB 与 Fastboot 对照

实物丝印确认 `UFI001C_MB_V01`，板上有标注的串口焊盘。用可逆的启动头对照触发原 OpenStick LK1ST 的 Fastboot，Debian 枚举 `18d1:d00d`，LK 日志显示识别了 eMMC 和分区表。原 LK1ST 从内存加载当前 ARM64 镜像和 OpenStick 内核对照镜像，均在约 3 秒后重新出现 Fastboot。经校验的上游 LK2nd 23.1 可以在内存运行；由它启动现有 A/B 镜像或使用独立静态 `init` 的最小 ARM64 测试镜像，约 20 秒后回到原 LK1ST，未出现正常 USB。OpenStick 内核经 LK2nd 启动后停留超过 40 秒，虚拟机仍未见 USB。以上 RAM 启动没有写入镜像。

通过 9008 恢复启动头，整区摘要与固定 USB 角色试验镜像一致并确认重启。恢复前只读比较了已有引导组件的主备摘要。SBL1、RPM、HYP 均与 OpenStick `base-generic.zip` 中的对应文件不同，参考包的 TZ 文件超过实机分区大小，不能直接用于这个布局。引导组件差异待进一步核实，正常启动与 USB 尚未通过。整体验收保持 81.5%。

## 2026-11-27 ARM64 引导链与迁移候选

原 LK 的 4 秒和 11 秒延时复位探针对照均约 3 秒返回。经 LK2nd 的两组均约 21 秒返回。观察不到随探针设定变化的复位时间，仍未确认 ARM64 负载开始执行。

将测试棒子上回读的 SBL1、RPM、TZ、HYP 主备镜像按 ELF 有效区间与独立校验的 UFI001C 原厂备份比较，八份相应字节均一致。现有 ABOOT 的有效载荷则与 OpenStick v1 的 ABOOT 完全一致。原厂内核与 HYP 使用 ARM32，OpenStick ARM64 基础包使用 AArch64 HYP。其 TZ 有效数据为 605,312 字节，原厂 TZ 分区只有 512 KiB。这些对照使早期引导链成为主要排查方向。启动故障仍待实机迁移试验确认。[OpenStick v1 发布包](https://github.com/OpenStick/OpenStick/releases/expanded_assets/v1) · [OpenStick 分区与提取脚本](https://github.com/kinsamanka/OpenStick-Builder/blob/master/scripts/extract_fw.sh)

基于已核对的主备 GPT 生成了离线迁移候选。主 TZ 保持起点并扩至 1 MiB，备用 TZ 迁到 1 MiB 对齐的空档，添加 4 扇区的 CDT。其他 30 个原有表项逐字节保持不变，包含两套系统槽、基带和校准。候选的两个新位置已对照确认在原厂参考镜像中全为零。实际棒子的这些位置仍需回读。候选使用固定摘要核对五个 OpenStick 载荷及其目标分区大小，原有引导镜像和 GPT 已按摘要保存在本地私有目录。

刷机工具已在写入前识别并拦截这组已知原厂 HYP/TZ。离线工具仅生成候选文件，264 项非 GUI 测试通过，1 项跳过。虚拟机缺少 Tkinter，GUI 测试未运行。[RustFlashKit 提交](https://github.com/Viper-Boss/RustFlashKit/commit/4d4728d271b521fdd52d40386b0bfb2b9f50d233)

虚拟机当前没有枚举到测试棒子，本次未写入或重启设备。下次在同一设备进入 9008 后，需要核对现有 GPT、两个迁入范围及回退数据，再验证迁移后的 Linux 启动、正常 USB 枚举和 A/B 切换。离线候选尚未通过真机刷机验收。整体验收保持 81.5%。

## 2026-11-27 迁移回退演练

对照 OpenStick v1 的两份原始 GPT，CDT 除分区类型外还有固定属性。候选 v2 保留该属性并重算主备 GPT 校验。仍只有原有的 TZ 主备表项和新增 CDT 有差异，其余 30 个表项逐字节一致。

回退清单覆盖 12 个区段，包括原有启动链、两份 GPT、迁入的备用 TZ 与 CDT。正式回读要求所有区段读两次且一致，两处迁入范围须全零。在与测试棒子同容量的 3.91 GB 稀疏磁盘副本上复用了已回读的引导数据和 GPT，演练了候选写入及回退，整盘 SHA-256 恢复到写入前值。测试也验证了拒绝有内容的迁入范围、两次回读不同和 GPT 不一致。镜像内的空档是模拟数据，实机空档仍待回读。

原厂与 OpenStick 镜像中的根证书字节不同。保存的 Sahara 哈希与现有引导、候选引导及已能连接的 EDL 加载器的 DER 证书和所测公钥表示均未匹配。该比较不能判定此板的启动验签状态。应由同一棒子的实测结果确认引导是否兼容。

非 GUI 测试运行 `python3 -m pytest -q --ignore=tests/test_gui_flash.py`，265 项通过、1 项跳过。虚拟机仍未枚举出测试棒子，当前阶段未执行实机写入，正常 USB 和 A/B 验收仍待完成。整体验收保持 81.5%。[RustFlashKit 提交](https://github.com/Viper-Boss/RustFlashKit/commit/a00863084af1031903ff98fa7c7815975517919a)
