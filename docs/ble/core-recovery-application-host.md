# D22/D26：获核通道的 Core 应用派发

2026-11-01（Asia/Shanghai）。新增 `RecoveryChannel::dispatch_application_v1(auth, expected_grant, optional_sealer)`，把**实际已接受的通道**接到原 Auth 验证、原 Core 恢复应用以及 A 加密回复接点。它是 A 握手完成后的应用路径；没有实现/选择 A 的密码协议，没有默认 verifier/sealer、明文回复替代或自动监听器。A 实际安全会话仍由原负责人交付。

## 实际顺序

1. 缺少显式 `ApplicationReplySealerV1` 时，在读取、验证和应用派发前拒绝。通道必须由原 `SourceGuard` 经实际 SO_PEERCRED 接受，再由同 Core Auth attach，不接载荷的 UID/channel/verified。
2. 从自己的原 descriptor 读取有界帧，走原 owned prepare→专用 verifier worker→同 Auth finish。只接 EncryptedRequest；CloseTransport 只关闭本地会话，不制造 Cancel/rollback 回执；Handshake 是 A 的另一阶段，不作为业务成功处理。
3. 从实际已核请求准备 `RecoveryResponseScope`，然后消费原请求调用原 `recovery_v1`。Scope 仅保留原绑定、session、请求/action、单调期限、原连接 live flag 和原 Store，既不是执行 Claim，也不能转换成正常 Principal/新请求。没有 Clone/Serde/Debug/公开字段或载荷构造器。
4. 原 Core 的 T5 DTO 或有限错误被放入一次拥有的 `SealingRequest`，交显式 A 适配器。A 必须按自身获审会话加密并绑定 Scope 的实际 session/request/action/绑定；`finish_from_trusted_session` 只检查尺寸和关联，**自身不证明加密**。生产模块没有假实现。结果字段保持私有，没有 JSON 成功标志到 SealedReply 的构造器。
5. 复用原专用阻塞适配 worker 执行封装，保留真实 peer/slot 到实际完成。前后核原 Scope，要求 kernel peer nonce 与原 transport 绑定相同；主机再要求回复来自同一个 Scope Arc。只有合规的 `SealedReply` 才写回原帧通道，写回后再复核 Scope，才能返回 Continue(channel)。

Scope 的当前检查直接读原 Store 的 grant，并在前后核原 live flag/单调期限，不拿 Auth mutex。Auth 销毁、verifier 替换、断连/重连、撤权、原 Store 关闭或期限到达都使它失效；新连接不能接管。没有增加 grant、清除 Unknown 或再派发写入的路径。

## 取消、真实 EOF 和有限等待

调用方持有原 `RecoveryDisconnectGuard`，即使通道已转入尚未运行的 Auth worker，取消也会立即使原 session 失效。worker 的真实 socket/slot 仍保留到实际退出，不能因为丢等待方就提前接受另一个操作。成功写回并完成门禁后才解除本次 guard；原通道 Drop 仍负责最终失效。

主机还用原 accepted socket 的安全 dup 创建**只观察关闭**的私有监视器：没有 raw-fd/read/write/Clone 接口，不消费输入。原 framer 独占字节读取；监视器仅在自己的 descriptor 忽略 data readiness，使用 Linux 的 read-closed/error readiness，连仍有未读排队字节的实际 EOF 也会唤醒。关闭事件优先使原 guard 失效并取消等待；迟到 A 工作仍须排空后才释放原资源。这不是 RF 清理或 Wi-Fi 已回滚的证明。

Auth worker 排队/Scope 复核和主机应用等待各有 5 秒界限，原帧 I/O、A worker 原5秒界限保持。原查询3秒、Wi-Fi trial40秒、NM checkpoint45秒及普通控制8秒预算没有延长。写等待结束或断连不等于撤销已提交结果；原 actor 仍按原门禁/有限补偿排空，结果未知只查原 UUID，不自动重发。

## 两条实际失败对照

- 从第一版新主机实现中仅移除调用方 guard，真实不同 UID、原通道、已阻塞工作池的测试得到“等待方已取消但原 Scope 仍有效”，Rust 实际 FAILED。恢复 guard 后相同断言通过；不是旧生产漏洞。
- 新增实际 peer 关闭测试，在原 A worker 挂起且 socket 里还有一帧排队数据时，未加监视器的实现等到2秒断言截止仍未发现 EOF，实际 FAILED。添加真实 FD 关闭监测后相同断言通过，没有放宽超时或改成模拟断连布尔。

失败注入还暴露了隔离夹具 controller 没有 CAP_KILL 时不能直接结束不同 UID 子进程。新夹具用辅助进程先切换到**该已登记子进程的同 UID**再发信号，不增加 capability、不改账户/NSS；失败清理本身也复跑验证。

## 验证与接线边界

5项新增常规 Auth 测试核原 Scope 绑定、断连/verifier/到期、撤权/Store关闭、正文篡改/跨Auth及锁忙时取消。新增显式 `recovery_application_uid` target（`test=false`），普通 suite 不运行 Docker/切 UID。

隔离容器实际以 Core UID1001、BLE UID1002 和6个拒绝 UID 建立2750目录/0660 socket，运行16个来源/应用/取消/超时/EOF/撤权场景，使用真实 Core/Auth/Identity/schema9/私有解码/T5查询；A verifier 和封装结果仅 cfg(test) 合成，假密文字节不称 AEAD 互操作。主机测试派发真实查询和有限错误，没有连接系统 NM 或试连无线网络。旧显式来源13场景独立复跑；手动场景不混入 workspace Rust 测试数。

精确 HEAD、命令、日志摘要及未运行项见[交付报告](../deliveries/member-d-core-recovery-application-host.md)。本库接点尚不等同完整可安装 BLE 服务：A 实际握手/密钥/加密互操作、主线启动与 Root/NSS/发行9兼容和设备验收仍保留；后续继续已经批准的 D24/D25 离线实现。不改其他成员或全局台账，不生产签名/安装/拨号/短信/普通蜂窝。
