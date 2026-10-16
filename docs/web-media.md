# 网页媒体接入

2026-10-16源码阶段。增加Edge两阶段媒体接口、浏览器麦克风/播放/取消、受信任多监听地址和Native多绑定地址白名单。本机WSL Rust1.98.0通过201项workspace、20项Native及两者Clippy，Node确切media.js生命周期与原proof.js验证通过；最新日志.private/media-edge-final-source-second.log。此接口的ARM、实机和真实浏览器验收尚待完成；上一轮e271常驻媒体验收不能替代网页验收，整体仍71.5/100约72%。

Root管理的Edge私有配置可选择`media_worker_socket`，固定Media UID984；未配置时仍禁用麦克风并返回媒体不可用。`additional_listen_ips`最多三个额外私有地址，与主地址共享固定HTTPS Origin和证书名；尚未分配的地址只做本机bind重试，不探测互联网或回退到蜂窝。实际TCP接受地址决定Native音频源地址，实际TLS客户端地址决定对端，忽略转发头。不同接口的同名域名解析及证书交付仍要独立验收。

Native仍保留旧`bind_ip`，Root可用`additional_bind_ips`再列最多三个地址；只有白名单中的已分配IPv4地址才可绑定。IPC增加可选prepare.bind_ip，重复prepare不能换源地址。新增本地`stop_all`仅供受信任Edge启动/退出接管：停止并join旧会话后才确认，不拨号、不修改电话状态、不复活已用会话ID。HTTP不提供这一命令。Edge使用TLS私有目录锁防止同目录重复实例，启动时关闭残留会话，正常退出及运行时监听错误后再关闭；硬SIGKILL时的产品服务联动仍待正式单元集成。

`GET /api/media/capabilities`给出是否配置此适配器和当前IPv4 LAN范围，不等于Core已经允许媒体。`POST /api/media/v1`要求相同Host/Origin、JSON和现有请求标记，禁止query凭据。协议1包含id、token、当前CallLease和以下命令：

- prepare(session,offer)：session为客户端随机32位小写十六进制标识。Core先验证media.use/calls.control、真实ACTIVE呼叫、当前owner/租约/线路，取得内部binding；本地保存binding、实际连接地址和offer摘要，再发送受限IPC。重复请求仅接受同一组内容。
- poll(session)：重新做Core授权，只返回自己的会话状态、answer和计数；不返回票据、peer_key或binding。
- activate(session)：Native必须已经完成真实DTLS并给出实际peer_key。Edge内部申请Core单次票据，再交给Native消费。浏览器不能自报peer_key、UID、绑定地址或凭据。重复active查询不再生成票据；不明确的已开始授权会关闭会话，不能再次发放。
- stop(session)：同样需要当前Core授权和会话归属，只停止自己的媒体。租约失效/撤销由Core持续心跳关闭PCM；浏览器同时关闭本地音轨和连接。

泛用`/api/v2`同时拒绝IssueMedia和CheckMediaLease；内部binding不从此路径泄露。媒体接口独立限速，单一信令操作串行、有界帧和超时。后台只回收明确closed/不存在的会话，不把连接超时当成已经关闭。未确认结果不自动重试授权、拨号或重连。

SDP只保留与实际TLS对端相同的IPv4 UDP host候选；标准隐私`.local`候选替换成该同一对端地址，不解析名称，也不选择其他目标。其他IP、IPv6、TCP、中继及任意DNS目标不用于此LAN接口；Native仍独立验证媒体形状、指纹和每次输出的对端地址。远程/TURN范围未取消，将另行实现和验收。

浏览器按用户点击请求麦克风，仅协商Opus、无外部ICE服务器，握手后只提交一次activate。私钥继续保留IndexedDB，媒体token/lease/SDP只在内存；没有录音。停止、退出、关闭页面、网络失联和租约失败都会关闭本地音轨、transceiver和PeerConnection；迟到的getUserMedia成功结果立即停止，不影响之后的新会话。麦克风授权等待独立于电话操作，不阻塞挂断。播放被浏览器暂停时提供显式播放按钮。旧浏览器配对若缺少media.use必须经所有者重新授权，不能自动升级权限。

本机失败测试覆盖同源、实际连接地址、跨owner/会话、变更offer、提前授权、重复/不明授权、伪造Core票据、接管/回收及配置拒绝。Node执行确切media.js验证迟到授权、开始即取消、静音、断网、丢失回复和关闭；这是生命周期测试，不宣称真实音频/浏览器已通过。标准参考：[W3C WebRTC](https://www.w3.org/TR/webrtc/)、[Media Capture and Streams](https://www.w3.org/TR/mediacapture-streams/)，2026-10-16核对。
