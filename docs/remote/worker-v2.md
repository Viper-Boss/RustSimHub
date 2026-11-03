# 远程媒体 worker v2（显式离线适配）

承接用户批准的 #56/#132 与 #170 TURN SDK。新入口只有 native `turn-relay` feature 下的 `--serve-remote-worker CONFIG`；默认 LAN/v1 和服务材料保持。不会创建账户、安装服务、开启外网端口或给现有网页加远程开关。后续[正常信令Host](normal-signaling-host.md)已将原Core身份/活动呼叫/原租约门禁接到实际worker并隔离验证；生产准入仍受原 DTLS `crypto-bigint` 告警、正式消费端/Root登记安装和设备验收约束。

生产入口使用同一份原生服务身份检查：真实/有效 UID 必须984，Root拥有的真实 NSS 必须分别把 media/edge/core 解析成984/985/986、nologin。配置经现有 private reader 读取：当前媒体UID拥有、父目录0700、文件0600、单链接、拒绝symlink/超长。固定服务端点 `/run/rustsimhub/media/remote.sock`，固定正常 Core 授权端点 `/run/rustsimhub/core/media.sock`；重用原 socket lock/inode清理，服务端0660、受保护媒体目录、真实SO_PEERCRED/有效PID，只有Edge985可请求。实际安装及IPC/audio组由主线维护，不能据BLE独立账户方案禁止正常媒体原有组。

Root登记JSON严格protocol2，唯一 `relay_handle`、`address`（固定SocketAddr）、`hostname`、`ca_der`（有界DER数组）、`username`、`password`、`until_ms`（绝对CLOCK_BOOTTIME，不是可续期TTL）。普通对象、重复/未知字段拒绝；总24576字节，CA最多8192字节，期限最多一小时，进程内保存同一个绝对Instant。正常CA/名字/期限与公网IPv4策略复用SDK；loopback仅test构造器，生产配置不接受。Edge请求不能传地址/URL/凭据/设备/身份代次或额外许可。

`rustsimhub_media::remote_worker` 是独立版本2的有界持久Unix协议，不复用v1的反序列化。帧24576、SDP16384；命令仅Prepare（session32位小写hex、已安装handle、正常受信信令提供的expected_peer、offer）、Inspect、Authorize（原Core Credentials）、Stop。禁止StopAll、电话控制、Owner/verified/authorized布尔和Root配置。严格对象/重复键/credentials对象、返回版本/id/session/阶段/对端/PCM形状均核对；即使合法snapshot也不能构造Permit。

客户端绑定实际socket UID/inode/mode、真实SO_PEERCRED与PID，路径必须绝对且有界。一个连接只Prepare一次、固定session、连续请求id；取消或任何不确定的实际交换立即shutdown原fd，不透明重连/重放。Prepare全部响应预算10秒，其他响应1秒。服务端一次只持有一个真实连接，其他连接即使同为Edge也不能查询/停止/接管。固定一条待处理请求，完整帧10秒且下一帧计时在上一响应之后开始，响应写500ms；重复id/换session/坏帧或失败操作终止该连接。

Prepare只分配真实TURN并回答SDP；实际selected relay pair和固定DTLS指纹成立后才到Verified。Authorize先匹配实际VerifiedRemotePeer与票据peer_key，调用原 `local::Client::redeem` 连接正常Core986消费单次票据；boot/call/line/主体/代次/租约仍全由原Core唯一权威判定。原Permit才可进入既有RTC/Opus和固定UfiDuplex。没有第二身份库/Owner、另发媒体票据、手机租约续期或网络活动延权。

Core观察以原250ms pulse运行；RTC异步操作在pulse之间保留同一个future，不能取消partial TLS后继续使用残帧。外部PCM打开/非阻塞读写在唯一拥有线程，队列沿用4帧/60ms。新 `Pump::step_with_clock` 在外部读取后重新取时再检查授权与帧年龄、写后再gate；旧 `step(now)` 保留原固定时间语义。设备wrapper也在每次外部调用前后核当前Permit/停止位，读取迟到撤权时清零结果，不发出迟到语音。

独立dup FD只监控RDHUP/error、不读取；读帧者唯一拥有字节，pipeline不能挡住实际EOF。EOF、交换取消、Core丢失、过期、SIGTERM或Stop先同步drop Core Client撤销镜像Permit、停止PCM、drop真实TURN/Codec，再等待迟到物理操作返回。线程不可detach，不提前释放服务slot或报告Closed；不能结束的外部设备操作由主线有限服务cgroup停机兜底，本适配不自杀/伪造释放。Stop只关闭本地媒体，不挂断电话、不更新通话租约。

关闭返回 `release=unknown`；没有远端唯一DeleteAck，不把TLS关闭或本地PCM关闭当TURN释放证明。新worker使用立即本地围栏，服务器按原allocation TTL回收；不承诺即时远端deallocate。

测试只替换外部Core业务context/PCM/私有TLS后端，实际Authority、原Core媒体协议/Client、v2 server/client、TURN/RTC/Codec/Permit均为同一生产源码。cfg(test) host注入不在普通二进制中；实际非测试CLI另验证固定UID/NSS、loopback拒绝与清理。完整Core daemon/真实身份与电话状态、Edge远程信令/C消费端、Root登记安装、真实PCM/外网/目标板/两端听音不是这些夹具的结果。
