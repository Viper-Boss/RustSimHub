# C-09 · 备份范围与恢复语义盘点

固定源码：main `6f228fd3dcb9ce6da54d7910f4b044833e71a91a`。
本盘点只读公开源码，不读取设备、用户目录、真实备份或维护凭据；表内是实现事实，不是整机恢复验收。

| 域 | 实际接口/源码 | 包含与不包含 |
|---|---|---|
| 业务 SQLite | [storage/lib.rs](../../crates/storage/src/lib.rs)，`Handle::backup` → actor 内 `Database::backup` | 使用 rusqlite Backup API，从实际连接读取已提交数据，包括 WAL 中的提交。输出 `backup.pending`，完成/同步后发布 `backup.sqlite3`；不是复制在线主文件。包含 meta/身份公钥绑定、principals、短信操作/尝试、收件箱/事件/删除管理、通话及操作/事件等实际同库表 |
| 明文设备身份 | [auth/identity.rs](../../crates/auth/src/identity.rs)，`Identity::backup/load` | 独立 72-byte `identity.seed` 和空 `.identity.lock`，RSHKEY01/摘要和公钥验证、私有权限、不覆盖。业务 SQLite 只有绑定公钥，没有此私钥；Owner 的 client.seed 也不在设备业务备份中 |
| 加密设备身份 | [recovery/lib.rs](../../crates/recovery/src/lib.rs)，`keygen/backup/restore` | age X25519；112-byte 明文仅为 RSHIBK01、公钥、seed；ciphertext 最大 8192 bytes。完整解密、魔数/校验和/预期 pin 验证后才向新空私有目录写 seed/锁。不是短信/通话/授权/outbox 或系统备份 |
| 转发 outbox | [forwarder/queue.rs](../../crates/forwarder/src/queue.rs)，`Queue::open/open_current` | 独立 `outbox.sqlite3`，保存配置/请求/任务/attempt/回执/显式历史批次及游标；DELETE journal，独占锁。没有同 Store 的公共 backup/restore 联合接口。恢复会核对 device、realm、Owner 公钥及 generation，不自动采纳旧绑定 |
| NAS 历史与授权 | [compat/history.rs](../../crates/compat/src/history.rs)、[reader.rs](../../crates/compat/src/history/reader.rs)、[session.rs](../../crates/compat/src/session.rs) | 独立 history.sqlite/access.sqlite/legacy.sqlite，不被 Store::backup 递归收集；历史不是当前操作，授权还绑定身份/realm/归档。C-05～08 的离线验证不变 |
| 部署配置/网络账本/TLS | 下方公开部署清单 | 独立部署域，不被上述 SQLite 或 identity 接口自动收集；其现场内容、密码/密钥及一致性未在本包读取或恢复 |

## 配置与目录清单的公开依据

- [Core 服务单元](../../firmware/system/overlay/etc/systemd/system/rustsimhub-core.service)的 `--serve-device` 分别传入 state、identity、archive、access、compat、journal、setup 目录；[Core forwarding](../../crates/core/src/forwarding.rs)使用 state 下独立 forwarding 目录。
- [Edge 服务单元](../../firmware/system/overlay/etc/systemd/system/rustsimhub-edge.service)从独立 installation/config.json 启动，部署/TLS 配置不在 Store 表中。
- [网络服务单元](../../firmware/system/overlay/etc/systemd/system/rustsimhub-network-agent.service)声明独立 system/network 持久目录；网络账本/意图不能因业务库备份成功就称已备份。
- [首配服务单元](../../firmware/system/setup-ap/rustsimhub-setup-ap.service)区分 installation 配置与 journal；运行时 lease/socket 也不是可恢复的持久授权。
- [系统树说明](../../firmware/system/README.md)涉及基础系统/程序/单元及安装材料；本包不执行组合、设备备份、EDL/分区恢复、签名或部署脚本。

以上只列公开路径类别，不提供现场配置值。测试中的 synthetic-settings.json 仅是证明未收集外部配置的哨兵，
不是另造配置解析器或宣称正式配置可恢复。即使 outbox 位于 state 子目录，Store::backup 也只备份其数据库连接，不递归备份目录。

## WAL、发布与失败保留

Store 的 actor 串行处理数据库工作，Backup API 在 SQLite 层取得一致快照。
测试保持原 Store 活跃并确认非空 WAL，再核对备份真实表和最新收件记录；没有把在线主文件副本当备份。
原 backup.pending/backup.sqlite3 不覆盖、不自动删除；失败保留材料。

`Store::restore` 限制输入名为 backup.sqlite3，复制到另一目录后才做 app/version/quick_check 和恢复处理。
因此不合法输入可能在新目标留下 `.restore-pending` 与未接纳的数据库；不能声称拒绝总是零写入。
原来源和已有目标不被覆盖；有效旧目标/在线写者分别受 BackupExists/AlreadyOpen 保护。
“不覆盖”不等于来源目录零写入：Linux 下该 WAL 格式快照被 SQLite 只读打开时，仍可能新建
`backup.sqlite3-wal` 和 `backup.sqlite3-shm`。首轮真实矩阵已观察到这一点；原有文件逐个核对摘要，
新增仅允许空 WAL 和有界 SHM，完整有效快照再核对逻辑记录。不能将来源写保护或整目录字节不变宣传为现有保证。
测试对已完成的独立快照使用带 `immutable=1` 的只读观察器避免观察本身创建辅助文件，
且先拒绝非空 WAL；活动业务库仍使用正常读取以覆盖真实 WAL 提交，产品 restore 的打开方式不变。
测试保留新目标失败证据，并检查 pending/无效版本不会被普通 open 当作空新库初始化；不删除证据来重跑。
Store 的“新目标”约束主要是没有现存 store.sqlite3，并不等同于 identity restore 要求整个目录为空。

## realm、Owner、操作和游标

- 实际 `Database::recover(restoring=true)` 轮换 db_epoch；原 Store/inbox/calls 游标必须重新同步。
- 所有 principal 变 inactive、generation+1；Owner ID/公钥保留。原 Owner 通过真实恢复 challenge/proof 后单独恢复，generation 再加一；Client/NAS 不随之复活。
- 恢复时 prepared/submitting 短信与通话操作变 Unknown；已有 Unknown 不变，submitted 保留。普通重开也将 submitting 隔离，不自动发送。
- 通话原记录保留，observed 变 reconciling；线路恢复成 unavailable，须重新观察，不能把备份中的现场状态当当前硬件事实。
- outbox 正常同绑定重开将 in_flight 变 Unknown，confirmed 保留；原 queued 并不一律取消。业务 Store 恢复后 realm/Owner generation 已变化，旧 outbox 由实际 Auth 绑定检查拒绝，而非自动重排发送。
- 这些是各模块规则，不构成跨 SQLite/身份/配置的联合 checkpoint。具体缺口及最小提案见[一致性缺口](consistency-gaps.md)。

实际测试结果与完整 HEAD 见[成员报告](../deliveries/member-c-nas.md)，矩阵见[离线验证](offline-matrix.md)。
