# C-06 · 历史重复导入、失败与恢复

本项基于 main `b1a926d6e5ab8b2c3fcc0c835280f8677b838531` 的现有导入器。
第一批和 C-05 分别已通过 PR #3、PR #6 合并。本项只增加离线验证和说明，
不增加迁移实现、生产故障开关、依赖或授权例外。

## 保持的产品规则

- 映射仍为 `(source, entity, legacy_key)`；归档 `seq` 是独立历史 ID。
- 同源同键的 canonical 内容和摘要相同则 unchanged；不同则整批冲突回滚。
- 不同来源可以保存相同旧 ID，不覆盖原来源；namespace 不能从当前 SIM 推定。
- 记录和快照批次标记在同一真实 SQLite 事务内提交。
- 源文件只读。历史只写独立归档，没有身份、Owner、当前线路或发送队列接口。
- 读者的现有身份、realm、归档绑定与逐次鉴权保持不变；游标不是授权。

## 真实模块测试矩阵

测试位于 `crates/compat/src/history_recovery_tests.rs`，注册在独立 migrate 测试程序，
与原 Go 子进程夹具和预检夹具共用测试互斥锁，不与库内文件锁测试并行启动子进程。
所有数据来自已有独立合成 `history/test-schema.sql` 和临时目录。

| 场景 | 实际动作与断言 |
|---|---|
| 同包重复 | 首次插入 15 条，再运行 5 次；每次 0 新增/15 unchanged，完整 seq/来源/键/payload/digest、批次和归档字节保持 |
| 冲突和不同来源 | 原 A 为 15 条；加入排序更早的新 ID，再改变已有 ID 内容；A 整批失败且完整原记录/批次保持。B 独立导入 16 条，再重跑 3 次不变 |
| 写入中途 SQL 失败 | 在临时目标添加 SQLite 触发器；只有同一事务已实际插入 3 条 B 短信、随后写 contacts 才 RAISE(ABORT)。失败 2 次均不留 B 记录/标记；移除故障触发器后 15 条成功，再跑 unchanged |
| 未提交进程强杀 | 为源增加 3000 条合成短信。父进程真实 SQLite 读事务阻止提交；子进程调用生产 import，观察到大于 4096 字节的实际 journal 后 SIGKILL，核实 signal=9。正常 SQLite 重开后与原 15 条/原批次完全对照，再导入 B 的 3015 条并重复验证 |
| 真实文件写入失败 | Python 标准库仅在子进程设置 RLIMIT_FSIZE=128 KiB、忽略 SIGXFSZ、禁止 core dump，然后 exec 同一个 Rust 测试程序。实际 import 返回数据库失败而非信号退出；分别验证新目标和已有 A 的目标，重开无部分 B 记录/标记，解除子进程限制后 3015 条导入成功且文件超过原限制，再跑 unchanged |
| 提交成功但回执丢失 | 子进程完成真实 import 后、输出报告前以 72 退出；父进程确认已提交 15 条/1 批，重跑 3 次不重分配 ID、不增加批次 |
| realm/代次/旧映射 | 正常重开可继续历史分页；NAS principal generation 改变后旧授权拒绝，重新授权后旧 audience 游标仍拒绝。旧 access 不得用于其他 realm 或变化后的归档，新 realm 无授权时也不能用旧游标；归档变化后旧 epoch 游标拒绝，A 的记录 ID 不能当作 B 的 ID |
| 业务隔离 | 合成旧记录故意包含与旁边哨兵相同的 request_id，以及 submitting/dialing 状态。实际 HistoryService 列表/详情始终 archived；归档只含 history_records/history_batches，来源、相邻合成当前库、身份和 Owner 字节不变 |

上述是测试动作及断言，执行结果必须看对应完整 HEAD 的
[交付报告](../deliveries/member-c-nas.md)，不因写出矩阵就视为已运行成功。

## 运行与恢复边界

```sh
cargo test -p rustsimhub-compat --locked
cargo clippy -p rustsimhub-compat --all-targets --locked -- -D warnings
cargo fmt --all -- --check
```

新增测试仅 Linux 执行；子进程测试要求 PATH 有 python3（只用标准库），原互操作夹具仍要求 Go。
`import_worker` 是仅供父测试带 `--exact --ignored` 调用的子进程入口；普通测试运行显示它被忽略，
父测试必须实际调用并校验结果，不允许缺 Python 或子进程失败时跳过。
子进程最长 30 秒，观察 journal 最长 10 秒；退出/失败时有界清理子进程，临时数据随夹具释放。

故障通过真实文件、SQLite、操作系统进程和文件大小限制触发；没有替代导入器。
文件大小限制导致写入 EFBIG，不是磁盘空间 ENOSPC；SQLite ABORT 也不是物理写满。
进程 SIGKILL 不等于硬件掉电、存储缓存失效或断电 fsync 证明；不夸大 journal 为必然已经刷盘的 hot journal。
重开验证使用正常 SQLite 读写连接触发其恢复，不手工删除 journal、清空归档或删除来源。

当前导入器在新目标创建 schema 后才导入记录。因此中途失败允许留下有效的空历史归档，
但不能留下半批业务记录或成功批次标记；恢复后的原导入器可继续使用它。
冲突仍须人工核对来源与内容，不因重试自动覆盖、改 namespace 或放宽 tombstone 规则。

SIM 代次夹具中的 current.sqlite 是相邻合成哨兵，只验证本模块不改它，
不是 Core/真实 SIM 换代的替代实现；本模块没有当前 SIM 输入。
真实 Core 对象换代、设备发送次数、真实旧 NAS、硬件掉电与端到端恢复仍由主线验收。
C-07 的完整权限/输入矩阵与 C-08 的综合交付不计入本项完成。

来源为上述固定 main 的真实 history/import/HistoryService 及既有独立合成 schema，
新增测试保留 AGPL-3.0-only；无新第三方依赖、无新 Go 模块。
