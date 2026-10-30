# 成员D：D23持久登记和一次领取交付

2026-10-31（Asia/Shanghai），Viper-Boss/RustSimHub，分支codex/gtw-recovery-durable-claim-20261031，目标main。起始#136最终`dbaf53f5edd0dbabd47d865429b5c2d656e07e56`，依赖本人#131/#135/#136栈；main仍`80db24e9b101dd3fb4b8c951aa7f1e0e1858e65d`。代码及最终完整正式受测HEAD **`f1e9706ef138c55cdcee3e234bcd3ad6b2186f1c`**，tracked_dirty=false；之后仅本人说明/报告。

[实现、线性化及失败矩阵](../ble/recovery-execution-ledger.md)。实际实现同writer的reserve/current-grant/fence、首次插入私有一次票据、reserved→claimed提交、Auth私有Pending/Claim及只读Lookup。Existing（包括Reserved/Unknown）无票据不能领取；断连/撤权/重开不自动接管或重试。持久行只有原批准字段/指纹/有限状态，不存SSID/口令/原始正文或可反序列化能力。

新增15项真实Storage测试+10项真实Auth测试，均通过。完整整仓 **1024通过/0失败/6继承忽略**；整仓严格Clippy/fmt/diff通过；默认Auth/Storage **87/0/0**；仅旧grant feature Auth/Storage **132/0/0**。实际SQLiteFull回滚、两路writer领取、排队取消/撤权、255+1cancel、原Auth丢弃fence失效、重连旧Reserved只读、完整重开Unknown、备份恢复/Owner恢复/重新授权后旧realm阻断、只读快照字节不变均有真实结果。外部A verifier只有cfg(test)合成适配器，不声称实际安全会话或设备/NM联调。竞争用例仅内部测试复制私有票据来挑战DB防线，生产没有Clone/构造器。

```sh
source /workspace/toolchains/activate-rustsimhub.sh
cargo test -p rustsimhub-storage --features recovery-execution recovery_execution::ledger::tests --locked --offline
cargo test -p rustsimhub-auth --features recovery-execution recovery_execution::tests::ledger --locked --offline
cargo test --workspace --all-features --locked --offline
cargo clippy --workspace --all-targets --all-features --locked --offline -- -D warnings
cargo test -p rustsimhub-auth -p rustsimhub-storage --no-default-features --locked --offline
cargo test -p rustsimhub-auth -p rustsimhub-storage --features recovery-grants --locked --offline
cargo fmt --all -- --check
git diff --check
```

初稿真实负例：重连获得旧Reserved后，原来的“准备对象+reserved行”领取API实际通过，拒绝断言失败。改为必须消费首次插入返回的两层私有票据，Existing只读且旧Pending受原连接fence约束后，同场景通过；保留原失败日志及摘要，不冒称此前主线已上线这个功能。不新增ignore或放宽断言。

| 本地证据 | SHA256 |
|---|---|
| workspace.log | 894f73f9f25c9f82ec62d4acfdc7f7333fa1f24c54fbd1f6c8d5ea59743531b2 |
| clippy.log | a83c412f956ac87de97b70651092229c1aa7dc712d3e00214fcd8b3e505b4b3a |
| default.log | 980cd63e6f1b30cf2825f3ec85f11391fcc9e09b677115ad99c9f3141b1112e5 |
| grants.log | e4ed77d2456cf3d28cccd172bba537a632996a63b7f52279a6c0cc9a26a6538b |
| reconnect-negative.log（本地durable-ledger-reconnect-negative.log） | 8ccd5edd92a9c3aecb96a89b55628a0372c916cd991f084ed4b6e86d672a2813 |

原grant检查只提取共享helper供同一writer事务复用，旧7错误/授权语义保持；无新依赖/Cargo.lock/DDL或默认入口变化。范围为Auth/Storage恢复接点及本人文档，无全局台账/其他成员/Root/UID/系统服务/发行兼容/签名部署或生产库、真实网络/口令、设备/电话/短信/蜂窝操作。源作者和AGPL-3.0-only保留，Codex辅助，实际工时未知。普通suite不自动运行manual kernel UID目标，本批不冒称重跑；既有3359419真实Core/GATT跨UID证据见#114。

仍未接Core唯一Wi-Fi actor、正常Principal/Recovery所有权分支、状态观察及可信终态证据、T5 wire映射/标准A安全会话、40秒业务/checkpoint和设备/Root/发行schema9兼容。claim仅是登记领取令牌，当前API只检查claimed行；不直接调用NM，不能用Begin能力充当Confirm许可。丢失票据的Reserved/Unknown保留阻断，尚无人工对账/归档或无限期可用性保证。

完整阶段新增0；仍 **21/26离线包已提交、剩D22–26五步、固定P0–P7约80%**。接下来继续原actor接点及其逐await/保存门禁，实际外部依赖保持，CI按最终HEAD回读。
