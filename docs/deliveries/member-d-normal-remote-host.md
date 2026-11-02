# D25：正常 Core 身份/租约到实际远程 worker

2026-11-02。用户已批准 #56/#132 离线实施。起点#170 `a333a1841137e105348fa4c402de6ec122aa0691`；继续同一公共入口PR，未自合并、未动main或其他成员实现。本包先开发并隔离验证，正式完整HEAD/数字在清洁源码测试后绑定，不能用脏源码的初验冒充正式结果。

[实现/边界](../remote/normal-signaling-host.md)。新media受信正常信令Host先走原Core CheckMediaLease，通过后才连接Media/准备TURN；实际Verified peer成立后才IssueMedia/Authorize。opaque Host、正常Core UID/版本/id/形状与固定端点、每步正常授权一致性检查、Core await期间取消也关闭同一worker连接、单次激活不重发；只返回脱敏状态。没有第二Auth/Call/Lease库、任意代理/身份/通话RPC、电话续租/挂断或新Owner。

独立模块/Root默认不启用，原LAN/v1不改；Core只加测试hook/feature，不改生产Auth/Calls/media_ipc。Root lock仅追加已有workspace边；Native lock189→192，仅新增本仓domain/policy/wire三包。真实活跃闭包/许可重新核对、registry/Git版本集合不变，没有新外部版本或密码provider，145个registry查询沿用注明时间的原快照，生产准入false/原DTLS RUSTSEC-2026-0387保持。

初验：实际原Core登录/Store/Calls Controller/normal dispatcher/media IPC五项边界通过，media新Core端口两项解析/来源测试通过。实际UID986的Core原身份/通话/票据/撤权，UID985的非测试Host构造器，UID984实际worker/原Authority、真实TLS TURN/ICE/DTLS/Opus双向和PCM关闭均在隔离容器跑通。只替代外部私有ModemManager、PCM/登记/远端音频对端；不再用合成Context/Authority代替正常Core，也不把mock worker的unit当整链成功。这些初验目前是脏源码，完整正式记录另绑。

NSS仅临时镜像、实际演员capabilities空，Root控制器仅容器setuid/gid/chown/fowner、无CAP_KILL；network none/read-only，未改host账户或设备、未拨号/短信/APN/生产签名/部署。Fake ModemManager中的一次拨号/启动是私有D-Bus后端计数，不是10010或任何真实外呼。AGPL/原作者与上游许可保留，Codex辅助，未重写贡献作者。

原23/26离线已提交、剩D22/D23/D25的口径先保持；正式D25隔离交付完成后才更新本人记录，不改主线台账/固定80%或把代码提交当设备验收。A生产安全会话/pin/签名域/Verifier/Sealer尚缺，D22/D23继续如实保留；原DTLS兼容修复/密码审查、正式消费端/Root安装及真实设备/外网/两端听音仍未验收。
