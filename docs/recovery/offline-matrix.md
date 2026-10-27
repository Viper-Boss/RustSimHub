# C-10 / C-11 / C-12 · 真实备份恢复离线矩阵

基于 `6f228fd3dcb9ce6da54d7910f4b044833e71a91a`，只测试现有公开接口，不修改生产 storage/auth/forwarder、schema 或恢复算法。
所有身份、Owner key、短信/通话、回执、配置均为程序即时生成的合成数据；仅在新临时目录中执行。

## 如何运行

```sh
# 默认只输出计划，无构建、恢复或设备操作
python3 tools/test-business-recovery-matrix.py

# Linux：Cargo 正常测试自动调用同一实际矩阵
cargo test -p rustsimhub-recovery --locked
cargo clippy -p rustsimhub-recovery --all-targets --locked -- -D warnings
cargo fmt --all -- --check
rustfmt --edition 2024 --check tools/fixtures/backup-recovery/matrix.rs

# 也可显式运行同一矩阵，或单独验证构建元数据选择器
python3 tools/test-business-recovery-matrix.py --run
python3 -m unittest discover -s tools/fixtures/backup-recovery -p test_runner.py
```

要求 Linux、仓库锁定 Rust/Clippy、Python 3，以及已缓存的 Cargo.lock 依赖。
Windows 仅运行计划/控制器单测、原有适用 identity 测试与静态检查，不能冒充 Linux 正向矩阵。
`crates/recovery/tests/offline_business_matrix.rs` 是唯一 Cargo 测试接入，正常 Linux CI 不跳过本矩阵。

recovery crate 没有直接依赖 forwarder/storage 的全部接口，且本包不允许改 workspace/Cargo.lock。
驱动因此用正常 Cargo `build --lib --locked --offline` 在私有新 target 中构建原有 storage/auth/recovery/forwarder，
从 Cargo compiler-artifact JSON 取得精确 .rlib，再用同一工具链 Clippy driver 编译独立测试源码并执行。
不复制产品实现、不新增 Cargo manifest/依赖、不挑选旧 target 中的“最新”产物、不用测试专用恢复器。
sha2 0.10 为既有项目直接接口，age 的 0.11 ABI 仍由真实依赖加载；不升级或替换任何版本。
构建最多 600 秒/2 jobs，独立夹具编译和测试各 120 秒，输出最多 8 MiB；超时清理子进程组，错误/缺库/歧义/漏跑一律失败。
四项元数据选择单测不等于下面十五项实际业务测试通过。实际结论只认[交付报告](../deliveries/member-c-nas.md)对应 HEAD 的运行证据。

## 15 个必须实际执行的场景

| 组/场景 | 真实动作和断言 |
|---|---|
| business / wal_backup… | 原 Store 活跃且有非空 WAL；实际 Backup API 获取完整同库逻辑记录和最新短信，来源逻辑不变；输出仅 backup.sqlite3，不包括其他域；已有 final/pending 均不覆盖 |
| business / matching_identity… | 明文 Identity::backup + Store::backup/restore 同批恢复，pin 保留、realm 变化、全部授权先隔离；实际 Owner challenge/签名恢复原 ID，Owner generation 共+2，NAS 仍 inactive 且不能登录 |
| business / sms_and_calls… | 用公开 API 建立各四条 prepared/submitting/unknown/submitted 短信与通话操作，并建立真实合成通话观察/permit/confirm。恢复前三类 Unknown、submitted 保持，ID/线路/正文/记录保留，旧三类游标拒绝，重新观察并恢复 Owner 后旧八条操作仍不能取得 begin 许可，许可计数 0；关库重开再次核对 |
| business / repeated_restore… | 同一备份恢复到三个新目标，realm 各不相同；操作 ID 不变。对在线/关库已有目标均拒绝且不覆盖，正常重开保持原 realm，来源备份保持 |
| business / corrupt_missing… | 缺文件、截断、空、错误 app、未来 schema 分别调用真实 restore；来源和原业务数据保持。原已有目标保持；新目标可能留下 pending/无效副本，正常 open 仍拒绝，不清除后伪装成功 |
| identity / identity_ciphertext… | 错收件私钥、错 pin、AEAD 损坏、截断、超限、缺私钥/包拒绝；另用真实 age 加密格式正确但内部 RSHIBK02 的有效 AEAD 包，原恢复器拒绝且无明文写入。缺失原身份不被 backup 创建；合法包最终恢复 pin/seed 一致 |
| identity / identity_restore… | 身份恢复重复两次及备份覆盖拒绝，原输出保持；符号/硬链接、相对目标、公有目录/私钥模式拒绝，不放宽保护 |
| identity / mixed_device… | A 的实际 Store 备份恢复后配 B 身份，真实 Auth::open 拒绝，库和两份身份保持；绑定库缺 seed 时不自动生成；正确 A 身份仍可加载 |
| business / wrong_owner… | 恢复库不能给非 Owner 发恢复 challenge；错误证明不能恢复 Owner 或改业务逻辑记录；正确原 Owner 的真实证明才恢复，其他 Client 仍 inactive |
| outbox / closed_outbox… | 实际配置/ingest/claim/finish 建立 confirmed、unknown、in_flight、queued。仅在 Queue 关闭且无 sidecar 后复制合成 outbox/空锁；同实际 Auth 重开验证 ID/attempt/游标绑定和 confirmed 保留、in_flight→Unknown，晚确认拒绝。显式调用只领取原 queued 项，结束为 Unknown 后再重开不能重领 Unknown；源 outbox 保持 |
| outbox / restored_realm… | 同身份的两次 Store 备份分别与旧 outbox 组合；Store 恢复/Owner 恢复后真实 Queue::open_current 对两者都拒绝，旧 in_flight 尚未被改写，原 outbox 字节保持。证明的是恢复绑定隔离，不是联合批次识别 |
| outbox / same_device_mixed… | 把同身份较新业务备份 B、旧关闭 outbox A 和配置哨兵组合。实际 Store::restore 仍只接纳 backup.sqlite3 的五条短信，不检查或复制其他域；单独 outbox 检查仍可通过。明确作为现有跨域一致性缺口证据，不伪造统一校验器 |
| business / missing_outbox… | Store 备份/恢复均不制造缺少的 outbox、identity.seed、配置或联合 manifest；原身份和配置哨兵保持。库有绑定公钥不等于包含私钥 |
| business / database_backup… | 非目录/缺目标、符号链接目录/备份文件、仅 pending、公开备份文件模式等路径失败；原备份、pending 和已存在普通文件保持 |
| business / restored_unknown… | 恢复后换合成 SIM 指纹形成新 LineEpoch，同幂等键的新线路请求冲突，旧 Unknown 不能 begin；原操作/短信仍带旧线路和原 ID，拒绝不改逻辑库 |

完整函数名由驱动固定核对；Rust 测试列表及实际 passed 集合必须全部匹配，禁止 ignored/过滤成更小集合后报通过。
C-11 的 confirmed 是通过真实 Queue::finish 写入的合成确认，短信 submitted 不是运营商送达确认。
没有构造网络 transport 或调用真实发送器；0 表示该夹具未调用外发以及恢复后 begin 许可为 0，不是实际设备发送计数。

## 保留证据与限制

对关闭的身份/备份/目标比较文件摘要和目录内容；对活跃 SQLite 比较真实各表的类型化逻辑记录摘要，
不把 WAL/SHM 的内部读标记变化当业务数据变化。操作/消息/通话/receipt ID 和 attempt ID 另通过原 API/只读 SQL 核对。
不打印 seed、私钥、正文或配置值；控制器只输出固定场景名、通过数和有限状态。
outbox 没有产品 backup API，关闭后的合成文件复制只用于验证重开/隔离，不能冒充在线跨库备份实现。
没有实机、生产配置/数据、系统 D-Bus、网络发送、Root、EDL/分区恢复、设备脚本、签名或部署。
真实硬件/掉电、联合备份的准入与主线修复仍待负责人验收。
