# 常驻媒体工作进程

当前结果见[生产集成验收](VALIDATION-MEDIA-WORKER-P3F.md)：e271ca1同源ARM及两轮真实生产授权/PCM/NAS解码和退出通过，123/214帧捕获、1/2次欠载恢复，累计10010=20；P3加0.5，整体71.5点约72%。没有活跃测试。网页、正式媒体单元、真实双向听音及长期稳定尚未验收，media_available=false。下面是研发过程和接口边界，旧“待验证”表述以本段与验收记录为准。

诊断补充：f36e9d6同源ARM通过，carrier-fourth完整生产授权/PCM/NAS解码和主动停止通过，122帧捕获、121帧NAS解码、峰值16473；但仍有5次欠载，不称稳定完成，累计10010=18。耗时仅记录整数：max_loop约21ms、max_service约2.5ms、max_pcm_step约14.9ms、max_codec约14.7ms；播放欠载时距上次成功写入18—27ms，累计最大间隔43ms。不能据此断言调度是唯一原因。正在验证固定播放环在启动和恢复时先预填既有2240帧静音阈值，2560帧硬件环及320帧周期、欠载限制不变；只在授权后打开，撤销后丢弃。ALSA软件起停阈值增加实际回读，null PCM启动/恢复均进入Running，本机194/19及Clippy通过，实机结果待验证。

后续实机记录：173b21f的ARM构建及负向预检通过。一次10010真实Core预检/单次票据、PCM打开与NAS解码已到达，但36帧后出现4次播放欠载而保护退出，不能称稳定媒体通过；报告media-worker-carrier-second.json清理/电话空闲/权限撤回均通过，10010累计16。程序原把PCM错误和异常断流都折为正常closed，现将PCM权限/设备错误分类保留，仅显式停止能消化关闭/撤销结果。回归后194 workspace、19 Native及Clippy通过。实机测试的状态轮询改为以Edge985执行同源诊断CLI，避免每次新建systemd命名空间；Core/Media的候选根目录、固定UID、空能力集合保持验证。正式Edge产品适配仍未实现，不把诊断CLI当网页服务。新修订ARM与实际复测待完成。

2026-10-16：233110a的193项workspace、17项Native/Clippy及同源ARM构建通过；真实候选用户态的第二轮无拨号预检通过，两次DTLS、固定Media984/Edge985和Root控制拒绝、换peer/无Core票据不打开PCM、进程复用及退出清理已验证，见私有media-worker-preflight-second.json。首轮因prepare请求失败保留，实机CPU耗时显示一秒预算过紧；SDK将prepare预算调整为五秒，其他控制仍一秒，服务端读帧/回复分别一秒，不把证书创建算作输入超时。慢证书回复和不完整帧回归后，本机194项workspace、18项Native及两者Clippy通过；修订后的ARM构建和实机复测待完成。当前不据此宣称网页媒体、授权PCM或完整系统可用。

`rustsimhub-audio-alsa --serve-worker PRIVATE_CONFIG`要求固定Media984/Core986/Edge985及Root管理的锁定服务账户。配置仅有control_socket、core_socket、bind_ip：私有文件、显式本地IPv4、绝对有界Unix路径。不接受网络提供的UID、ALSA路径、shell、录音路径、呼叫控制或硬件profile。只监听本地控制socket；音频UDP仅为实际协商的一条会话。

控制socket父目录必须由Media拥有、不可被组或其他用户写入，采用2750和IPC组继承。私有flock防双实例；只在确切owned旧socket连接被拒绝时回收。退出仅删除原socket inode，其他替换文件保留。只接受SO_PEERCRED为Edge985的连接；最多4个控制任务，每帧24KiB，读帧和写回复分别一秒，一次一请求；SDK为证书创建保留五秒prepare期限，其他命令一秒。SDP上限16KiB、ASCII音频/Opus/RTCP mux、固定私有IPv4主机候选与真实DTLS指纹规则沿用已验收Session。

`crates/media/src/worker.rs`定义独立版本协议：prepare(session,peer_ip,offer)、inspect(session)、authorize(session,Core credentials)、stop(session)。session为随机32位小写十六进制标识。先prepare得到answer，完成实际DTLS后inspect才返回verified及真实peer_key；此时还没有PCM、编解码授权或Core票据。随后受信任适配层请求Core的IssueMedia，把短期凭据送入authorize。worker核对真实peer、实际Core socket owner和SO_PEERCRED，再由Core消费单次票据，取得持续许可后打开固定PCM。授权不能由HTTP客户端自行断言。

Core新增check_media_lease(token,lease)：同样要求media.use、calls.control、当前principal/generation、当前呼叫与租约、实际ACTIVE以及稳定SIM/后端；返回稳定的opaque binding，不生成票据，不占pending，也不控制呼叫。binding随owner/line/backend/call/lease incarnation改变，正常续租不改变，用于后续Edge会话隔离。原HTTP通用API仍拒绝直接IssueMedia；网页适配尚未接入。

worker只允许一条会话。相同session且相同offer/peer重试返回原answer，不新建RTC或再次打开PCM；不同内容拒绝，其他活跃session busy，关闭后的同id不复活，并保留有界已用id。提前授权、换peer或重复授权拒绝并停止；authorize只排入容量1的通道，不在未实际打开前声称active。状态依次为handshaking/verified/authorizing/active/closing/closed。pcm_opened是该会话曾实际打开的标记；closed仅在音频线程退出、Pump/RTC关闭后写入。

Core心跳独立于PCM线程，每250ms刷新，断开、许可/租约失效或stop会先撤销许可再join。PCM/Opus每帧继续检查许可，网络断流保留原一秒媒体超时。SIGTERM/INT或控制服务失败会停止并等待会话线程，之后移除控制socket；没有自动拨号、重拨或短信。统计只含帧计数、丢帧及xrun，不含声音、SDP或凭据日志。

`simhubctl worker-request SOCKET PRIVATE_REQUEST NEW_PRIVATE_REPLY`使用固定984对端校验，校验协议/id/session后将回复写入新私有文件，仅打印保存确认；用于调试及后续适配复用，owner私钥无需交给Media。native诊断新增offer_synthetic_gated：完成DTLS后等私有synthetic-start文件的精确start换行标记，再开始静音媒体，等待期间不开PCM、不发送媒体包。该模式用于先做握手再执行一次受控10010验收，不能替代设备Core许可。

`tools/test-media-worker-preflight.py`默认仅做无拨号检查；显式`--call-10010 --once-tag UNIQUE`还要求已有人工拨号授权，先完成负向预检再新建生产Core/签名owner，以临时最小Voice规则验证一次真实呼叫。Root拥有owner私钥，Edge只取得短期票据。拨号标记在提交前独占创建，18秒独立停止计时器先启用；Unknown只查询，绝不自动再拨。临时规则、Core/Media和计时器在退出时清理，再核验原四服务/PCM/电话空闲及Voice重新拒绝。私有信令先写临时文件、设定权限和owner，再原子发布，避免消费者读到中间权限。

本机测试覆盖真实DTLS握手前后无PCM、换票据peer拒绝、伪Core身份拒绝、重复prepare/busy/终止后不复活、socket双实例/活跃旧端点/链接与替换证据保护、IPC错误UID及回复session拒绝，以及Core预检不消费票据并拒绝错误owner/租约/非ACTIVE。旧音频/Opus/DTLS/SRTP/撤销测试保持通过。

下一步：同源ARM构建与真实Core/Media隔离验收，然后Edge受鉴权的两阶段信令、网页getUserMedia/播放/取消及正式开机单元和安装配置。现阶段仍为IPv4 LAN，原计划的动态多接口、mDNS候选、远程/TURN、来电和真实双向听音没有取消；media_available继续false。普通蜂窝实测仍等换卡。
