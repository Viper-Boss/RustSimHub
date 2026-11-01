# D25：固定 TURN/TLS 与强制中继 RTC 适配

2026-11-01开始。用户批准 #56/#132 离线实施后，在既有 native audio crate 增加显式 `turn-relay` feature。已实现实际 TURN/TLS/ICE/DTLS/Opus SDK，并在私有服务运行；**尚未接正常 Core/Worker v2 与 Root 服务登记，不是生产准入或完整远程通话交付**。默认 LAN/worker v1、Core 唯一权限/票据/租约保持。

`remote::Registration` 仅供已受信的安装登记层用 Rust 构造，没有 Deserialize、Debug、公开凭据字段或网络 URL。固定 IPv4 服务地址、正常 CA/主机名/有效期验证、有限凭据期限；拒绝私网、loopback、link-local、CGN、组播、文档/保留网段、已列出的元数据地址与任意名称格式。没有 DNS 重试、代理或忽略证书分支。最初版本只允许 peer 使用**本次实际已认证 Allocate 返回的相同 relay IP**，不把公网地址或 SDP `typ relay` 当登记证明；多 relay IP/provider 不自动扩展。回调没有业务权限。

实际 TCP peer 地址固定；独立原 fd 副本仅用于关闭围栏。TLS 与 Allocate 共用绝对5秒准备期限；permission/channel 准备5秒，单次控制 I/O 500ms，STUN帧8192字节、channel数据1500字节、每轮最多32个协议事件/发送。只将完整有界帧交给固定 Sans-IO 库，取消、坏头、EOF、过期或无法确认的 partial I/O 均关闭原 fd，不能继续消费残帧或重连重放。原 Core 票据5秒、Permit心跳1秒、音频60ms年龄/队列没有延长。

`turn-client-proto` 固定 Git `d8d127ab1e37aef14f95a9f6b529073c31ee04a5`；stun-proto/types 固定已复核2.0.3，tokio-rustls0.26.6显式ring provider、正常TLS1.2/1.3验证。TURN长期完整性由库处理（支持SHA1/SHA256协商），未自行写密码算法。没有采用带 unwrap 的上游TLS封装，也没有复制其测试中忽略证书的 verifier。原 str0m0.24.1 DTLS provider保持。

私有 Allocation 只从实际认证的协议事件产生，不能反序列化；同一连接只绑定一个 peer，实际 permission/channel 与分配地址均再次核对。已知同peer在 ChannelBind 完成前送来的数据丢弃，不授予ready、不发布给RTC；不同peer、空/超限数据仍拒绝。只有原 fd 和原分配继续有效，实际 ICE selected pair 的 local/remote 与该分配和固定peer相符，实际 DTLS 证书又匹配正常受信信令提供的指纹，才产生私有字段的 `VerifiedRemotePeer`。它只证明当前传输事实，不是业务执行许可。

复用原 `rtc::Session`、Codec和Permit：提取发送出口与帧入队供新适配器使用，LAN仍用原UDP出口、host/private-IP校验和原配置；remote使用实际relay候选与100ms真实stats事件。新会话在实际DTLS和路径成立后，仍须持有从正常Core消费来的原Permit，指纹与Core票据一致才能创建Opus状态、编码/收取语音。每帧沿用原Permit检查；有界密文待发队列也核60ms期限，过期/取消关闭而不重发。撤权后不因TURN寿命、ICE连通或其他会话的Permit恢复声音。

关闭先清理Codec/帧，再尝试最多1秒lifetime=0和TLS关闭。上游没有唯一DeleteAck证据；SDK不返回 `release_verified=true`，干净TLS关闭也不当远端释放证明。强制退出/取消后只有本地fd关闭事实，服务器仍依其TTL回收。

三条真实候选失败及保持断言的修正均有日志：误用elapsed API导致认证挑战后不发请求；permission/channel未完成时正常早到数据误触发Peer错误；只检查公网地址时实际允许为其他公网IP创建permission（没有向该IP发数据，容器无外网）。这些是新离线SDK候选的问题，不冒称旧生产漏洞。期限、实际授权和失败断言未放宽。

依赖与许可见 [实际闭包](turn-native-dependencies.json)：native lock189包、Linux x86_64 normal/build活跃149包，145个registry版本实际查OSV，全部有声明许可；它不是上游209包workspace。保留原版本，仅native Tokio1.53.2更新1.53.3修复RUSTSEC-2026-0386；新增26包。固定Git源码仍是MIT OR Apache-2.0/原作者Matthew Waters，按MIT选项保留版权与许可；ISC/BSD/Unicode等复合许可及分发NOTICE仍须由正式产物保留。

**生产准入仍未完成**：原DTLS依赖链 crypto-bigint0.5.5 命中 RUSTSEC-2026-0387（秘密整数运算的时序问题）；修复>=0.7.6不能直接替换p256/p384/dimpl要求的^0.5接口。本包不私改密码实现或把换版本当已验证修复。新TLS使用ring，旧DTLS链风险仍明确保留；Git精确提交未被OSV索引不等于无漏洞。正式兼容升级/密码审查、Root登记/实际Core接线及设备验收是后续门槛。

离线重跑入口为 `native/audio-alsa/tests/run_private_turn.py`（普通cargo test不启动该显式test=false目标），pinned Dockerfile在同目录fixtures。实际客户端、正常TLS、真实Coturn4.6.1-2、UDP转发、nonce/refresh与str0m/Opus都运行；测试CA/密钥/凭据与声音是合成且仅在隔离环境，loopback构造器/协议时间推进只有cfg(test)。nonce、TLS、凭据、Core与Frame时间仍用真实独立时钟，协议定时测试以小步推进避免跳过原刷新窗口。

正常调用方的SO_PEERCRED/Worker v2、Root/C登记装配、真实Core业务状态到远程信令、真实PCM、运营商/外网TURN/两端听音、目标板完整cgroup负载与部署尚未验证。不得仅凭本SDK测试扣除完整D25、生产Crypto准入或设备验收。
