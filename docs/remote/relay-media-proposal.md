# D-25a：远程媒体接线与 TURN 依赖提案

日期：2026-10-29。状态：**待独立接口/依赖审查，未实现、未安装、未验收**。

本提案基于 `9f4ff04373846bd1555b3f471bc92072eaf5c412`，对应 T6 的 `RemoteCallMediaPortV1`、D-25（REMOTE-03/05）。不扣减 D-25 或 D-26，不接管 A 的安全会话、C 的中继部署材料、B 的页面，也不并行操作 Claude 的设备。本次只查源码和上游固定版本，没有拨号、连接 TURN、下载构建新依赖或更改生产协议。

## 1. 现有实现可复用到哪里

| 实际源码 | 已有能力 | 不能据此宣称的能力 |
|---|---|---|
| `native/audio-alsa/src/codec.rs` | libopus、16 kHz 单声道 PCM、每帧320采样/20 ms，Opus包不超过400字节；RTP时钟48 kHz、每帧步长960 | 不需要重写编解码；这些不是远程链路的延迟实测 |
| `native/audio-alsa/src/rtc.rs` | str0m ICE/DTLS/SRTP，真实连接事件后核对 SHA-256 DTLS 指纹，媒体发送/接收检查 Permit | 当前仅局域网 IPv4、host候选、UDP、固定peer IP；没有TURN分配或TLS到TURN服务器 |
| `crates/media/src/worker.rs` | v1有限本地Unix协议：Prepare/Inspect/Authorize/Stop；StopAll仅用于本地Edge生命周期 | 不能将StopAll、Core票据或整个Worker控制协议直接暴露给NKN/公网 |
| `crates/core/src/media_ipc.rs`、`crates/media/src/authorization.rs` | Core唯一授权、一次性票据、真实本地连接和call/line/principal/peer/租约绑定 | 客户端传来的Context、`active=true`或NKN成员资格不是业务授权 |
| `firmware/system/overlay/etc/systemd/system/rustsimhub-media.service` | Media低权账户、特定ALSA设备、AF_UNIX/AF_INET、128M/12任务/停机5秒 | 未允许IPv6；现有网络地址族限制也不是TURN目的地址白名单 |

当前 `MAX_AGE_MS=60` 是本地排队年龄限制，RTC从本地 `network_time` 计算年龄，**不是60 ms端到端WAN保证**。队列4帧、票据5秒、Core镜像心跳1秒及RPC超时保持现值，不为WAN放宽。

依赖现状：native已固定 `alsa=0.12.1`、`opus=0.4.0`、`str0m=0.24.1`（rust-crypto）。str0m是Sans-IO，调用方负责候选获取和实际I/O；仅调用 `Candidate::relayed` 并不执行TURN Allocate、鉴权、权限创建或续租。

相关故障前置另在 [PR #55](https://github.com/Viper-Boss/RustSimHub/pull/55)：本地媒体IPC请求编号耗尽及进行中future取消时立即撤销Permit。该PR是独立修复，尚未合入本提案基线；后续实现前应正常集成，不能把提案当作已包含修复。

## 2. 需要先审的接口变化

现有Worker v1的LAN限制保持。远程另用显式版本的有限本地命令；下表是**接口草案名称**，不是现有Rust类型或已发布JSON。

| 拟议输入/结果 | 可信来源与限制 |
|---|---|
| `RemotePrepareV2(session, relay_handle, offer)` | 仅真实SO_PEERCRED核验后的Edge到Media本地连接；session由既有业务控制器关联当前操作；relay_handle只能引用Root安装时登记的服务项，不能是客户端URL/IP/路径 |
| `RelayAllocation` | 由实际TURN客户端成功分配产生的进程内对象，包含本次会话、已核验服务器连接、实际relay地址、有效期与generation；不能通过反序列化JSON创建“已分配”能力 |
| `VerifiedRemotePeer` | 实际ICE选中受限relay路径且DTLS指纹校验成功后产生；指纹来自已认证正常信令并与实际DTLS证书一致，不信任offer里的自报字段 |
| 既有 `IssueMedia` / `CheckMediaLease` | Core重新核验正常身份、CallMedia/CallControl、当前活动呼叫、线路和租约，并绑定实际peer_key；复用既有Authority/Ticket/Permit，不新增并行授权库 |
| `RemoteStop(session)` | 只清理本调用方拥有的对应会话；不得映射为StopAll、任意挂断、命令执行或驱动控制 |

是否采用独立socket或严格拒绝未知版本的同socket，由接口PR裁决；两者均必须在接入点明确核验角色和版本。旧Worker收到新命令返回Unavailable/不支持，**不得退回v1、host候选或更宽权限**。v1已有测试保持。

新Worker消息上限沿用24576字节、SDP16384字节，不把TURN凭据塞进客户端可复制回执。未知字段、重复关键字段、非法session、错版本和错UID全部拒绝。正式Rust类型、wire字段、兼容测试及日志脱敏须随获审后的实现PR提交，本文不是冻结线协议。

## 3. 正常业务授权到传声的顺序

1. NKN/Relay仅传输既有正常登录和业务请求；不制造Owner，不消费BLE恢复权作为CallMedia权，不信任Origin或IP提升权限。`PrepareCall/SubmitCall/CallOperation/CancelPreparedCall/RenewCall`继续由Core唯一业务状态处理。
2. 正常身份已核验、当前call/line/lease成立后，Edge才可申请受限媒体准备。TURN短期凭据只能为该正常会话签发，附本地generation和截止时间。发凭据不等于授权开PCM，也不等于已存在TURN分配。
3. Media用Root登记项的主机、端口、证书验证策略及凭据，在有界期限内实际Allocate并CreatePermission/ChannelBind。协议鉴权、nonce、响应完整性和TLS由经过审核的库处理，不自行写密码算法或接受 `verified=true`。
4. 所选ICE路径必须对应真实有效allocation。提取已选候选对和generation作证据；仅SDP `typ relay`、浏览器 `iceTransportPolicy=relay` 或页面标签都不足以证明强制中继。
5. 在真实DTLS连接并核对正常信令的指纹后，Core按现有绑定签发一次性媒体票据；票据仅通过本地已核验IPC交付Media。此时再Redeem、取得Permit、打开PCM。准备时允许有限ICE/DTLS控制包，授权前不得编码/发送/播放语音。
6. 每次捕获、编码、发送、解码、播放仍检查Permit；Core boot、backend、line、call revision、principal generation、lease generation、peer改变或权限撤销时立即停止。远程网络不得刷新Core租约或改写有效期。
7. 网络、TLS、TURN分配/权限续期、指纹或CoreIPC失败：先撤销并停止PCM，再关闭peer和本次allocation，尝试有界deallocate。无法确认释放时保留Unknown，等待服务器TTL回收；不宣称释放成功，不自动重拨。

Core票据5秒是消费窗口，Permit最多1秒心跳；TURN allocation寿命是服务器控制的另一层期限，两者不混用。即使TURN服务器给了更长TTL，Core撤权仍立即禁止传声。凭据用户名、密码、票据、ICE密码、SDP、音频内容不写公开日志。

## 4. TURN与网络边界草案

- 首轮只提议IPv4 TURN-over-TLS/TCP控制连接、UDP relay allocation和单路音频。TLS是设备到TURN服务器，终端间媒体仍由DTLS/SRTP保护。TURN/TCP的队头阻塞可能增加延迟，不能承诺等同LAN。
- 仅Root预登记服务和端口；证书正常CA链、主机名、有效期核验，私有测试CA只用于隔离测试。禁用“忽略证书”、任意代理URL、自动跟随重定向与未经审核的替代DNS/目的地址。解析结果每次与登记项和地址策略核对，不能把冻结一次DNS当永久可信。
- 明确拒绝无关局域网、loopback、link-local、组播、未指定地址、元数据地址等目标。私有TURN测试使用**单独测试构建/夹具明确登记的loopback目标**，不把测试例外编入生产公网策略。
- TURN permission的peer目标必须来自本会话已认证、获准的relay候选；不能把任意对端IP当代理目标。客户端和服务器双向白名单/限额均需验证，不能只在浏览器设置。
- 初始预算提议每通话一个有效allocation、一个peer、一次握手，现有单活动媒体会话不变；拨号和媒体重试仍服从业务操作回执，不自动重发Unknown。新的握手总时限、TLS缓存和TURN控制帧上限应由依赖API及隔离实测确定，再冻结进实现PR；不以“没有限值”上线。
- 双TURN、IPv6、UDP直连优化、ICE restart和网络切换均不默默回退；后续另审。切换若不能保持原绑定，关闭当前媒体并报告需重新授权，不能复用旧票据给新peer。强制relay失败不切回host。
- Media服务当前只限制地址族，生产出口还需要有限Root策略和C的 `RelayTemplateV1`。若需要新网络权限、CA挂载或service材料，另列差异审查，不暗改现有Root安装器。

## 5. 新依赖候选与准入清单

候选是 **ystreet/turn-proto**，仅作为审查输入；未添加到Cargo.toml/lock，未确认crates.io发布状态，也未声称经过安全审计或ARM验证。

固定源码提交：[`d8d127ab1e37aef14f95a9f6b529073c31ee04a5`](https://github.com/ystreet/turn-proto/tree/d8d127ab1e37aef14f95a9f6b529073c31ee04a5)。读取固定提交的Cargo声明：workspace版本0.7.3，`turn-client-proto`继承该版本，`turn-client-rustls`自身版本0.1.2；edition2021、MSRV1.80。不要将TLS子包版本误写为0.7.3，也不要使用旧搜索摘要的0.7.2。

源码声明 `MIT OR Apache-2.0`，已打开固定提交的 [MIT](https://github.com/ystreet/turn-proto/blob/d8d127ab1e37aef14f95a9f6b529073c31ee04a5/LICENSES/MIT.txt) 和 [Apache-2.0](https://github.com/ystreet/turn-proto/blob/d8d127ab1e37aef14f95a9f6b529073c31ee04a5/LICENSES/Apache-2.0.txt) 文本。选择具体分发许可、保留版权/NOTICE及整个传递依赖许可清单仍须在准入PR完成；本文没有复制上游实现。

实际准入必须逐项给出证据：

1. API源码审查：真实Allocate、401/438挑战、响应认证、Permission/ChannelBind、Refresh、释放、TCP分帧与填充、TLS验证及失败取消；不只核对crate名称。
2. 生成固定lock并审查全部新增传递依赖、features、密码provider、重复TLS/加密实现、许可证和已知漏洞；根Rust1.98及ARM目标实际构建。禁止仅因MSRV小于1.98就判兼容。
3. 用真实client和私有TLS TURN服务运行下节测试；测试必须显示实际allocation和转发流量，不得用模拟 `Allocate=Ok` 证明互操作。
4. 测量静态体积、内存、任务数、CPU、TLS/握手耗时和失败清理，在现有128M/12任务限额内通过；超过预算需明确重新审查，不能直接加大。
5. 列明native crate接入文件、Worker v2的有限字段与新生产接线许可，再独立审核；批准后才加入依赖和实现。

候选上游主要声明来源：[根Cargo](https://github.com/ystreet/turn-proto/blob/d8d127ab1e37aef14f95a9f6b529073c31ee04a5/Cargo.toml)、[client proto Cargo](https://github.com/ystreet/turn-proto/blob/d8d127ab1e37aef14f95a9f6b529073c31ee04a5/turn-client-proto/Cargo.toml)、[client rustls Cargo](https://github.com/ystreet/turn-proto/blob/d8d127ab1e37aef14f95a9f6b529073c31ee04a5/turn-client-rustls/Cargo.toml)。这些只证明声明和版本，不证明库已满足本项目所有要求。

## 6. D-25/26验收矩阵（全部待实现运行）

| 编号 | 真正要运行的路径 | 通过条件 |
|---|---|---|
| R01 | 正常Core身份→实际私有TLS TURN→双方ICE/DTLS/Opus合成音 | 有真实allocation、selected pair和双向可解码数据；授权前PCM打开/语音发送均为0 |
| R02 | 错CA/主机名、过期证书、明文服务冒充TLS | 证书错误即停，无关闭校验的重试、无媒体票据消费或host回退 |
| R03 | 伪relay候选、错allocation generation、未创建permission | 不把字符串/JSON自报当能力；实际peer路径不成立即拒绝 |
| R04 | DTLS指纹与正常信令不符，旧会话指纹重放 | 不签发/消费有效媒体票据，PCM仍关闭 |
| R05 | 成功后撤权、换SIM、call/lease/principal generation变化、Core重启 | 实际Core/Authority/Worker停止传声；旧票据/迟到回调不可恢复 |
| R06 | TURN断流、续期失败、半帧/超长帧、分配中取消future | 有界退出、立即关PCM，资源释放证实或明确Unknown，服务器TTL最终回收 |
| R07 | 错UID/同UID错session、重复session、旧v1 Worker、新旧混用 | 拒绝跨session；不降级到LAN权限、不自动重试业务请求 |
| R08 | 恶意relay_handle、目标地址、DNS变更、任意局域网peer | 固定登记项与真实目的策略共同拦截，无任意代理能力 |
| R09 | 重放、重复ticket、reply id/boot/type错误、未确认本地IPC结果 | 实际Core唯一票据/租约裁决，继承PR55撤销语义，失败不保持旧Permit |
| R10 | 并发停止/撤权/迟到分配成功，资源耗尽和有界洪泛 | 每个成功创建资源均有关闭路径，旧generation不得发布，预算不放大 |
| R11 | 原LAN v1及已有WebRTC合成回归 | 原限制和声音路径保持，误用公网/IPv6仍拒绝 |
| R12 | ARM受限服务、真实私网/外网两端听音、切网 | 主线另行安排设备/服务器/真人验证，不拿本地模拟替代 |

隔离环境要求：普通UID、私有network namespace、仅loopback/测试私网、合成证书和音频；不读真实ALSA设备、不放宽生产CA、不连接公网、不用生产TURN凭据。测试服务可由夹具启动，服务实现/版本也需固定，不能用客户端自身的假端口完成R01。

## 7. 分工与下一步

- lead接回D任务：D-25a提案→接口/依赖正常审核→真实适配与上述隔离测试→D-26补完整跨模块矩阵。只有达到原验收才从D清单扣除。
- A：安全会话仍按原分工；用户已确认尚未完成，不用假实现接进生产。
- C：`RelayTemplateV1`、服务器/DNS/证书私有模板和NAS正常接口消费；本文没有代替C交付或部署。
- 主线/Claude：具体设备、Root安装、正常PR合并和真实外网验收。当前方案不申请并行控制设备。

当前lead已交付D-01～20离线包，D-21～26仍6步；待审提案不是第21步完成。交付分支全局20/169、加权10.6%，原P0–P7约80%保持；这些数字不是main/Pages已更新证明。

协议依据：[RFC 7587 §4](https://www.rfc-editor.org/rfc/rfc7587.html#section-4)规定Opus RTP时钟48 kHz；[RFC 8835](https://www.rfc-editor.org/rfc/rfc8835.html)说明WebRTC传输/TURN要求；[coturn官方说明](https://github.com/coturn/coturn/blob/master/README.turnserver)介绍临时凭据和部署选项。实现仍以固定源码和真实测试为准。

## 8. 本次实际验证

源码归档 `ad62c49550c9267efa0cf7e898f4a0cd65047ad54bd9d32116943ee8070ca846`，日志摘要 `65f1f8567193446d7d903fe55b848cc75a468bbb07c5c0c4babd195a1472d386`。隔离Linux、UID65534、网络/设备隔离：`python3 -m unittest discover -s tools/progress -p 'test_*.py'` **23项通过、0跳过**；`python3 tools/progress/build.py --check`通过。Windows执行Node看板计算/过滤/依赖/估时/转义/五人步骤唯一归属检查及生成一致性、diff检查通过。更新任务文档后，旧计划源哈希检查先正确拒绝；重新按同一成员/已审记录投影计划后通过，没有重算或增加Git贡献/审核数量。

这些只验证提案台账与看板一致性，不是R01～R12或Rust/ARM/音频回归；未因文档变更重复运行无关生产Rust测试。归档在加入本段验证结果之前冻结，最终提交仅多这段结果和交付台账证据。
