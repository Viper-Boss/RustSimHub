# 成员D：D23原trial持久观察交付

2026-10-31（Asia/Shanghai）。分支codex/gtw-recovery-observations-20261031，目标Viper-Boss/RustSimHub main。起始#138最终`4c729b3b20446bb67f4c597b5f97e93f6be9692f`；本包实现`c2d035994215c342d354de6a2c4a4ce346224ede`，随后正常合并最新main `342adaa2fee6f3ea36a82f0145dc7931e12e36de`（无人工冲突修正），保留其作者/来源。最终完整受测代码HEAD **`5799a95a45cb0692a2062f5ddd618c3546cfea85`**，tracked_dirty=false；之后仅本人说明/报告。仍依赖本人#131/#136～138待审部分；不越过逐HEAD审核。

[进度、两行事务、权限与事实记账边界](../ble/recovery-execution-observations.md)。新增实际Auth→同SQLite writer的starting/connecting/ready观察、原Begin+独立Confirm同事务saving及saved/rolled_back/unknown事实记录。活跃门禁跟随合法进度，初始派生单独严格要求claimed，防止重开40秒窗口。事实记账在撤权/断连/到期后仍能报告实际补偿，但不签发能力或清Unknown。

新增 **10项Storage+8项Auth测试**通过；外部A verifier与backend观察输入为合成，不冒称真实A密码/NM/设备验收。完整新main组合整仓 **1180通过/0失败/6继承忽略**，严格整仓Clippy/fmt/diff通过；默认Auth/Storage87通过，旧grant-only Auth/Storage132通过。相较旧1035的其余增量来自main已经合并的其他模块，不算本包新增。

```sh
source /workspace/toolchains/activate-rustsimhub.sh
cargo test -p rustsimhub-storage --features recovery-execution recovery_execution::ledger::observations --locked --offline
cargo test -p rustsimhub-auth --features recovery-execution recovery_execution::tests::trial --locked --offline
cargo test -p rustsimhub-auth -p rustsimhub-storage --features recovery-execution --locked --offline
cargo test --workspace --all-features --locked --offline
cargo clippy --workspace --all-targets --all-features --locked --offline -- -D warnings
cargo test -p rustsimhub-auth -p rustsimhub-storage --no-default-features --locked --offline
cargo test -p rustsimhub-auth -p rustsimhub-storage --features recovery-grants --locked --offline
cargo fmt --all -- --check
git diff --check
```

真实SQLite TEMP trigger令第二张Confirm UPDATE ABORT，分别验证saving与saved事务第一张也回滚、两张原状态/证据不变；移除临时触发器后生产函数通过。两个不同终态的真实writer竞争只提交一种结果；独立Confirm/Cancel动作、非法进度、已progress禁止新派生、断连/撤权/到期后记账但拒绝执行、其他Auth/Claim/指纹、Unknown/重开迟到拒绝、备份恢复saved历史均有实际模块测试。没有新ignore、生产fault开关、替代状态机、延长期限或前端布尔当证据。

| 本地日志 | SHA256 |
|---|---|
| workspace.log | 57054d61bdd5526d698581d9215b5edf08518319d9f9f5cb478a258d3c584e73 |
| clippy.log | d30fd61df80679731c304b6433153a6c37d1cdba6102c88d2d66f2641a8347a3 |
| default.log | b7f1f0933468a3562fdacc6718bbbbc1769706b5c384e4382e21886ee2b3d9c5 |
| grants.log | 8160dafb5968843bfa4c9c004c31574cc2ea7724946569ecb77d799611c41895 |

新增实现范围仅Auth/Storage恢复接点与本人文档，无新DDL/依赖版本/默认入口；Cargo.lock只正常接收main已有依赖。其他成员/全局进度改动均由main已有提交带入，本包不改写它们。AGPL-3.0-only，保留来源作者，Codex辅助，工时未知。无生产库、设备/NM/Wi-Fi/网络、电话/短信/蜂窝、Root/UID/安装/签名动作；没有冒称本批重跑manual kernel UID目标。

新增接口只接可信原backend事实，本身不验证硬件完成；尚无Core原actor派发、同一认证正文语义解析、Normal/Recovery所有权分支、USB恢复分支或T5映射。A实际标准会话、Root/真实UID/发行schema9兼容及设备联验保留。完整阶段新增0：仍 **21/26已提交、剩D22～26五步，固定P0–P7约80%**。下一批处理Core同正文参数绑定；CI按最终HEAD独立回读。
