# 成员D：D23原trial与独立控制授权

2026-10-31（Asia/Shanghai），Viper-Boss/RustSimHub，分支codex/gtw-recovery-trial-authority-20261031，目标main。起始#137最终`7b682d4f7f31a161957730115dcbb9aafbee07ec`，依赖本人#131/#135/#136/#137栈；main仍`80db24e9b101dd3fb4b8c951aa7f1e0e1858e65d`。代码及最终完整受测HEAD **`36a12ce4061592eb912dc6f47004efc99560a65c`**，tracked_dirty=false；之后仅本人说明/报告。

[实际Auth类型、期限、门禁与Core保留接点](../ble/recovery-trial-authority.md)。消费真实Begin私有Claim派生原trial授权，固定40秒/grant/原能力最小期限；Confirm/Cancel各自必须有同原trial、完整绑定和相同Auth/连接/session的独立已领取能力，不能互换动作或通过重连延长/接管。没有创建或转换正常Principal，也没有另一个Wi-Fi事务。

新增11项真实Auth/Identity/签名Owner/Store9测试通过；只有外部A verifier是cfg(test)脚本适配器，同一session提供独立Begin/Confirm/Cancel。60秒能力下实际夹验40秒上限、短能力/12340毫秒grant缩短、确认前后期限精确相等；动作混淆、跨Auth/grant、断连重连、verifier变更/撤权、确定性过期拒绝。没有sleep、延长租约、改生产时钟或新增ignore。最初两个期限测试把证明准备前的时间误当派生起点，已改为在真实派生前后夹验并直接比较原能力期限；没有因测试偏差修改运行期限算法。

```sh
source /workspace/toolchains/activate-rustsimhub.sh
cargo test -p rustsimhub-auth --features recovery-execution recovery_execution::tests::trial --locked --offline
cargo test --workspace --all-features --locked --offline
cargo clippy --workspace --all-targets --all-features --locked --offline -- -D warnings
cargo test -p rustsimhub-auth -p rustsimhub-storage --no-default-features --locked --offline
cargo test -p rustsimhub-auth -p rustsimhub-storage --features recovery-grants --locked --offline
cargo fmt --all -- --check
git diff --check
```

最终实际整仓 **1035通过/0失败/6继承忽略**，整仓严格Clippy/fmt/diff通过；默认Auth/Storage87通过，旧grant-only Auth/Storage132通过。源范围只有Auth独立恢复接口及本人文档，未改依赖/Cargo.lock、Storage DDL/默认入口、正常Core业务、Root/生产UID/服务/发行兼容、全局台账或其他成员。没有生产库、实际A安全会话、网络/设备/NM/Wi-Fi、电话/短信/蜂窝或签名部署操作。AGPL-3.0-only、来源作者保留，Codex辅助，实际工时未知。

| 本地日志 | SHA256 |
|---|---|
| workspace.log | e8b78810afabbf970c8b0956f253e5d43a9113bcfa31636d6acd22c9e790e9de |
| clippy.log | 9b622ad0641fab659463acc41428818a7a06893729d6b2c728fbf3404f251b0b |
| default.log | 5d2b0e06aadc5c1616b47b7312ef0c67cf4895bda34b52ef51e58773d4b24ff7 |
| grants.log | 043764644da1e89d00ab59bf0f44fc3bc3a530069664489c9c681bf5615aac61 |

这是可供原Core actor消费的授权组件，尚无Core派发/Normal与Recovery所有权分支、可信状态观察与终态证据、T5 wire映射/认证正文语义解析或USB Recovery分支；当前Storage门禁只接claimed状态。不能因此宣称实际开始/确认保存/回滚或可安装端到端恢复。A实际安全会话及正文接口、Root/实际UID/发行schema9兼容、设备验收保留；#103已有流程批准不再虚报首次批准缺失，但新增实现须逐HEAD审查。

普通suite不包含manual kernel UID试验，本批未冒称重跑；上次3359419真实Core/GATT跨UID结果在#114。完整阶段新增0，仍 **21/26离线包已提交、剩D22–26五步，固定P0–P7约80%**。CI按最终HEAD回读，账单导致的启动前失败不计测试通过。
