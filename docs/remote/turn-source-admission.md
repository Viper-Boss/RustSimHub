# D25：固定 TURN 候选源码准入复核

2026-10-31（Asia/Shanghai）。这是一份接口/依赖待审证据，**没有添加、安装或构建 TURN 依赖，没有实际 TURN/TLS/媒体互操作，也不批准准入**。现有 LAN 媒体、权限、Cargo.lock、Root及设备保持。离线范围仍是任务书D25，不接管A密码或C部署。

## 来源及可核对性

候选固定为 [ystreet/turn-proto d8d127ab](https://github.com/ystreet/turn-proto/tree/d8d127ab1e37aef14f95a9f6b529073c31ee04a5)，没有跟随默认分支或搜索摘要。14份源码/Cargo/许可/锁文件从固定提交读取，逐一按Git blob算法核对上游对象SHA，并记录SHA256/字节/行数，见[来源清单](turn-source-admission.json)。源文件只留本地work，未复制整个第三方实现入库。

旧#56的Viper-Boss提案原commit `31ec213840566c8715b741bcb318943169f8196a` 含全局进度等其他文件；本分支只经file-scoped `git format-patch`/`git am`保留 [原提案](relay-media-proposal.md)，得到 `477c24d6d51c471857e54aaf0131f1dcccf1ccfd`，原作者/日期/内容保留。没有带入旧台账、任务书、HANDOFF或改旧PR。原文#55待合并是历史截面，该媒体修复现已通过#104进入main。

上游源码声明MIT OR Apache-2.0，两份实际许可文本均核对；不据此宣称全部传递依赖许可已清。上游workspace/turn-client-proto/turn-types版本0.7.3，TLS子包turn-client-rustls为0.1.2。根workspace依赖要求0.7.1搭配path，不能仅按版本范围证明下游引用了同一提交。源码锁文件含209个**整个上游workspace**包，含server/dev/OpenSSL；它不是拟用TLS客户端的生产依赖图。锁中rustls0.23.45、stun-proto/stun-types2.0.3，manifest的stun要求2.0.1并非实际锁定版本。

## 逐接点复核

以下都链接到同一固定提交；“有源码路径”不等于本项目运行通过。

| 必需接点 | 核对的真实API/源码 | 本项目需补的边界 |
|---|---|---|
| TLS构造和主机名 | [rustls lib L80](https://github.com/ystreet/turn-proto/blob/d8d127ab1e37aef14f95a9f6b529073c31ee04a5/turn-client-rustls/src/lib.rs#L80) allocate接收ServerName及Arc<ClientConfig> | 由受信资源配置创建；拒绝客户端URL/CA/忽略证书。该库不自行选密码provider |
| provider及TLS版本 | [根Cargo](https://github.com/ystreet/turn-proto/blob/d8d127ab1e37aef14f95a9f6b529073c31ee04a5/Cargo.toml) rustls default-features=false、std/tls12；[TLS Cargo](https://github.com/ystreet/turn-proto/blob/d8d127ab1e37aef14f95a9f6b529073c31ee04a5/turn-client-rustls/Cargo.toml) ring只在dev | 正式provider、TLS策略、活跃依赖/许可/漏洞和ARM须单独批准/验证，不能复制dev默认当产品决定 |
| allocation配置 | [api L143](https://github.com/ystreet/turn-proto/blob/d8d127ab1e37aef14f95a9f6b529073c31ee04a5/turn-client-proto/src/api.rs#L143) TurnConfig，默认IPv4/UDP、完整性Sha1、匿名名Auto | 冻结真实算法协商/降级兼容、单allocation、地址族和凭据边界；控制连接TCP/TLS不等于relay分配为TCP |
| 401/438及完整性 | [protocol L611](https://github.com/ystreet/turn-proto/blob/d8d127ab1e37aef14f95a9f6b529073c31ee04a5/turn-client-proto/src/protocol.rs#L611) 委托LongTermClientAuth.validate_incoming_message，Validated/ResendRequest/拒绝；stale nonce上限8 | 仍须实际错口令/错realm/错完整性/重复nonce/伪响应试验；这次没审核或执行stun-proto本体，不能宣称密码审计完成 |
| 分配/权限/channel证据 | [api L445](https://github.com/ystreet/turn-proto/blob/d8d127ab1e37aef14f95a9f6b529073c31ee04a5/turn-client-proto/src/api.rs#L445) AllocationCreated/PermissionCreated/ChannelCreated及失败事件；relayed_addresses/have_permission | 从实际已验证事件建立私有allocation代次；不从SDP typ relay/网页布尔制造能力 |
| I/O及收包 | [TLS poll_transmit L211](https://github.com/ystreet/turn-proto/blob/d8d127ab1e37aef14f95a9f6b529073c31ee04a5/turn-client-rustls/src/lib.rs#L211)、[recv L447](https://github.com/ystreet/turn-proto/blob/d8d127ab1e37aef14f95a9f6b529073c31ee04a5/turn-client-rustls/src/lib.rs#L447)、poll_recv | host实际驱动TCP读写、partial-write/EOF/backpressure/定时器，按真实peer和五元组送入str0m；不是库替你打开socket |
| 对端及返回数据 | TurnRecvRet PeerData/PeerIcmp/Ignored/Handled，TurnPeerData带transport/peer，poll返回AllocateTcpSocket/TcpClose等 | 仅本会话获准relay地址，拒绝任意代理目标/额外TCP relay能力；不把Ignored转发到任意网络 |
| 续租及期限 | [protocol L50](https://github.com/ystreet/turn-proto/blob/d8d127ab1e37aef14f95a9f6b529073c31ee04a5/turn-client-proto/src/protocol.rs#L50) permission300s/channel600s、STUN/TCP39.5s；[TLS poll L192](https://github.com/ystreet/turn-proto/blob/d8d127ab1e37aef14f95a9f6b529073c31ee04a5/turn-client-rustls/src/lib.rs#L192) handshake WaitUntil60s | 外层独立有限准备/关闭截止，不延长Core票据5s、Permit心跳1s或媒体排队60ms；allocation期限不能作业务租约 |
| 释放 | [delete L2071](https://github.com/ystreet/turn-proto/blob/d8d127ab1e37aef14f95a9f6b529073c31ee04a5/turn-client-proto/src/protocol.rs#L2071) 构造Refresh lifetime0、标本地expired并排队；TLS delete只转交并刷新发送队列 | delete返回Ok只证明已请求。TurnEvent无独立DeleteAck，Closed也可能来自认证错误/超时；不能据此写rollback/release_verified=true，缺证据保持Unknown或提受审证据接口 |

## 已运行的最小失败样例

[turn-types L70](https://github.com/ystreet/turn-proto/blob/d8d127ab1e37aef14f95a9f6b529073c31ee04a5/turn-types/src/lib.rs#L70) 的凭据结构派生Debug，password为String。新增[摘录复现器](turn-debug-excerpt-repro.py)先验证**整个源文件固定SHA256**，只抽取原始derive/struct声明并保留原许可头，在临时目录使用rustc/std和合成值实际编译运行；断言Debug文本含合成密码。仅输出布尔证据，不打印凭据/Debug正文，不拉取或链接第三方依赖。篡改一字节的源文件也实际测试被拒绝，发生在抽取/编译前。

```sh
source /workspace/toolchains/activate-rustsimhub.sh
# 参数是已取得固定提交的turn-types/src/lib.rs；可按清单核对blob和SHA256。
python3 -B docs/remote/turn-debug-excerpt-repro.py --types-source work/turn-review/turn-types__src__lib.rs
```

这证明该**原始类型摘录**的Debug会包含合成密码，不是完整候选库、TLS或生产泄露试验。分配入口的tracing已skip(config)；本包没有发现或声称现产品正记录这些凭据，现产品尚未依赖该库。正式适配必须避免Debug日志/DTO透传、提供自有脱敏类型并审查tracing features与错误路径。

## 待验证的资源及故障接点

TLS封装源码有ClientConnection::new(...).unwrap()及writer.write_all(...).unwrap()，多socket/缓冲Vec，recv每次只读至多2048字节，而poll_recv转交协议层。这些是需要实测的配置错误、背压、粘包/分片和内部明文剩余数据接点，**本包未证明可触发panic/卡死/漏洞**。须用真实候选和私有TLS TURN验证，并决定有限进程失败策略；项目release panic=abort，不能用catch_unwind宣传线程隔离。

准入尚缺：锁定活跃生产features/依赖及全许可/漏洞检查；批准provider/完整性与资源/证据接口；实际TLS错CA/主机名、分配/nonce/重放、permission/channel/refresh/delete、partial-I/O/缓冲/耗尽和服务器退出；ARM构建、原128M/12任务/5秒停机预算；str0m真实selected relay/DTLS指纹/Opus双向数据及Core Permit撤权。只在批准后的独立适配包接入，不改变LAN/USB/正常通话权限、不创建第二Core权威。

仍21/26离线包已提交、剩5步，固定P0–P7约80%。14文件核对、源码摘录执行和修改源拒绝都不算实际TURN/媒体完成，不新增步骤或改权重。无服务器/提供商/设备访问、主机网络权限或Root角色修改、真实数据/电话/短信/蜂窝、签名部署。
