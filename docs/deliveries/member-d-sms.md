# 通信观测工作包交付与主线接回台账

## 2026-10-28：主线完成 D-01～D-04 的离线实现

用户明确说明 RedamancyGTW 尚未开始，并授权主线直接接手。执行者为项目负责人 Viper-Boss（本会话 Codex 辅助），不计入 RedamancyGTW 已完成工作；不更改历史 Git 作者或他人提交。

| 原步骤 | 本轮交付 | 当前负责人 | 边界 |
|---|---|---|---|
| D-01 | 实际 source/线路/ID/游标关系与不变量见下文；给出可重放的最小输入序列 | lead | 已完成离线分析，待本 PR 审核 |
| D-02 | 真实 Store/SQLite 的 70 条消息多页、乱序重观察、删除恢复、事务失败、查询失败、重开及恢复游标测试 | lead | 4 个新增测试，通过；不是双端/运营商验收 |
| D-03 | 真实 observer 的私有 peer RPC 重现对象读取与退休竞争；补代次检查及列表失败/缺失对象恢复 | lead | 3 个新增测试，通过；未连接真实 ModemManager |
| D-04 | Core/Store 共 209 项、严格 Clippy、格式检查；源码与失败/成功证据固定 | lead | 本 PR 提交交付；审核和设备验收分别记录 |

**数量结算：RedamancyGTW 原 26 步，减去本轮接回 4 步，余 D-05～D-26 共 22 步；主线接回 D-01～D-04 共 4 步。** 转移的是这四步及 SMS-02/SMS-04 离线实现责任，不是把短信模块或整个固件判定完成。个人已审步骤仍由 PR 当前 HEAD 对应的审核记录计算，不把执行者自测写成成员验收。

名册的 118 个步骤总数不变；四位成员当前持有 114 步，主线持有接回的 4 步。下一批 D-05～08、D-09～12 及后续 BLE/媒体包仍为 RedamancyGTW 待办；后续主线继续完成时逐批在本文件追加减账，不一次性撤销全部任务。原 D 包 33～75 小时为 26 步历史计划预算，本次未把其当作当前 22 步剩余工时或实耗。

## 固定来源与代码

- 起始 main：`7407976c8f566df96394310e690db1650c10598e`。
- 代码提交：`17a63d56df4bd7a3edb281dc5a788cb901e86578`。
- 分支：`codex/lead-member-d-sms-recovery-20261028`。
- 生产修改：`crates/core/src/observer.rs`、`observer/watch.rs`。
- 测试：`observer/checks/lifecycle.rs`、`crates/storage/src/inbox_recovery_tests.rs`，既有模块仅接入测试。
- 来源许可：沿用本仓库 AGPL-3.0-only；无复制外部实现、无新依赖、无 schema/协议/颜色变更。

## D-01：实际状态关系

1. Core 的 `session` 绑定 broker UUID、MM 唯一 owner、modem/SIM 路径；SimFingerprinter 派生身份摘要。`Stable` 观察稳定后，由 Store 串行 writer 提交 Ready 线路。
2. `Sources` 使用 SMS 对象路径和 incarnation（`path#n`）作为 source；完整内容摘要仅用于识别同一对象内容变化。`saved_sources` 在同一验证过的 line 下恢复来源映射。
3. Store 以 `(line_epoch, source)` 找既有记录；相同来源/内容返回原 UUID，不新增接收事件；新来源即使正文相同也保留，只标 duplicate_candidate。
4. `inbox` 的 cursor 是消息 seq，`inbox_events` 的 cursor 是事件 seq；两者共享 db_epoch，但不能混用 seq。事件与入库、删除/恢复在同一 SQLite 事务提交，客户端只在成功处理后保存自己的事件 cursor。
5. 同一 db_epoch 的历史游标可追赶仍保留的记录；格式错误、超过当前表最大 seq 或恢复后旧 db_epoch 的游标被拒绝。备份恢复更换 db_epoch，因此客户端必须重新同步，不能沿用恢复前游标。
6. 换 SIM/owner 失联等由线路 fence 阻止旧异步写入；本次进一步覆盖 SMS 对象自身退休，不将“线路未变”当成“对象仍是原代次”。真实同内容短信不会自动合并。

## D-03：已证实缺陷与修复

最小序列：已 Ready 的同一 SIM → 开始 `GetAll` 读取 SMS/1，并用信号量保持正文回复 → 观察器收到 SMS/1 的退休通知 → 放行旧回复 → 原代码确认入库 1 条，预期为 0。

修复前测试明确失败于“旧回复不应成为已确认短信”：实际 1、预期 0。修复在 session 已消费的 `(fence, revision)` 上绑定整个 reconciliation 周期；SMS 读取完成后检查路径是否在该 revision 后退休，以及队列是否溢出要求全量重建。实际 writer 中的 `receive_sms_if` 再检查一次，覆盖等候写入期间的失效窗口。其他路径退休和普通 Wake 不影响本条消息；下一周期按新代次重新读取，可正常入库。

这仅处理已收到/处理的对象生命周期通知；完全丢失通知、同路径同正文且缺少其他来源证据时，不能据此宣称能识别所有物理新短信。既有周期 List、内容摘要、线路隔离和不确定时保留候选的语义不变。

## 测试矩阵与结果

| 输入/故障 | 真实被测入口 | 观察结果 |
|---|---|---|
| 70 条合成消息，删除前 34 条，反向重观察 | Store receive/inbox/events/delete | 36 条可见，UUID 不变；1/16/32 页大小返回一致；105 个持久事件不重复 |
| 重开数据库后恢复第一条 | Store open + restore visibility + event catch-up | 旧游标仍可追赶唯一 sms_restored；恢复原 ID，其余删除标记保持 |
| 收件事件插入触发 SQLite ABORT，再重开重试 8 次 | 真事务 + inbox_event_head | 失败无消息/事件；同一来源成功后只有 1 次 sms_received |
| TEMP VIEW 制造实际事件读取/解码错误，再解除 | 真 SQLite 查询 + inbox_events | 原游标保留，恢复后仍读到同一个持久事件；重复读取 seq 不变 |
| 非法/溢出游标，0/33/最大页长，恢复前旧 realm | Store cursor 校验 + backup/restore | 拒绝且不推进；恢复后可全量读取原消息 ID |
| GetAll 中途退休，随后新周期重读 | 真实 reconcile + 私有 Unix peer + 信号量 | 旧回复 0 条，新读取正常成为 1 条 |
| 其他路径退休、普通唤醒、退休队列溢出、SIM fence、断连 | 生产 sms_valid/Notice | 只使匹配路径或明确失效范围无效，关闭 watcher 后不发布成功 |
| List 失败，再恢复且有一条无法读取的对象 | 真实 reconcile/Store/saved_sources | 原 2 条/ID/事件不变，缺失对象增加读错误，未伪装成本地空箱 |

```sh
cargo test -p rustsimhub-storage -p rustsimhub-core --all-features --locked --offline -j3
cargo clippy -p rustsimhub-storage -p rustsimhub-core --all-targets --all-features --locked --offline -j3 -- -D warnings
cargo fmt --all -- --check
```

执行环境：WSL/Linux、Rust 1.98.0、UID 65534、空 capability、无网络、无设备、只读源码及缓存、临时 SQLite 和私有 peer Unix socket。结果 **209 项通过、0 失败、0 忽略；Clippy/fmt 通过**。验证归档摘要 `1470c3cb64aaf5a4298cc29cd6bbcbd9d054c201e4906128367ed1fb19deda5f`，日志摘要 `f8ad1914ef1a914ec7393187fedcbb492edc015e7fe4efb3872cf8cf6fee1d90`。该归档是代码验证截面，后续文档/面板及最终 PR HEAD 的检查另列，不冒充自指的完整提交归档。

保留的前置失败：首版夹具的 ObjectManager 在注册时也读取属性，干扰了门控；改为无 ObjectManager 的最小 peer 后才得到真实生产缺陷复现。另一次新测试误以为 InboxCursor 实现 PartialEq，编译失败，改为分别比较 realm/seq；未修改生产游标类型来迁就测试。

## 未完成与后续

最终包含分工/面板改动的源码截面摘要为 `3c3d191d40e824c066317afcb69f546f60cb859f88a5e28d3ce8f819c12724e7`：同一隔离 Linux 环境再次通过209项Rust测试、严格Clippy/fmt、23项Python进度/真实Git归属测试和生成一致性；Windows Node面板检查通过。首轮Python在Windows沙箱临时Git目录受限而失败，另有新增测试使用公开快照缺少git_names的夹具错误，修正为真实名册后Linux23项全部通过；保留前次失败，不宣称Windows全套通过。

看板会显示主线接回4步、成员D22步；已审数仍为0，等待本PR当前HEAD审核。代码贡献仍统计于 `origin/main` 的真实Git截面，本PR未合并前不将新增行数预记主线。当前全局为原80%、16/169（加权8.4%，剩余153），本次不增加完成动作。

- 未运行真实 MM 对象重建、真实 SIM 存储满、双客户端、真实发送、换卡、ARM 或冷启动；SMS-02/04 不据此标 done。
- 未改发送器、幂等键、Unknown/提交恢复、权限、schema、Root 服务及更新器。
- CC 正在从固定旧主线制作应用 53；本修复不属于该候选包，未参与其签名、安装或设备测试，后续单独集成。
- D-05～D-26 保留；主线每完成新批次再更新本表和看板。不把 Markdown 勾选、Git 合并或测试数量换算为固件功能百分比。
