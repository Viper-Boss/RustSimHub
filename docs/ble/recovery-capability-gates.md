# D-22：当前 grant 复核和两阶段私有能力

2026-10-30。接续已合并的 #84/#88；移植旧 #61 的连接/能力语义，
使用当前主线固定 Ed25519 grant，未引入旧 #60/#63 的实验 schema7/8。
这是显式 recovery-grants feature 内的库实现，没有生产 verifier、Core 派发或密码协议。

## 同一个 Auth 中的三个阶段

1. 可信 Core 在实际 SourceGuard 接纳后，把私有 PeerContext 的 nonce 交给
   `open_recovery_connection_from_trusted_transport`。Auth 再生成独立连接随机量，
   token 私有字段、不可反序列化。此方法本身**不核 UID**，不能从网络 payload 调用。
2. 短 Auth 锁内调用 `prepare_recovery_verification`：真实 Store 单 writer 的只读事务
   复核完整 grant、当前设备/realm、唯一 active Owner id/generation；绑定 Auth 实例、
   两个连接 nonce、verifier 配置代次、全部 grant 约束。返回拥有型工作项。
3. 释放 Auth 锁，在有界独立工作线程调用 `work.verify()`；重新取得短锁后调用
   `finish_recovery_verification`。先核连接/配置/五秒期限，再核适配器绑定及当前数据库，
   才生成无 Clone/Serde/Principal/token 转换的 RecoveryCapability。

`RecoveryRequestVerifierV1: Send + Sync` 只供获审 A 适配器注入，无默认实现。
有状态适配器可管理自己的锁，但不得取得全局 Auth 锁或派生返回后仍执行的验证任务。
`VerifiedRecoveryRequest::from_verified_adapter` 是可信 Rust 装配边界，必须在实际认证解密
之后调用；它不是一个密码验证器，不能用解析 authenticated/channel 字段来实现。
默认 None 或移除/替换验证器使旧连接/能力失效。密码 suite/transcript/nonce 域仍未冻结。

## 有界状态与迟到结果

- 原始信封1～2048字节；认证后的 opaque body 最大1024，session id 16～64且非全零，
  request id 为16字节非零内部标识；不是新增 JSON 请求格式。D-23 仍须按 T5 校验业务 UUID。
- 私有能力只表示 ReadStatus/BeginWifi/QueryOperation/ConfirmWifi/CancelWifi/Disconnect
  六动作中的一个，绑定确切正文摘要、请求、会话和 grant，不扩充普通 scope 位。
- 证明从 prepare 到 finish 最多五秒，开始验证前也核对；会话最长300秒，不被新请求延长。
  每连接最多128次工作尝试，失败/未运行就丢弃也消耗预算；成功请求另保留128个防重墓碑。
  改 session id、过期或耗尽不会刷新同连接预算。连接外的速率/cgroup 和实际 A 密码防重
  仍需宿主实现；这些常量不是密码 wire 格式或生产安装许可。
- 一个 Auth 同时最多一个工作项或待消费结果。Permit 最后析构；断连、配置更换不重置
  busy 标记。线程仍运行时，宿主必须保留 work；结果没有消费前也不准下一项。
- 工作 API 本身同步、不启动线程或计时器；Core 可在 #95 的 verify_blocking 内拥有 work。
  timeout 只停止等待，不能把别的线程里仍运行的工作当作已经 Drop。永久卡住继续 busy。
  本批尚未接这个宿主，未实现 A close/清理或真实 GATT 生命周期。

成功能力的 `check_recovery_capability` 每次核动作/正文摘要、当前 Auth/连接/配置/会话/期限，
再在 Store 复核当前完整 grant。`max_trial_ms` 保留于绑定，由未来 D-23 控制实际试连；
这里没有运行 Wi-Fi 或把会话300秒当作40秒试连许可。

## 数据库与兼容

Store 新增只读 `check_recovery_grant(expected)`，在同一 read transaction 读取 schema7、
完整记录、设备、realm 及唯一 active Owner。逐字段必须一致，active 历史管理回执不算证明。
不修改表/版本/receipt，不接受 schema6 或旧实验 schema7；默认生产仍 schema6。
该事务只保证读取时的一致快照，不承诺返回后的 grant 永远有效。D-23 保存/claim 边界还须
复核并协调同 writer 的执行门禁，不能用一次检查替代后续授权或持久执行回执。

能力可供同一请求多个 await gate 复核，本身不是 exactly-once claim；重启后不存活，
不能把内存请求集合当作跨重启执行账本。Unknown、唯一网络锁、USB/电话/WWAN门禁保持未接。
断开实际运输必须由可信宿主同时关闭对应 Auth 连接，单独读到 nonce 不代表仍在线。

## 证据与未完成

新增16项 Auth、3项 Store 测试使用真实 Auth/Store/SQLite/Owner 签名和临时数据库。
只替换外部验证器的密码返回，测试适配器仅在 cfg(test) 内；没有伪造 SourceGuard 正例。
覆盖撤销/历史回执、regrant、断线/配置轮换/跨 Auth 实例、五秒/会话期限、动作/正文、
防重/新 session/128尝试、工作和结果占用、数据库每字段与 Owner/realm/设备变化、真实恢复。
真实阻塞线程的夹具确认 Auth mutex 可用，期间撤销后迟到结果确实拒绝。

A 标准协议/互操作、真正 UID 绑定/运输宿主、GATT、生产密码关闭、D-23 执行及设备均未完成。
本批沿用 release panic=abort，普通 panic hook 保持；同步验证线程不是进程隔离。
没有默认生产调用、新依赖、schema、Root/签名/部署或全局进度变更。
详细命令、精确 HEAD 与数量见[交付报告](../deliveries/member-d-recovery-capability-gates.md)。
