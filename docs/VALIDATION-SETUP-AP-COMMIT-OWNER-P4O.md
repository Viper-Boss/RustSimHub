# P4O：Root/Core 保存确认、持久回执和只读恢复

2026-10-26。已把 P4N 保存底层接入真实 Root 可配置执行循环和 Core 的 Owner 入口。**AP-03 仍进行中**：完整网页、首次认领会话接管、跨进程真实 NM 联调、安装、设备保存与冷启动仍待验收。未访问设备或部署；原 P0–P7 仍 **80%**，剩余行动台账仍 **7/169、余 162、加权 2.4%**。

固定源码 `d4fda4b365ec22f48baa002f891be09406e80353`，656 文件归档逐字节核对 Git；SHA-256 `db5d4b89c67e1f8a9a3fdc52074dad0a6dc86e2b4b35843dede325cc4e82cb7a`。

## Core 有限接口

| 命令 | 输入 | 行为 |
|---|---|---|
| `setup_ap_wifi_confirm` | token、operation_id、trial_id、commit_id、SSID、passphrase | 最近五分钟登录的当前 Owner、network.manage、通话空闲；绑定已完成的成功试连和当前父 AP，提交独立保存意图 |
| `setup_ap_wifi_commit_status` | token、operation_id、commit_id | 当前 Owner 与 network.manage；查本地回执或向 Root 发无凭据的只读查询 |
| 已有 `setup_ap_wifi_cancel` | token、operation_id、trial_id | 保存过程中撤回仍在输入槽内的载荷、取消父租约并查询；不声称已经撤销最终提交 |

所有 ID 为规范 UUID。Core 派生原试连摘要、Owner 公钥摘要/generation、完整父绑定及时间；前端不能指定 Root 身份、路径、设置字典或 APN。Root 的保存能力独立显式声明，且必须同时支持试连；旧 availability 缺字段时默认关闭，false 继续省略序列化。

确认要求成功试连已保存精确 Root Ready/HTTPS 回执，试连未取消、父 AP Ready 仍新鲜、Core 实例匹配，且至少剩余 80 秒。保存窗口最多 40 秒，并保留父会话清理余量；并行试连/保存被拒绝。凭据必须重建出原试连的相同摘要。

Core 先写不含 SSID、密码或 token 的不可变确认 intent 与 pending，再发布受保护载荷。同 commit_id、同载荷只查询；改载荷拒绝。结果不明、Core 恢复、网页以后重新查询均不得重提保存。

## Root 请求、审计和提交

新增三个固定受保护槽：`station-commit-request.json`、`station-commit-status.json`、`station-commit-query.json`。沿用 2750 目录、640 文件、精确 UID/GID、8192 字节上限、拒绝链接和有界序列化。请求含原试连私有载荷；查询只含不可变 intent，不含凭据。Root journal 为 700/600。

Root 只接受完全匹配的成功试连及当前父会话，先持久消费确认，再发布 Saving，之后才进入网络操作。候选 profile UUID 在消费前确定为 commit_id，重启后不会失去候选身份。Root 主循环持续检查原租约、停止和精确试连取消；输出失败请求取消，但不会直接丢弃负责清理的 future。

保存沿用 P4N 两步写盘和精确 checkpoint/AP 清理。Saved 只有在实际提交、AP 删除、checkpoint 清除、隔离释放全部证明后产生。Root 的独立 `.committed` 证明不会伪造旧的 rollback/Closed 回执；最终保存成功不再走旧 AP 的无条件 rollback。

Root 先同步终态结果，再写父提交证明、清理 pending，最后发布回执。中途丢失输出或仅完成部分元数据，后续只补已验证的元数据与回执，不重新激活或保存。任何实际网络结果可能不明的错误都避免落入旧回滚路径。

## Core 确认与恢复

Core 只接受精确 intent 和当前 boot 下的新鲜 Root 观察。有效 Saving 可清除输入槽的凭据；Root 原始执行时间与当前观察 boot/时间分开，重新查询不延长硬件授权。

Saved 要求四项证明同时成立：设置已保存、AP 已移除、checkpoint 已清除、隔离已释放。Core 先持久写结果和父提交证明，再清理精确 pending，最后释放网络事务锁。写证明失败时继续保留锁与意图，不提前报告完整完成。

Core 重启会撤回旧载荷，只发无凭据的回执查询与原父取消，不重做保存。已有本地 Saved 证明可完成精确清理；缺证明时保留 Unknown。Root 重启可重新发布此前持久终态；跨 boot 只更新观察时间，保留原始执行时间。

RestoredAp 要有返回热点 HTTPS 证明；父 AP 仍存在，网络事务锁保持。相同 ID 只读原结果，另一次明确确认才可使用新 ID。Unknown 不证明“没有写入”，也不能作为重发依据；即使收到旧式 Parent Closed，缺少本次候选清理证明时 Core 仍不释放保存事务占用。没有实现基于猜测的自动硬件清理。

## 已验证的范围

最终第三轮 `cargo +1.98.0 test --workspace --all-features --locked --offline` **496 项、0 失败**；严格全工作区/所有 target Clippy、生产 Core/forwarder 检查和生产 Core/setup-ap 主机构建通过。所有本轮执行在只有回环的隔离网络命名空间，业务测试降为普通用户、无额外组/能力。

- 4 项协议测试：原载荷绑定、期限、稳定候选 ID、额外字段拒绝、完整成功证明及跨 boot 观察。
- 4 项真实 journal 测试：精确成功试连前置、不可变消费、恢复不重放、Saved/RestoredAp/Unknown、元数据中断和不伪造旧 Closed。
- 4 项实际 Root 执行循环测试：真实受保护槽和持久审计，网络/TLS 明确使用夹具；确认成功不回滚 STA，未知结果保留隔离，失败/停止先恢复 AP，改原载荷无提交副作用。包含回执重发与无凭据查询不产生硬件动作。
- 10 项 Core 确认测试：真实签名 Auth、私有 journal 和生产文件槽，Root 回执明确模拟；覆盖委托客户端拒绝、忙通话、重复/改载荷/并行拒绝、凭据清理、完整证明、恢复只查询、未知与旧 Closed、无能力/过期父回执、取消、证明写失败与显式新确认。
- Wire 与保存能力各 1 项边界测试；既有 USB Wi-Fi、短信、电话及转发测试随工作区回归。

中间第二轮因新增“发布前先检查输入槽”的守卫，模拟目录权限错误会更早拒绝，旧故障注入断言不再适用。已明确改为检查“拒绝发生在 intent 前”；恢复重启、无回执及取消不重放另有独立测试。原日志保留，最终第三轮通过；不把该测试称为已注入真实发布中断。

固定源码的两份生产 Core/setup-ap ARM64 release 在**全新空 target**中编译通过；源码文件时间已刷新，避免共享缓存误用。ELF 均为 AArch64，GLIBC 需求在设备边界内；实际两个程序分别含新增确认命令及查询槽。QEMU Core 元数据确认同一归档摘要、schema6、开发夹具关闭，AP 无效命令有界拒绝。

首次 ARM 重编读到了缺少新接口的旧配网库，保留其失败；第二个包装脚本因 CRLF 在编译前退出，原脚本保留；第三次使用 LF 脚本、全新目标目录完成。P4N 原共享缓存的 ARM 归属证明已更正。本轮可信构建依据仅使用第三次记录，不覆盖旧失败或拼接产物。

精确 d4fda4b 的 [CI40245479489](https://github.com/Viper-Boss/RustSimHub/actions/runs/40245479489)已 success；此前 e7731dd 的 [CI40242843736](https://github.com/Viper-Boss/RustSimHub/actions/runs/40242843736)也已 success，分开归属。上述证据不等于真实 Core socket 到 Root 再到真实 NM daemon 的整链路，也不证明真实设备重启后连接成功。

## 下一步

1. AP-02 简洁网页：SSID/密码、试连、明确确认、取消和同处状态，刷新只查询；不暴露手动多按钮流程。
2. AP-05 首次 Owner 认领后接管 bootstrap AP；保存接口目前面向已绑定的 Owner 父会话。
3. 跨进程真实 NM/权限、联合安装、未知状态人工核对路径及设备保存/冷启动矩阵。AP-03 的最终重启连接验收仍未完成，不计已完成动作。
4. 对未交付的旧 AP 开发版本不能假设理解新提交证明；部署/回退必须按整组组件和明确能力检查。当前已部署版本尚未开启本轮 AP 服务，实际服务兼容仍需联合安装验收。

代码入口：`crates/core/src/setup_ap_control/station/commit.rs`、`crates/setup-ap/src/channel/commit.rs`、`crates/setup-ap/src/channel/files/commit.rs`、`crates/setup-ap/src/executor/configuration/commit.rs`。本轮无新运营商短信、电话、普通蜂窝数据、刷分区或关机试验。
