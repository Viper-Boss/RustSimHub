# D-21局部交付：请求授权与私有能力

后续：[D-23持久回执](recovery-ledger.md)已离线实现（候选schema8、324项通过），本文件中“账本未实现/schema7”是上一批历史截面；唯一Wi-Fi/Root接线与生产迁移仍待。

日期：2026-10-29。代码`6c018ca18e4ef78bb3619cbff2addff212127f72`，分支`codex/recovery-capability-20261029`，堆叠在PR60。按用户“先独立分支离线实现、合并前统一审核”执行，未启用服务、迁移生产数据或操作设备。

## 本批已实现

实际代码在[Auth authority](../../crates/auth/src/recovery/authority.rs)、[真实模块测试](../../crates/auth/src/recovery/authority/tests.rs)，沿用同一个[Auth](../../crates/auth/src/lib.rs)与上一批Store授权记录。没有新Owner、scope、普通token、数据库或网络控制器。

| 入口/类型 | 实际保证 | 尚不证明的事项 |
|---|---|---|
| `set_recovery_verifier` | 可信Core配置，默认None；每次替换/移除都立即丢弃旧连接及能力上下文 | 不是HTTP/BLE命令，未提供A密码实现；不能将payload bool注入为验证器 |
| `open_recovery_connection` | 现有Core随机实例域+新随机32字节连接上下文，最多1连接；缺验证器/默认schema6拒绝 | 不是蓝牙地址或SO_PEERCRED证明，D-22仍须只从真实受限IPC建立 |
| `RecoveryRequestVerifierV1` | 可信A适配器必须校验完整设备、boot、连接、realm、Owner/grant/key代次上下文后，返回认证解密结果 | 本包没有握手、密钥算法、nonce/密文线格式或生产实现；未知profile应由A适配器拒绝 |
| `authorize_recovery` | 核对真实Store当前grant，验证前后再检查；仅固定六动作，输出私有字段能力和Zeroizing正文 | 不直接执行Wi-Fi，不创建D-23持久操作收据 |
| `RecoveryCapability` | 私有字段、无Clone/Deserialize/普通Principal转换，绑定动作、请求、正文摘要、期限、连接/会话及完整grant快照 | 只能证明这笔请求当前有权；不能替代Core事务账本或允许重复执行副作用 |
| `check_recovery_capability` | 在每个实际异步Gate重新核设备实例/连接/会话/动作/正文/期限与持久grant，Store读取后再核期限 | D-23尚未把这些检查接入现有Wi-Fi Gate，不能说保存路径已受此保护 |
| `close_recovery_connection` | 本地立即撤销连接能力，迟到的旧连接不能关闭新连接 | GATT对象/密码会话的关闭和失败清理仍由D-22/24/26完成 |

## 预算、期限与重放

- 与T5保持最多1活动连接/会话，外层信封最多2048字节；认证后的正文最多1024字节。会话ID只是16～64字节有界opaque适配器标识，不是冻结A的密码学nonce；请求关联使用非零16字节ID。
- 单会话最多使用grant批准的1～128个请求ID；没有无限增长集合。重复ID拒绝，即使正文相同也不重新发能力。新session ID不能在同连接重置额度。
- 第一个有效请求固定单调时钟deadline，上限为grant的15～300秒及适配器认证后的剩余期限二者较小值。后续请求不延长它；过期会话保留墓碑，必须断开重连，不能静默淘汰后重放。
- 证明验证耗时也算在期限内；验证过程中由另一Store handle撤权，返回后复核会拒绝。每次请求使用现有Auth有界速率检查，不接受远端传入时钟。
- 旧能力因撤销/重新批准、Auth重建、断线、适配器替换/移除立即不能再用。KeyRegistry只负责上一批的批准时公钥验证，不能替代本批的请求证明验证器。
- 重连/重启后的跨会话副作用去重仍必须靠D-23的持久账本。A还必须验证密文/转录绑定到本次新上下文；这里不把内存HashSet或摘要冒充加密认证/全局恰好一次。

## 测试和实际证据

冻结源归档SHA-256 `69d9e8230dfc0d07af53f68b7c3a8b82201d7405020de1018c1bb346913b6055`，日志`dea5bc73c7612d76ee0cdee8d8c8c1813d2b30f7db2954d5db7d2f72d115976f`。Linux/Rust1.98，真实测试UID65534、无网络/设备、空capabilities，只读源码/缓存。私有原日志不提交或公开。

```text
cargo test -p rustsimhub-storage -p rustsimhub-auth -p rustsimhub-core -p rustsimhub-policy --all-features --all-targets --locked --offline -j3
cargo clippy -p rustsimhub-storage -p rustsimhub-auth -p rustsimhub-core -p rustsimhub-policy --all-features --all-targets --locked --offline -j3 -- -D warnings
cargo fmt --all -- --check
cargo run -p rustsimhub-core --locked --offline -j3 -- --build-info
```

Auth58、Core170、Policy1、Store73，合计**302项通过，0忽略**；新增11项已包含其中。四包严格Clippy、全仓fmt和默认Core构建信息通过，默认Core仍schema6、无开发fixture。没有Cargo依赖/锁文件变化，未运行ARM、真实BlueZ、浏览器、A互操作或设备检查。

11个新入口覆盖：固定六动作/普通身份不增；缺后端/schema6/第二连接拒绝；动作/正文/其他Auth实例拒绝；撤销与重新批准；验证中撤权；重复ID/预算/session置换；deadline不滑动/过期墓碑；断线/移除后端/旧连接迟到；坏/超大信封和拒绝证明；错误binding/7种输出形状；慢证明超时。真实Auth/SQLite/能力状态机参与测试，**只有尚未交付的外部证明适配器在cfg(test)内合成**，因此不构成密码安全或生产端到端验收。

## 剩余与合并前审核

D-21目前已有真实grant/revoke和请求能力实现，仍缺A真实安全会话适配及浏览器独立非导出恢复key。D-22真实IPC/凭据/GATT接线、D-23唯一Wi-Fi事务/持久Unknown收据、D-24有限服务、D-25远程媒体和D-26跨模块故障继续保留。默认Core没有调用新增入口，无生产验证器，没有远程可用的恢复通道。

上次A交付状态已在本轮只读核验：打开的PR41仍为统计包`295c47655af1d69d2eb0568eda09023c4599c16b`；没有用统计包冒充安全会话。主线/Claude、A/B/C职责不动。D-21不扣整步，lead20/成员98/总118、D余6、行动20/169、原固件81.5%均保持；线上看板只更新真实进行中说明。

## 文档和看板验证

冻结投影归档`5ef54a702261af98b7bd7c2af3f5d5531e3d7483dc9151b0b87a4bd4dddc3b7e`、日志`8c4800ee4137a59f42293657950ad749ec2dcf9a47b90fa7f984c7778eb71f23`：隔离Linux进度Python24项0跳过与生成一致性通过；Windows Node计算/个人任务/唯一归属/转义和diff检查通过。此后只补本段证明，不重复声称Rust测试新跑。
