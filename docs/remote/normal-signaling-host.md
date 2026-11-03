# 正常 Core 授权的远程信令主机

本模块完成 #56/#132 已批准离线范围内、实际worker之前的正常业务门禁；位于media的显式 `remote-signaling` feature，native `turn-relay`选择它。没有注册HTTP/NKN路由或安装服务，默认LAN/v1和正常Core命令保持。生产准入仍false，原DTLS依赖审查/正式Root与消费端安装、实机验收独立保留。

`remote_signaling::Host::prepare(Prepare)`只供实际Edge985的受信本地信令层调用。真实/有效UID均核985；生产Core986和Media984、两个固定 `/run/rustsimhub/...` 端点写在私有构造器，不能由JSON、URL、SID、UID/路径字段或配置覆盖。测试自定义端点只有cfg(test)，完整跨UID正例调用的是非测试library的正常构造器。

Prepare输入是普通正常token、原domain::CallLease、登记handle、正常已认证信令提供的expected_peer和offer；它们本身没有执行权。session内部用现有getrandom生成，Host私有字段且无Clone/Debug/Deserialize，也没有替换身份/lease/session的setter。正常调用者不能把snapshot当ticket上传回来、查询/停止另一个对象或发送StopAll。

先通过原Core `CheckMediaLease`：实际socket为Core986拥有/单链接/0660、父目录同属且不组写/其他写，真实SO_PEERCRED/PID与inode再次核对。原Auth/Calls Controller核正常身份、media.use/calls.control、当前活动呼叫、line/call/修订/租约/主体代次。检查成功前不连接Media、不创建TURN allocation；远程源/Origin/IP/恢复权或NKN成员资格不新增权限。

Core RPC只可由模块创建原 `CheckMediaLease`、`IssueMedia`，复用原AuthRequest/版本/解析器；没有拨号、续租/挂断、身份库或新的Core授权版本。单次总预算1秒，原Core/Edge的既有接口期限不变。接收严格对象/字段/正例形状、版本和请求id，凭据仍核boot/peer；不确定结果不重发。token与RPC缓冲在此层用Zeroizing持有，不记录token/ticket/SDP/ICE/音频；沿用原凭据类型，不能据此宣称所有跨层内存已零化。

Core返回的binding只用于当前Host的一致性检查：它由原Core检查结果产生，不接受对端自报。Prepare实际完成后、每次poll前后和激活前后再次核正常Core/绑定。普通v2 snapshot永不制造Permit。只有实际worker的当前Verified phase和peer_key吻合，才请求原Core签发一次性媒体票据，私下交给同一v2连接Authorize。返回给消费端仅answer与Status（phase/pcm/counters/release），没有ticket/token/peer_key/binding/登记凭据。

每次Host操作在任何Core await之前持有该worker连接的取消围栏；Core失败、身份/绑定改变、future取消或坏回复都会shutdown原fd，即使调用者继续持有Host也不能恢复。激活尝试在await IssueMedia前标已开始，丢失/取消Core回复不能自动重新领票。原Core票据5秒、媒体Permit1秒/250ms pulse、4帧/60ms及actual worker撤权机制全部保持。Core先前可能处理过的未知Issue结果留给原票据TTL/权威裁决，不伪称撤销远端结果或重放。

close消耗本Host、只Stop对应连接，返回Unknown，不调用CallAction/Hangup/RenewCall，不以媒体活动延长手机通话租约；Drop同样关闭原v2连接。提前activate、重复激活或任何失败使该Host连接终止，消费端需保留明确失败/Unknown，不自动重拨/重建业务。

测试分两层：Core普通测试直接运行原Auth、正常登录/Store、Calls Controller、authenticated::handle及media_ipc，恶意worker transport仅用来测试host边界（不是DTLS/PCM成功证明）。专用不同UID夹具再把非测试Host API、实际正常Core上述模块、原媒体Authority和真实native worker/TLS/Coturn/ICE/DTLS/Opus连在一起，只替代外部私有ModemManager/PCM/登记和远端音频对端。正常token与CallLease是真实原Core流程产生，媒体票据不预造/不写夹具文件；原Owner存储撤权使实际媒体停止。

固定IPv4服务登记、正常TLS/证书、真实selected pair、无LAN回退由现有实际SDK/worker负责。源文件的cfg(test)端点和Root同UIDparser夹具不在普通binary中；实际不同UID测试不冒充生产账户/Root安装、真实蜂窝/PCM/真人两端听音。x86三角色夹具为128MiB/16pids整体容器；ARM Media/Edge双QEMU接原x86 Core的组合需128MiB/24pids，原16pids启动失败如实保留。单ARM worker矩阵仍128MiB/16pids。不将这些说成单一服务或整板生产预算/并发负载验收。[逐来源正式结果、失败及边界](member-d-normal-host-evidence.json)。
