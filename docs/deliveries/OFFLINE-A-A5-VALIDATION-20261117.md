# A5 测试与交付（2026-11-17）

状态：本轮固定来源的离线验证和复跑文件已交付，待负责人审核。它不替代 A1～A4 尚未完成的生产/工具/现场工作；整体固定里程碑保持 **81.5%**。

## 来源与范围

- 起始 main：`de663743bd726e77f7c560139d0f91ea78e8fe57`。
- A5 工具 `implementation_head`：`ff0130b8d70460b0a86f16248042ea5505fdf12f`；报告为后续独立提交，PR 正文记录当前完整 HEAD，避免自引用。
- 分支：`codex/offline-a5-validation-20261117`，同仓库 PR 到 main，不改 main 或他人分支，不自审核/合并。
- 对应 A5.1～A5.6；本 PR 只含 `tools/offline-a/{validate_a5.py,validate_sdk_a5.py,runtime_a5.py,run_a5_isolated.py,test_a5_validation.py,README-A5.md}`、本报告和脱敏证据 JSON。无生产代码/接口/数据库/依赖/工作流改动。

受测来源分别固定，**没有合并成一个产品，也没有证明跨 PR 整合通过**：

| 来源 | 完整提交 | 边界 |
|---|---|---|
| main / 已整合的统计、转发及 Root 候选 | `de663743bd726e77f7c560139d0f91ea78e8fe57` | 不含完整 CheckInTransport 草稿 |
| A2 / PR272 | `fd3358ef1cee1bc2305ca96d13f55ff1ac3fc4ae` | 尚未合并的 NKN/远程候选 |
| A3 / PR273 | `c2a60903edb30a4bf4f3efb4dffa244da58d1cef` | 尚未合并的通信/NAS 候选 |
| A4 / PR274 | `774d4a05a1ff5191297f5dfa0ca4e0d0a0f89b6b` | 尚未合并的 USB/隐藏网络候选 |

PR268 `924e4b7441cf61398423a4e3281608bfa3776215` 仍是草稿，本轮不接纳为产品基线。源码从本地 Git 对象重新 clone/checkout，每个来源核祖先、完整树及锁文件，不使用当前工作区的未提交产品字节。

## 已执行验证

Ubuntu-24.04 WSL2，Rust1.98.0、Go1.27.3、Node24.22.1。所有产品运行检查在独立网络命名空间、仅 lo、UID1000、CapEff/CapBnd=0、NoNewPrivs=1 下执行。ENOSPC 另加私有挂载命名空间和精确 16MiB tmpfs；无真实设备、普通蜂窝、公开 NKN/RPC 或真实通信。

本轮有 9 份通过的观察记录、98 条实际命令记录（含 clone/版本/编译/格式等，**不是98个测试或验收动作**）。完整命令、退出码、原始忽略数、日志 SHA256 及固定来源见[脱敏证据](../offline-a/a5-validation-evidence-20261117.json)。参数路径以占位符规范化，原始日志和实际路径保留在本地测试包。

| 检查 | 实际结果 |
|---|---|
| main：usage-stats/nkn-agent/forwarder 默认及 transaction/SMTP 生产组合 | 全部退出0；原有忽略不计通过，不跨重复套件累计 |
| main：原 statistics signer、Root、网络鉴权 | 分别5通过、16通过/1忽略、1通过；含拒绝、撤权、原摘要持久化/重启核对 |
| A2：agent、Auth NKN、默认普通会话、生产 NKN/Root | 65、9、1、9/4忽略、16/1忽略，退出均0；默认 NKN候选未编译，不冒充该模式已运行 |
| A3：storage、默认/生产 observer/calls、默认/SMTP forwarder | 全部退出0；storage74/1忽略，observer各18，calls23/24且各1忽略；详细分组见证据，不重叠求和 |
| A4：Wire隐藏协议、普通试连、防重重启、关闭模式IPC、目录撤权 | 各选择均实际非空并通过，原试连6/防重7；网络与纯灯组合63通过 |
| 各来源 fmt / 严格 Clippy / Core ARM | `cargo fmt --all -- --check`；适用crate `cargo clippy --all-targets --all-features --locked --offline -- -D warnings`；Core生产组合 `cargo check --lib --bin rustsimhub-core --target aarch64-unknown-linux-gnu --locked --offline`，均0 |
| Node | main3组、A2五组、A3六组、A4七组，均0；有重叠，不算21个独立功能 |
| SDK固定链 | 原31文件、compatibility/minimal/mask-backport/managed四层补丁、WebSocket归档和模块sum、完整before/after树、固定向量及engine复制均核验；go.mod/sum测试前后不变 |
| Go原SDK及引擎 | 原managed回归通过；默认SDK及交易标签各58个顶层测试通过、各200个含子测试pass事件；race/vet均0，顶层与子测试不混算 |
| SDK/Root ARM | 实际编译并核ELF machine183及SHA256，未执行；Rust ARM同样只做编译检查，未跑QEMU或实机 |
| 双端真实Auth/IPC | 原Core库/存储与两个委派读取端，游标/删除/撤权/重启1项通过，0 |
| 五处真实ENOSPC | 原storage精确ignored用例显式运行，五个发送边界均保留原ID/状态/许可栅栏，0；tmpfs卸载、生成空目录移除 |
| NAS→Edge→Core | 原Go适配器race用例、实际Edge/Core测试进程，9次转发；Create/Start/Hangup各1、tones0，0；均为合成对端，无真实拨号；子进程全部停止 |
| USB默认/生产真实actor+NM适配 | 两个特性组合各实际运行1个顶层测试中的17场景，原principal/原ID/隐藏模式/撤权/回退/Unknown均通过，0；私有D-Bus停止 |
| A5观察器负对照 | 三项自身测试通过：故意的exit42、零测试均被拒绝；单项成功不能将部分报告标为整套通过。不是产品失败或产品测试数量 |

Rust运行特性为各crate实际 default、usage-stats/transaction-candidate、forwarder/smtp-production-candidate，以及Core `usage-root-transport-candidate,email-production-candidate`。具体参数由观察器逐命令记录。Cargo锁定离线；Go禁下载/自动工具链，原四层校验未降级。

### 保留失败与历史证据

1. 首轮A2默认组合误选 `nkn_product::`，编译/进程退出0但实际零测试；观察器拒绝并保留 `a5-a2` 原报告和日志。改选当前默认可运行的普通签名会话测试，生产组合仍测真正NKN，在新输出 `a5-a2-corrected` 完整通过。未删断言或把零项当成功。
2. USB最终调用曾因自动审批服务额度失败而**没有执行**，不是产品测试失败；用户继续后重新通过审批，实际默认/生产17场景均通过，未绕过检查。
3. A2原12-peer UID/cgroup/Root/SDK/Noise容器链及旧代码负对照，仍按PR272原固定实现/报告引用，本轮没有重跑，也没有在宿主机模拟Root容器布局。
4. A3旧NAS header负对照、A4旧计数/鉴权负对照及实际浏览器三宽证据仍保留在各原PR报告。本轮没有新跑真浏览器、QEMU或整仓测试；只对表中列出的命令和来源负责。
5. 原有ignored保留，特定NAS/ENOSPC/USB测试只有在显式隔离前置下单独运行才记该场景通过；其他ignored仍不计通过。热点专用、BLE/AirSIM/声音、B的发行/工作流与设备套件不执行；严格检查编译这些依赖不等于运行它们。
6. GitHub Actions对本PR最终HEAD另行回读；本机成功不能写成CI成功。既有PR的零步骤failure不改写、不自动重跑，不猜测仍是同一种计费原因。

## B/负责人复跑与集成

看 [README-A5](../../tools/offline-a/README-A5.md)。先验证本PR完整HEAD及来源对象，再准备固定工具链/缓存。`run_a5_isolated.py`只做命名空间/16MiB临时挂载设置，随后清空组/capability，以非root运行固定runner；拒绝root测试UID，不能传任意root执行器。`validate_a5.py`按独立固定来源运行；`validate_sdk_a5.py`复用原仓库验证器重新校验完整链；`runtime_a5.py`先核已构建测试二进制SHA256，再运行限定场景。

不提供让B直接运行的未知二进制。B从已提交源码重建。缺Rust目标库/Go/cache/dbus/权限时报告准确前置并保留原失败；不得关闭来源校验、改feature或删门禁凑绿。新增命令/退出码已经写入本包文件，供B消费；未修改B的CI/打包入口，也未通过消息工具宣称已通知某位协作者。

本工具不生成应用/Rootfs/签名载荷或与既有发行格式竞争的build清单。证据JSON明确为test-observation，不是可安装/发行批准。合并本PR只加入工具和材料；回退可revert本工具提交和报告提交，无迁移、部署或设备恢复步骤。

自有Python/材料沿用AGPL-3.0-only、标准库，无新增依赖；原SDK/Go及补丁的许可证/来源保留，不冒充Rust实现。AI辅助编写与核验，未估造工时、改作者/权限/台账/整体百分比。测试包只包含合成日志、工具及报告，不含真实通信、令牌、证书私钥、设备备份、浏览器身份、模块缓存或测试二进制。

## 仍需完成

主线审核及跨PR整合、云端CI、A1完整生产传输/长期权限、A4旧Session重启回执契约、受保护外部电脑工具接线，以及实际USB/Wi-Fi/隐藏路由器、真实通道/公网/灯控等验收保持未完成。热点按用户要求暂缓。A5离线验证交付不把这些项销账或提高81.5%。
