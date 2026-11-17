# A1：统计适配器离线故障矩阵补充（2026-11-16）

后续固定来源更新见[2026-11-17 Owner换代与ARM复验](OFFLINE-A-A1-OWNER-FENCES-20261117.md)：新源码重跑原14场景并新增4场景，共18个通过；本报告保留原14场景与勘误的历史截面。

本轮在PR268内补九个故障场景，并复跑原五个场景，**十四个实际隔离场景均退出0**。
只修改专项测试、合成节点夹具和交付文档，生产Rust/Go字节未改；整个A1/A包未标完成，生产统计上报继续关闭。

## 固定来源

- 起始HEAD：`d98c76b1bf66bf7cc1c8b5c3ee11b2d9ca30b222`，main快照：`de663743bd726e77f7c560139d0f91ea78e8fe57`。
- 本轮实现HEAD：`d3e950bd5054bd6725dda22833af87fff3fed3b2`。
- 最终受测tree：`76410905c1df0ddff26f41e1a0e3f9cf0b94a6e7`。
- 完整源码tar SHA256：`1c45f077b2125ac3d3a561b916b0cecd34decb73af1c31d3fc144c601d1ea6f4`。
- 实际Core专项程序编译tree：`925cb7c4356c5dbf98cd9186e6e0f3bf94afd5d5`，tar SHA256：
  `7e6a10f4e20d6182a3402b67f3bddbd4a13c596b8081e3a53a25164e28469286`。
  到最终tree仅Python夹具变化，刷新前已用Git核对全部Rust、Cargo及Go/prepare字节相同。
- 实际Go SDK/Root复用原固定HEAD `18cbd7c405d844d24de15eb115880217cf8f5fad` 构建；
  与本轮对应Go/prepare源码相同，不声称重新构建SDK。SDK SHA256：
  `643e580a321bd29a6021b3a804f2db076e58e30c4867eadbb53ddec82edd77b0`。
- 后续仅报告变化，最终PR HEAD在PR正文登记。

## 实际矩阵

各行均使用真实Core/Auth/Machine/FileStore；除Root启动前拒绝场景外，实际使用原Root和固定完整SDK。
节点为本机合成TLS/RPC服务，独立核交易字节和摘要；不是公网节点或可信链证明。

| 场景 | 节点收到提交次数 | 实际断言 |
|---|---:|---|
| start_failure（新增） | 0 | SDK不可执行，真实Root在监听前拒绝；Core准备失败，未生成Pending或持久状态变更 |
| bootstrap_failure（新增） | 0 | 实际SDK已启动并位于原engine cgroup、UID1002、原可执行文件；合成节点拒绝引导，随后准确回收该进程；未生成Pending |
| off_before（新增） | 0 | Prepared后完成独立参与OFF，下一步不提交；原Pending保留，共享查询/Remote/Manual继续 |
| cancel_before（新增） | 0 | Prepared后取消原Machine，结果Disabled；Pending保留，共享引用不被抢占 |
| expired_prepare（新增） | 0 | 原准备票据的真实3秒截止到期后只返回Due，不能再用旧票据提交；Pending保留 |
| late_reply（新增） | 1 | 合成节点收到后延迟2.3秒，超出原观察连接2秒上限；返回Unknown，未把迟到回执记成功 |
| root_revoke（新增） | 1 | 在途使原Root authority失效，迟到回执不记成功；Pending保留并清理原SDK |
| save_failure（新增） | 0 | 仅Core专项进程软文件上限为0，真实FileStore写入返回`Store(Io(FileTooLarge))`；状态不变、无Pending、保留其他共享引用 |
| save_full（新增） | 0 | 仅专用64KiB tmpfs写满且可用块为0，真实FileStore返回`Store(Io(StorageFull))`；状态不变、无Pending，保留其他共享引用 |
| startup（复跑） | 1 | 先持久化原摘要，再单次提交；上报引用不误报显式查询开启；最终回收原SDK |
| shared（复跑） | 1 | 查询/签到/Remote/Manual共用原SDK，各自关闭不抢占；重启只查询原摘要一次，无新nonce/重复提交 |
| lost_ack（复跑） | 1 | 已接收但丢回执时Unknown；原Pending保留，重启只查询原摘要一次 |
| off（复跑） | 1 | 已发送后关闭独立参与，迟到回执不记成功；原Pending和其他共享引用保留 |
| cancel（复跑） | 1 | 已发送后取消，结果Disabled；原Pending保留，最终准确回收原SDK |

`root_revoke`是原有限Root authority失效，不冒称覆盖了Owner principal撤销/恢复换代的所有组合。
3秒专项预算只缩短原10秒上限，不延长Root/Auth授权；迟到测试仍使用原2秒观察连接限制。
准备到期使用真实Instant等待，没有改生产时钟。恢复查询沿用上一报告的测试逻辑冷却时钟，
Auth/Root/AttemptBudget真实时钟不变，仍要求节点收到原摘要，且不会再次预检或提交。

FileTooLarge场景仅对Core测试进程忽略SIGXFSZ以取得实际write错误，结束后恢复原文件上限；
磁盘满只填专用tmpfs，失败后释放填充文件。没有写满宿主磁盘、改宿主限制或替换FileStore实现。
SDK退出由Core原PID消失及Python原engine子组清理共同核验；Root manager在夹具结束前保持原PID。
每次容器为独立network/mnt/pid/cgroup命名空间，运行结束回读无残留运行容器。

## 验证命令与结果

WSL2 Ubuntu24.04、Rust1.98.0、原Cargo.lock、Linux原生固定源码；最终tree执行：

```sh
cargo test -p rustsimhub-core --lib --features usage-root-transport-candidate,email-production-candidate --locked --offline -- --nocapture
cargo clippy -p rustsimhub-core --all-targets --features usage-root-transport-candidate,email-production-candidate --locked --offline -- -D warnings
cargo check -p rustsimhub-core --no-default-features --all-targets --locked --offline
cargo fmt --all -- --check
```

分别退出0；普通Core lib **129通过、0失败、4忽略**。十四场景为同一被忽略的专项函数分别在独立容器中运行，
不把普通套件里的忽略项计通过，也不把十四场景说成十四个新增Rust测试函数。
专项实际命令（只能由真实安装角色/私有容器夹具调用）：

```sh
/core-test --ignored --exact nkn_root::check_in::tests::statistics_transport_kernel_sdk --nocapture
```

本轮未新增ARM链接/QEMU/设备验证；此前ARM库check与bin237/nkn-agent65/Auth9等结果仍仅属于前一报告的固定源码，
不冒称本轮重新执行。未改工作流，云端CI与本机结果分别记录。

## 失败保留与勘误

- 初版91c050cd及d1d59b03的子组检查失败；f1cdb1ca诊断确认唯一剩余条目是合法Root manager，
  不是SDK泄漏。改为分别核对原manager PID和engine子组，未移除SDK进程/子组清理断言。
- ec86acad：提交前关闭/取消/票据到期通过；1秒专项配置在prepare阶段被原Deadline拒绝，未进入目标迟到路径。
  后续专项短预算改为3秒、节点延迟2.3秒，仍保留原2秒观察上限和所有生产门禁；最终目标路径通过。
- 45c9fcf1：前七个故障场景通过；不可执行SDK使Root正确在监听前退出，旧夹具却等待监听而失败。
  后续显式断言Root提前退出、无manager和无Pending；另增加真实SDK已启动的bootstrap_failure，避免混淆两种失败。
- 上一报告把off/cancel写成“未发生提交”不准确，原记录均为一次提交后的在途关闭/取消，现已勘误；
  本轮独立off_before/cancel_before才提供零提交证据。

完整原始结果、日志和复验脚本仅保留私有`.local`证据包；不公开本机命名空间、进程原始记录或私有运行环境。
源代码和本报告可供B消费，不能把私有环境绑定脚本视为跨机器一键包。

## 未完成、集成与下一步

原候选feature继续默认关闭；本次无需变更wire、持久格式、依赖或Root授权上限，也不提供新启动入口。
十四场景是本轮明确矩阵，不等于穷尽原300秒Root到期、Owner撤销/恢复换代、重启及跨阶段故障的全部组合。
可信链包含证明、确认深度/链重组、长期参与授权仍待负责人决策；query保持Unknown，生产上报不启用。
继续A0逐项映射及A2/A3/A4可独立离线缺口，A5固定交付；整个A1/A包仍未全部完成。
负责人负责审核/合并及设备验收；本轮没有自审核、自合并、真实消息/电话/公网链交易、设备/NAS访问或签名部署。
来源AGPL-3.0及固定SDK来源保持；AI辅助Codex，不估造工时，不改全局台账/分母/权重。整体验收保持 **81.5%**。
