# P4N：Wi-Fi 最终保存底层事务

后续源码接线见 [P4O Root/Core 确认与回执](VALIDATION-SETUP-AP-COMMIT-OWNER-P4O.md)：Root 持久审计、Core Owner 确认/查询和只读恢复已继续实现；本页保留 e7731dd 底层阶段的范围。网页、跨进程真实 NM 及设备验收仍未完成。

构建证据更正：后续 P4O ARM 构建发现共享 target 可能复用旧配网库。此前本页 ARM 记录不再作为新增模块精确源码的编译归属证明；主机 472 项和 e7731dd 的干净 CI 分别保留。本轮已用 P4O 的固定 d4fda4b、全新空 target、刷新源码时间和实际二进制协议检查重编，覆盖本页保存后端及新增接线；不能将新结果回写为旧产物已验证。

2026-10-26。AP-03 进行中。本轮增加 Root 库内的最终 STA 提交事务与实际 NetworkManager D-Bus 后端；**尚未接通 Root worker、Core 确认命令或网页，不是已经可用的完整配网功能**。本轮无部署、切网或硬件操作；原 P0–P7 仍为 80%，剩余台账不因组件实现而计为 AP-03 完成。

## 提交顺序

`station_commit::run` 接收已经存在的父 AP 资源和隔离、实际授权 Gate、一次性确认审计与固定 Wi-Fi 凭据。初始取消、断开调用方、授权/隔离失败或审计失败均不能进入 NetworkManager。审计必须在网络变更前消费。

当前 `CommitAudit` 是产品接线契约，测试使用明确标注的合成一次性审计；尚无 Root 生产审计实现。后续必须用当前 Owner、父 AP、成功试连和确定候选 UUID 构造真实持久审计，不能用始终允许的 Gate/Audit 替代。普通试连 Session 无保存能力。

1. 创建同一无线设备的有限内层 checkpoint，启用 RAM STA 并核对精确候选、NM 唯一 owner、活动连接及 DHCP 地址。无互联网探测、WWAN/APN 或任意设备输入。
2. 明确确认后先以 `Update2` 的 TO_DISK 写入候选，保持 **autoconnect=false**；核对完整受限设置、Unsaved=false、非 volatile 及候选仍可用。回复丢失不能重复提交。
3. 再次检查授权，先销毁该父 AP 的外层 checkpoint，再检查仍只有本次内层 checkpoint，复核授权后销毁内层。任何回复不明都停止，不猜测还可回退，也不猜测已完成。
4. 在自动回退计时器都已明确清除后，再核对授权，最后写入 autoconnect=true 和固定重连优先级。核对实际保存状态后，才删除精确的旧 RAM AP profile；不删除其他 profile。
5. 核对剩余 profiles、保存状态和实际 STA 活动连接，证明 AP/checkpoint 已清理后才释放固定隔离。只有全部完成才返回 Saved。

试连会随临时 worker 的 D-Bus 连接退出而断开；最终提交使用独立的显式 commit Session，活动 STA 不绑定临时 worker 的存活。该差异仅在库的明确提交入口生效，原试连流程保留。

核对依据为 NetworkManager 官方 [AddAndActivateConnection2 接口](https://networkmanager.pages.freedesktop.org/NetworkManager/NetworkManager/gdbus-org.freedesktop.NetworkManager.html)和[checkpoint/Update2 标志](https://www.networkmanager.dev/docs/api/latest/nm-dbus-types.html)。本实现不使用 DELETE_NEW_CONNECTIONS：该标志会在回退时删除 checkpoint 后新增的连接，范围不限定于本候选。先保存但不开自动重连，避免以全局删除其他接口配置作为清理手段。

## 取消与未知结果

- 在最终 checkpoint 清理开始前，连接失败、保存失败、撤权、超时或取消均等待精确 AP 恢复并删除本候选。隔离保持，因为父 AP 还存在。只有恢复和隔离复核成功才返回 RestoredAp。
- 进入最终清理后，外层或内层 checkpoint、启用重连、删除 AP 的任一回复丢失均为 Unknown。不得自动重试保存、猜测恢复或提前释放隔离。
- 最终清理已开始时不直接丢弃 future；有界等待清理结果。请求方在这个阶段离开并不等于可以撤销已经接受的提交。
- 最后重连启用前崩溃可能留下一个未开启自动重连的持久候选。真实 Root 审计和重启核对尚待接线，必须按确定 UUID 和设置证明处理，不能把“文件存在”当成完整保存成功。
- 保存成功不能继续调用旧父 AP 的无条件 rollback，否则会把已提交 STA 当作未清理现场。Root/Core 集成必须增加明确的已提交终态与资源证明。

## 验证范围

测试分为事务次序夹具、真实 zbus 客户端至严格 NM 模型、以及组合调用。模拟模型只复现受测 D-Bus 方法与回复，不是真实 NM daemon 或棒子。

覆盖：初始权限/隔离/取消/审计拒绝、一次性消费、连接和保存失败、保存后撤权、等待 DHCP 超时、AP 恢复失败、提交后丢失回复、隔离释放失败、保存中取消、最终清理中取消，以及真实 zbus 的保存属性、两层 checkpoint 顺序、精确 AP 删除、错误候选拒绝和普通试连不能保存。组合用例同时调用事务引擎和实际 D-Bus 后端，模拟丢失最后自动重连回复，要求 Unknown、无释放隔离、无回退猜测、无重提。

第一轮工作区测试通过，但严格 Clippy 因测试中显式 drop 非 Drop 类型失败；已改为词法作用域。后续修订改为两步保存、去除全局删除标志，并补组合测试。最终第三轮工作区 **472 项通过、0 失败**，其中新增 7 项事务及 6 项 D-Bus/组合测试；严格 Clippy、生产 Core/forwarder 检查和生产 Core/setup-ap 主机构建通过。原始失败日志保留。

固定源码为 `e7731ddbe08c1aa1803911e56885afdf134d910d`，647 文件归档逐字节核对 Git，SHA-256 `f18ec700a78a43ee7e97260ead22648f69ad565111827b600b3f45e6fce1b16e`。归档源码的生产 Core/setup-ap ARM64 release 均构建通过，ELF AArch64、GLIBC 需求在设备边界内；QEMU 实际 Core 元数据确认相同归档摘要、schema6、开发夹具关闭，AP 无效命令有界拒绝。该检查证明交叉编译和元数据，不代替新增保存函数的实机执行。

精确源码 [CI40242843736](https://github.com/Viper-Boss/RustSimHub/actions/runs/40242843736)两任务均 success，已在后续轮次核对。上一轮 AP-01 的固定 `3516f45` / [CI40241225479](https://github.com/Viper-Boss/RustSimHub/actions/runs/40241225479)两任务也已成功；这些证据不代替后续确认接线的 CI。

## 代码与接手动作

- `crates/setup-ap/src/station_commit.rs`：有界提交、授权/审计门禁、取消和隔离释放。
- `crates/setup-ap/src/nm/station.rs`：显式 CommitProvider/CommitNetwork、两步写盘与精确资源核对。
- `crates/setup-ap/src/station_commit/tests.rs`：事务失败、取消与次序测试。
- `crates/setup-ap/src/nm/tests/commit.rs`：实际 zbus 与 NM 模型的组合验证。

AP-03 后续必须继续完成：受保护的确认/状态槽和 Root 不可变持久审计、与原试连摘要/Owner 代次/父 AP 的绑定、Root worker 执行与已提交终态、Core 最近 Owner 的确认及查询、重启/旧 Core 回退后的秘密清理和未知结果核对。之后接 AP-02 的简单页面及 AP-05 首次认领会话接管，再做联合安装、真实 NM/硬件保存和冷启动验收。
