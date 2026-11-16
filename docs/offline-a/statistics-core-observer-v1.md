# Core统计交易观察连接候选v1

本组件消费[Go有限交易契约](statistics-rpc-candidate-v1.md)，默认关闭，未启用生产签到。
它是原Controller上的有限观察操作，不是完整CheckInTransport。

## 唯一生产入口

显式feature `usage-root-transport-candidate`（依赖原签名feature）导出：

```rust,ignore
Controller::statistics_transaction(
    &self,
    signer: &SubscriptionSigner,
    store: &FileStore,
    lease: &StatisticsLease,
    call: TransactionCall<'_>,
    budget: &AttemptBudget,
) -> Result<TransactionConclusion>
```

- `Preflight`无任意方法或参数，只有所有窗口无Pending才允许。
- `Submit(&PreparedSubscription)`只接受本机固定用途纯签名结果；原period/window/digest必须已持久化。
- `Query { intent, digest }`只允许匹配已持久化的原交易，不能以新意图代替原Unknown。
- 结论只有Nonce、Accepted、Unknown，没有Confirmed；调用者不得将Accepted清Pending或记确认。

调用要求原真实Auth同一Arc、当前Owner/realm/原有限权限、独立参与许可和身份、已有Ready Statistics引用、
原Root策略交易版本1+status版本1及相同统计公钥。没有引用/能力或授权时明确失败，不创建后台引擎。
Statistics引用代次跨OFF/再获取变化，旧操作不能借新引用继续；Remote/Manual语义不变。

## 有界I/O及撤权

固定原Root Unix socket和实际peer检查，不接受URL、路径、UID、shell或任意交易透传。
保留RSCN原注册和原Root回包，另使用RSTX v1长度有界正文，绑定原generation/deadline/nonce/policy/PID。
正文绑定原lease ID、真实当前参与代次和独立统计公钥；submit/query额外绑定原意图和真实交易摘要。
正文<=2048字节、交易<=768字节；重复/未知/null/跨意图/额外尾帧拒绝。字段以实际Rust/Go类型为准。

总期限不超过原AttemptBudget、Root原期限和本次2秒上限；每次读等待<=25ms后重查原取消/授权/许可/引用。
在最终一次非阻塞本地写期间持原Auth/许可和精确引用锁；不持锁等待socket、RPC或服务启动。
单独新建观察FD，成功必须单次EOF；读前不半关闭写端。退出只关该FD，不stop/release共享引擎。

## 保留接口和接入边界

原CheckInTransport、CheckInMachine、FileStore、许可Ledger、有限Owner权限均不更换。
当前方法不实现引用获取/释放、启动、纯prepare调度或重启恢复；这些仍须原Machine适配器完成。
也没有新的永久能力、自动续权或reporting_available=true。原300秒上限保持。
集成不得用另造预算、AlwaysEnabled或可注入ready/proof代替真实授权与原内核证据。

验证和完整来源见[本轮交付](../deliveries/OFFLINE-A-CORE-OBSERVER-20261116.md)。
当前socketpair和真实Auth/许可组件测试不是安装Root/SDK全链路测试；B应从PR完整HEAD复建并保留这个缺口。
原AGPL-3.0-only及PR265/266固定SDK来源和许可证保持，无新增依赖或上游版本变化。
