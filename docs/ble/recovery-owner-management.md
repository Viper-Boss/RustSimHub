# D-21 局部交付：恢复授权记录与近期所有者管理

后续：[D-23持久回执](recovery-ledger.md)已离线实现（候选schema8、324项通过），本文件中“账本未实现/schema7”是上一批历史截面；唯一Wi-Fi/Root接线与生产迁移仍待。

后续进展：[请求授权与私有能力](recovery-capability.md)已离线实现、302项通过；本文件的“authorize未实现”保留为上一批历史截面。A真实会话/浏览器/IPC/Wi-Fi仍待，不据此称D-21完成。

日期：2026-10-29。代码提交：`c5f9ea7d316b8c64d986fb340ddf26d38688a107`。独立分支 `codex/recovery-authority-20261029`，基于PR59；正常审核前不合并、不安装、不启用。用户已明确允许PR52/53/56方案先离线实现、主线合并前统一审核；这不代表提案已获得技术审核。

**D-21尚未完成。** 本批交付真实Store/Auth管理代码及16个新增测试，未实现`RecoveryAuthorizerV1::authorize`、密封的Core能力、浏览器非导出密钥、A密码会话、IPC或Wi-Fi接线。成员D仍余D-21～26共6步，lead20/成员98/总118保持；原固件里程碑81.5%、行动台账20/169不加点。

## 实际源码与入口

| 文件 | 已实现内容 | 使用边界 |
|---|---|---|
| [Store入口](../../crates/storage/src/lib.rs) | 显式`open_recovery`/`restore_recovery`，单writer和现有备份/恢复流程 | 默认`open`/`restore`仍只接受schema6；Core未调用新入口 |
| [恢复记录](../../crates/storage/src/recovery.rs) | grant/revoke/reapprove、CAS、Owner/realm核对、持久审计与容量拒绝 | `check_recovery_grant`只检查当前记录绑定，不是会话认证、bearer或私有能力 |
| [候选DDL](../../crates/storage/src/recovery.sql) | STRICT授权记录/审计表，显式6→7事务迁移 | 当前schema7仅含grant/审计，不含D-23操作账本；不是生产可安装格式承诺 |
| [Auth管理](../../crates/auth/src/recovery.rs) | 近期Owner加NetworkManage，批准/撤销/列表/审计；验证密钥前后复核近期登录 | 沿用同一Auth/Store/Owner，不增加普通Scope、Kind或登录token |
| [Store测试](../../crates/storage/src/recovery/tests.rs) | 10项真实SQLite/Store边界 | 真实临时库、失败事务、并发CAS、重开/备份恢复 |
| [Auth测试](../../crates/auth/src/recovery/tests.rs) | 6项真实Auth登录/权限/重启边界 | 只有密钥注册器是cfg(test)合成后端，不存在生产默认放行 |

`RecoveryKeyVerifierV1`是可信Core注入的密钥类型/编码验证接口，**不是安全会话验证器**。缺注册器、未知profile、坏输入拒绝。生产未提供任何算法实现；不假定A会采用Ed25519/32字节、ECDH或自创握手。恢复公钥保持有界opaque数据，不能复用普通Principal/设备身份公钥。

## 数据和撤销语义

- 授权绑定当前db realm、唯一Owner的id/key/generation、独立恢复key profile/key、grant generation和约束。每次管理操作先通过真实近期Owner规则；Store writer再核对持久Owner。
- UUID规范格式；profile 1～64个ASCII字母数字或`-_.`；公钥1～1024字节。会话预算15～300秒、请求预算1～128。只规定上限，尚无安全会话执行这些预算。
- 最多32个授权，包含撤销墓碑；满额拒绝，不静默删除墓碑。同key重新批准必须先撤销并提供当前代次，沿用原id、推进generation；陈旧CAS拒绝。
- 授权变更与审计在同一writer事务；注入审计失败时整体回滚。只保留最新256条审计历史，授权墓碑不回收。这里没有操作收据，不能把审计环当幂等账本。
- 普通进程重启保持授权记录，普通Owner bearer不恢复。备份恢复推进realm并隔离全部普通身份与恢复授权；必须经既有Owner恢复流程后，当前Owner再明确重新批准。不会创建第二Owner或重开首次认领。
- Debug不输出key/grant内容；不保存私钥、Wi-Fi口令或可重放会话秘密。授权快照可构造/比较，但不Deserialize、不充当授权能力。

## 迁移与回退的实际限制

只有显式可信调用`Store::open_recovery`才建立候选schema7；迁移错误事务回滚到完整schema6。普通schema6读者遇到7在WAL/迁移前拒绝，测试核对拒绝前后数据库字节不变。显式读者拒绝未来版本8、未知记录格式、损坏数据和缺表，不重置Owner。

**不得用本批给生产数据库升级并确认健康。** 旧回退程序只接受6，尚未解决现装/候选/回退三方的数据兼容、D-23持久操作账本及更新材料准入。候选schema7仍可能在后续设计中修订，已有生产未承诺接受此格式。实际Core默认`--build-info`仍报告`database_schema:6`、setup AP契约schema6、`development_fixtures:false`；没有新增Core/wire/HTTP/BLE路由或调用点。

## 真实验证与保留的失败

环境：冻结源归档，Linux/Rust1.98，实际测试UID65534，无网络、无设备，空capabilities，只读源/缓存；root只用于隔离启动器。不是棒子ARM、BlueZ、浏览器或密码互操作验收。

最终源归档SHA-256：`e78d980713b63e80c657238fa30b284bafc53a757a443f7a2b24c9d852ab6b8d`；日志：`4d3a773fc2768950e26cc540e33287fdde408a1c20cf08245bc7a6aec6f1e8df`。本地原始证明仅保存在忽略目录，不提交、不公开。

```text
cargo test -p rustsimhub-storage -p rustsimhub-auth -p rustsimhub-core -p rustsimhub-policy --all-features --all-targets --locked --offline -j3
cargo clippy -p rustsimhub-storage -p rustsimhub-auth -p rustsimhub-core -p rustsimhub-policy --all-features --all-targets --locked --offline -j3 -- -D warnings
cargo fmt --all -- --check
cargo run -p rustsimhub-core --locked --offline -j3 -- --build-info
```

结果：Auth47、Core170、Policy1、Store73，共291项通过、0忽略；四包严格Clippy、全仓fmt和默认Core构建信息通过。新增16项已包含在291中，不能重复相加。

Store新增10项覆盖显式迁移/拒绝旧读者、持久CAS/墓碑、伪造快照/陈旧Owner、角色/密钥/限额拒绝、三种审计事务失败、迁移中断、损坏/未来格式、32授权容量/256审计环、并发CAS、旧备份恢复。Auth新增6项覆盖真实近期Owner、缺/未知注册器、全scope Client/NAS拒绝、近期登录过期不被管理调用刷新、默认6不隐式迁移、重启不复活bearer。

早期编译因SQLite没有u64的FromSql/ToSql实现失败（8处），归档`1c506825103e052babb95eb6ed6da7958c6889796f6b08ff57566624d3212949`、日志`1366aafd623c2241f344fde01af678a40ae7da4aa6d1cdabb553096bcc2ffba9`保留。修正为显式正数i64边界转换后，独立Store10项与严格Clippy先通过，再完成上述291项；没有删断言或关闭lint。

## 下一批与合并前必审

1. A继续完成原安全会话任务；确定profile/编码/转录/设备pin与域绑定后再接真实验证器，不提供生产假后端。
2. 同一Auth中的实际`authorize`与私有能力：绑定realm、Owner/grant代次、设备、会话/连接、动作/请求和期限；撤销/重启/换验证器后的能力失效。
3. 浏览器独立非导出恢复密钥及批准/撤销流程；不能复制普通Owner私钥。
4. D-22固定IPC/实际SO_PEERCRED，D-23现有Wi-Fi Gate/唯一事务与持久幂等/Unknown账本；不另起网络控制器。
5. 生产迁移及回退兼容、有限服务身份/材料由正常PR审核；设备/实际签名部署仍由主线负责。本批不操作NAS/棒子，不改Claude工作区，不接管A/B/C任务。

## 本批任务/看板一致性

最终文档与任务投影归档`45e59d48e0036a839d08d8be45de4b417376ee0523c7112eab297f8a5608266e`、日志`6d20b010fdd191965e548a9fd38ee3cc2828100913975b5612076619dd8d9fc2`：隔离Linux Python24项0跳过、生成一致性通过；Windows Node个人任务/唯一归属/计算/转义检查及git diff检查通过。此后只补本段证明，不重复将291项Rust计入本轮Python测试。看板BLE2-01改为进行中，未增加done，主线Git代码贡献截面仍dfd11e9。
