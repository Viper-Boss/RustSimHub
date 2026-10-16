# 常驻媒体工作进程

2026-10-16源码阶段：常驻worker、受限IPC/客户端、Core租约预检已实现。本机WSL Rust1.98.0的193项workspace、17项Native测试和两者Clippy通过；ARM冻结构建与实机验收尚待完成。当前整体70.5/100，不据此宣称网页媒体或完整系统可用。

`rustsimhub-audio-alsa --serve-worker PRIVATE_CONFIG`要求固定Media984/Core986/Edge985及Root管理的锁定服务账户。配置仅有control_socket、core_socket、bind_ip：私有文件、显式本地IPv4、绝对有界Unix路径。不接受网络提供的UID、ALSA路径、shell、录音路径、呼叫控制或硬件profile。只监听本地控制socket；音频UDP仅为实际协商的一条会话。

控制socket父目录必须由Media拥有、不可被组或其他用户写入，采用2750和IPC组继承。私有flock防双实例；只在确切owned旧socket连接被拒绝时回收。退出仅删除原socket inode，其他替换文件保留。只接受SO_PEERCRED为Edge985的连接；最多4个控制任务，每次24KiB/1秒，一次一请求。SDP上限16KiB、ASCII音频/Opus/RTCP mux、固定私有IPv4主机候选与真实DTLS指纹规则沿用已验收Session。

`crates/media/src/worker.rs`定义独立版本协议：prepare(session,peer_ip,offer)、inspect(session)、authorize(session,Core credentials)、stop(session)。session为随机32位小写十六进制标识。先prepare得到answer，完成实际DTLS后inspect才返回verified及真实peer_key；此时还没有PCM、编解码授权或Core票据。随后受信任适配层请求Core的IssueMedia，把短期凭据送入authorize。worker核对真实peer、实际Core socket owner和SO_PEERCRED，再由Core消费单次票据，取得持续许可后打开固定PCM。授权不能由HTTP客户端自行断言。

Core新增check_media_lease(token,lease)：同样要求media.use、calls.control、当前principal/generation、当前呼叫与租约、实际ACTIVE以及稳定SIM/后端；返回稳定的opaque binding，不生成票据，不占pending，也不控制呼叫。binding随owner/line/backend/call/lease incarnation改变，正常续租不改变，用于后续Edge会话隔离。原HTTP通用API仍拒绝直接IssueMedia；网页适配尚未接入。

worker只允许一条会话。相同session且相同offer/peer重试返回原answer，不新建RTC或再次打开PCM；不同内容拒绝，其他活跃session busy，关闭后的同id不复活，并保留有界已用id。提前授权、换peer或重复授权拒绝并停止；authorize只排入容量1的通道，不在未实际打开前声称active。状态依次为handshaking/verified/authorizing/active/closing/closed。pcm_opened是该会话曾实际打开的标记；closed仅在音频线程退出、Pump/RTC关闭后写入。

Core心跳独立于PCM线程，每250ms刷新，断开、许可/租约失效或stop会先撤销许可再join。PCM/Opus每帧继续检查许可，网络断流保留原一秒媒体超时。SIGTERM/INT或控制服务失败会停止并等待会话线程，之后移除控制socket；没有自动拨号、重拨或短信。统计只含帧计数、丢帧及xrun，不含声音、SDP或凭据日志。

`simhubctl worker-request SOCKET PRIVATE_REQUEST NEW_PRIVATE_REPLY`使用固定984对端校验，校验协议/id/session后将回复写入新私有文件，仅打印保存确认；用于调试及后续适配复用，owner私钥无需交给Media。native诊断新增offer_synthetic_gated：完成DTLS后等私有synthetic-start文件的精确start换行标记，再开始静音媒体，等待期间不开PCM、不发送媒体包。该模式用于先做握手再执行一次受控10010验收，不能替代设备Core许可。

本机测试覆盖真实DTLS握手前后无PCM、换票据peer拒绝、伪Core身份拒绝、重复prepare/busy/终止后不复活、socket双实例/活跃旧端点/链接与替换证据保护、IPC错误UID及回复session拒绝，以及Core预检不消费票据并拒绝错误owner/租约/非ACTIVE。旧音频/Opus/DTLS/SRTP/撤销测试保持通过。

下一步：同源ARM构建与真实Core/Media隔离验收，然后Edge受鉴权的两阶段信令、网页getUserMedia/播放/取消及正式开机单元和安装配置。现阶段仍为IPv4 LAN，原计划的动态多接口、mDNS候选、远程/TURN、来电和真实双向听音没有取消；media_available继续false。普通蜂窝实测仍等换卡。
