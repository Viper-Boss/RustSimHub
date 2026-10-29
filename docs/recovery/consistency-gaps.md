# C-09～C-12 · 跨域一致性缺口及最小提案

原盘点基线：`6f228fd3dcb9ce6da54d7910f4b044833e71a91a`；当前接入主线：`516228a73a7de1fa4a8a2dac40ca9f57e96a07ca`。本文只交复现和提案，不改生产格式/授权，也不实现测试专用“联合恢复器”。

## GAP-REC-01：没有统一业务备份批次/一致切面证明

真实 Store::backup 只输出自己的 SQLite 快照；Identity::backup/加密 recovery、outbox、配置是独立域。
Store::restore 没有期望的 batch ID、device/Owner/realm、外部组件摘要参数，只消费 backup.sqlite3。

可重复的合成复现：

1. 同设备、同 Owner、同 realm 通过真实 API 建立四条短信及转发回执，完成 Store 备份 A，并关闭 Queue 后保存真实 outbox A。
2. 同一业务库再接收第五条短信，完成 Store 备份 B；没有篡改 SQLite 哈希或另造状态。
3. 将 B 的完成备份、A 的关闭 outbox、旧配置哨兵放入同一临时输入目录，调用原 Store::restore。
4. Store 实际恢复 B 的五条短信；不会核对或复制旁边 outbox/配置。正确设备 Identity 仍匹配；outbox 独立只读版本/完整性检查也仍可通过。

这证明当前各自“有效”不能证明“同批/同切面”。不能因为自建 JSON 比较器拒绝了混批就宣称产品支持联合一致性。
矩阵中的 same_device_mixed… 场景断言当前事实；真实执行结果见对应 HEAD 交付报告。

## GAP-REC-02：realm 隔离不是 outbox 联合恢复方案

Store 恢复会旋转 realm、隔离所有 principal generation。即便是逻辑匹配的旧 outbox，也不能由新 Auth 自动打开；
混批旧 outbox 同样拒绝。因此 Queue 拒绝证明旧授权/任务未自动采纳，不能证明它区分了正确批次和错误批次。
不能重写 outbox 的 device/realm/Owner 绑定来“修复”，否则可能复活旧 queued/Unknown 或过期权限。
现有 Unknown/confirmed 和 attempt/receipt ID 的保留与不重投已有正常重开测试，但缺少正式联合恢复协议。

## GAP-REC-03：无效业务包可能留下新目标的隔离副本

Store 先创建目标锁、restore-pending 并复制 SQLite，再检查 app/version/integrity。
无效包不覆盖已有业务目录，但可能在新目录留下待查副本。普通 open 对这些 pending/无效版本继续拒绝；
不能宣传为“任何拒绝都零写入”，也不能删标记/清库后把一次新初始化算作恢复成功。
加密 identity restore 的边界不同：验证失败不会将解密明文写入目标，且目标必须为空。

## GAP-REC-04：旧 WAL 格式备份限制；新备份已由主线修复

首轮Linux确认：旧WAL格式备份被SQLite普通只读连接打开时，可能在可写来源目录创建WAL/SHM。
负责人已合并[PR #14](https://github.com/Viper-Boss/RustSimHub/pull/14)，提交
`f8bd02dd24c3db9587a0b52622ecfb77e77b00bc`、合并提交
`c185325062617995f5644f436c31cf8ab9ace4ad`：Store::backup发布前将完成快照切换为DELETE journal。
本批合入主线 `516228a73a7de1fa4a8a2dac40ca9f57e96a07ca`，新产出的备份恢复后严格检查来源目录/文件摘要完全不变。
已撤回旧基线为说明副作用而临时允许空WAL/有界SHM的测试助手，不忽略或删除sidecar。

该修复不会改写以前生成的WAL格式备份，旧备份仍有上述限制。只读介质、恶意预置非空WAL、
真实硬件掉电没有在本批验收，不能从新备份的临时可写目录测试推定已支持。
观察器仍只对已完成且无非空WAL的离线快照使用immutable URI；活动Store读取保持WAL感知，不能给在线库套immutable。

## 最小主线接口提案（未实现）

1. 先由主线冻结需要恢复的域：业务 Store、设备身份、outbox、NAS 历史/授权、网络账本及必要配置；区分应重建的运行时租约/会话，不把它们恢复成有效权限。
2. 定义版本化联合 manifest：明确 batch ID、包含/排除域、来源 device pin、业务 realm、Owner 公钥/generation、各库 app/schema、文件摘要及一致切面的事件/转发游标证据。身份私钥只用独立加密包；不把凭据/正文放公共报告。
3. 由实际 Core/各唯一写者提供有界协调：停止新领取、处理/保留 submitting/Unknown、取得同批快照，再以暂存→验证→发布交付整组。不能靠普通文件复制正在写入的 WAL 主库证明一致。
4. 恢复先验证全部域及已获授权的目标，再向新暂存目录运行原恢复器，验证身份匹配、Owner 隔离、realm/游标变化以及 Unknown/confirmed/原 ID 的精确关系；完整成功后由主线受限发布。保留失败证据，不覆盖原活动目录。
5. 若未来支持 outbox 显式迁移，必须专门定义新 realm/Owner 授权下的保留/隔离语义，Unknown 不自动变 queued、confirmed 不变待发，不凭旧幂等键制造新操作；不是直接替换 binding 字段。
6. 联合包还需规定完整主文件与旁路日志的关系及只读来源的准入规则，避免普通只读打开的辅助文件副作用或混入外来 WAL；先验证生产契约，再决定不可变读取策略。

以上是需主线评审的兼容/原子性提案，不是本包已交付的生产能力。签名、发布、硬件掉电和实际设备恢复依旧由主线验收。
