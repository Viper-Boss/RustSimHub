# 成员D：认证状态读取与正文对象约束

2026-11-01（Asia/Shanghai）。分支 `codex/gtw-recovery-status-20261101`，目标 Viper-Boss/RustSimHub main。起始 #142 最终 `f7743b6b5bcaf6afe8e940ca2772dddab45aed52`，main基线 `ce9ccb8debb61db49860fe795c1f3e471b749f4c`。完整受测代码 **`beef6d6f762af586efb33f28faea31cada5baec8`**、tracked_dirty=false；之后仅本人说明/报告。最终推送 HEAD 在 PR 正文记录，依赖 #139–142 等待精确 HEAD 审核，不自行合并。

[具体接口与边界](../ble/core-recovery-status.md)。原 `recovery_v1` 现在接 ReadStatus 私有能力及严格对象正文，整体3秒截止；固定只读 NM/MM 查询、原 actor/协调锁/SQLite容量与Unknown状态共同给出当前快照，前后核原 Auth/session/grant。属性代理不缓存，末尾实读设备身份/状态/无线和 owner。没有写网络、开启 checkpoint、激活普通蜂窝或将布尔转换成权限；失败/服务变化不能制造 connected/available。

新增 **8项测试**：5项真实私有 D-Bus/Core/Auth/SQLite状态用例、1项四动作数组回归、2项 Storage容量/模式/fence/当前grant用例；只在外部 A verifier 与 NM/MM 端点使用合成数据。整仓 **1229通过/0失败/6继承忽略**，Core库全feature **84/0/0**、默认Core **173/0/0**；整仓严格Clippy/fmt/diff、旧grant+transport库编译通过。43份实际Rust结果直接由未修改的前端契约校验：旧36份operation/lookup/error加7份status，不重复计新增测试或完整阶段。

真实失败记录：在原候选解码实现上加当前新数组断言，Begin/Confirm/Cancel/Query 全部实际接受数组，Rust得到 `[true,true,true,true]`，FAILED/退出101。修正只要求首个非JSON空白字节是对象起始，再走原serde约束；认证原始字节、指纹和所有权保持，不称特权绕过或旧生产漏洞。新Status空结构的 `[]` 拒绝也保留。初版状态测试另有一条预期错误码不正确：持久撤权被既有Auth合并成Storage，所以实际T5返回unknown；测试改为对应有限错误且不得返回状态事实，没有修改Auth的错误合并行为。

```sh
source /workspace/toolchains/activate-rustsimhub.sh
CARGO_INCREMENTAL=0 cargo test --workspace --all-features --locked --offline
CARGO_INCREMENTAL=0 cargo test -p rustsimhub-core --lib --all-features --locked --offline -- --nocapture
node docs/ble/core-recovery-t5-contract-check.mjs work/recovery-status/core.log
CARGO_INCREMENTAL=0 cargo clippy --workspace --all-targets --all-features --locked --offline -- -D warnings
CARGO_INCREMENTAL=0 cargo test -p rustsimhub-core --no-default-features --locked --offline
CARGO_INCREMENTAL=0 cargo check -p rustsimhub-core --lib --features recovery-grants,recovery-transport --locked --offline
cargo fmt --all -- --check
git diff --check
```

最终上述命令均退出0；另单独运行的Storage availability_ 两项通过，包含在整仓中。没有新增ignore、没有借用前一HEAD全绿覆盖本次修正。原始日志仅本地合成材料，精确摘要如下。

| work/recovery-status证据 | SHA256 |
|---|---|
| workspace.log | 98b702ee1aba4631015a7786e503607224dbbdd785cda4fe043f9fd1e604d841 |
| core.log | 77544700e8c265c78bbfc93bd19a941fad7fc479c78514b20940ffdb6e44db12 |
| js-contract.log | 28b0d34da29bcc3d9e0a5f933050c7779fac9cb44548ae71a8b614b396c5cbaa |
| clippy.log | ebfb6aa8eb2a0e68ec15948053d48a19cbc75a56137a3e739ae12ed0d9f9395d |
| default.log | 8bcb959c0cbaab8576d08536918808ab31f7a340940b051bc8f1d80a3df2f8ad |
| old-library.log | c92508a29848820f033cd2ea015c85dde0953c88b07cc967ce34b7c08f610549 |
| arrays-negative.log（真实 FAILED） | d167b3acdc95517359d41b592f750cffbe1997364fc69d4ba7ddcac5dc72aec5 |

无新第三方依赖、DDL、默认schema/启动、正常命令/权限、其他成员/全局台账或Root策略变更。没有系统D-Bus/设备/真实NM/蓝牙、生产数据库/凭据、Root安装、电话/短信/蜂窝、签名部署。AGPL-3.0-only、来源作者保留，Codex辅助，实耗未知。

用户于2026-11-01明确答复“批准这三份方案的离线实施”，对应 #53 BLE角色/Root材料、#56/#132 TURN方案；此批准供下一包继续，不等于生产UID分配/安装或设备验收。A实际标准密码/会话仍属其他成员；本包不接管。后续继续D22可信主机接线、D24低权服务、D25媒体适配及D26实模块故障矩阵。仍 **21/26离线阶段已提交、剩D22–26五步，固定P0–P7约80%**，不靠拆包改变分母/权重。远端CI按最终HEAD单独回读。
