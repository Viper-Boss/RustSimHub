# SIM 观察与短信收件箱

2026-10-13。当前实现是 Linux 用户态读取链路，尚未形成完整可刷写系统。

## 模块与边界

`core/observer.rs` 通过系统 D-Bus 的固定 ModemManager 接口读取：唯一总线 owner、ObjectManager、Modem 的 Sim/UnlockRequired、SimIdentifier、Messaging.List、Sms 属性。适配器没有 Create/Send/Delete/Enable/SendPin 等方法，也不暴露任意 AT、QMI 或 shell。

每轮建立连接并重新枚举，仅接受一个 modem；MM 重启、路径变化、ICCID 变化或失去可信身份都重新隔离 line epoch。ICCID 必须是 8–32 个 ASCII 数字，否则状态保持 unavailable。无 SIM 为 absent，需要解锁为 locked。同一 backend 和 SIM 指纹经过至少 2 秒的单调时钟观察后进入 ready；当前轮询间隔 3 秒，每轮限时 12 秒。这表示 SIM 身份稳定，**不表示已注册网络或可拨号/发送**。

指纹在 auth.Identity 中用 HMAC-SHA256 计算：先从设备 seed 和独立用途域派生专用观察 key，再计算 ICCID 指纹。观察器不持有签名 seed，不争用鉴权锁；原始 ICCID 不入库、不出现在状态接口或日志中。backend 包含进程随机 instance、MM 唯一 owner 及对象路径，仅保存到私有数据库。

PIN/PUK 等阻断锁保持 locked；PIN2/PUK2 按 MM 语义允许普通读取，但仍要求 modem State 至少 DISABLED，并成功读到稳定 ICCID。UNKNOWN 锁、失败/初始化状态保持 unavailable，绝不自动提交解锁码。实机曾报告 UnlockRequired=3 (PIN2)、State=8 (REGISTERED)，不能误当成 SIM PIN 锁。[MM 锁状态实现](https://github.com/linux-mobile-broadband/ModemManager/blob/main/src/mm-iface-modem.c)明确区分这两类限制。

读取完整短信后再次核对唯一 owner、modem/SIM 路径及 ICCID；变化则丢弃本轮候选，进入 unavailable，下一轮重新观察。短于观察周期的拔插，以及最后核对之后的硬件变化，不能由轮询证明已捕获；未来发送路径必须另加信号失效通知和最终执行门禁。

## 存储

storage schema v3 增加 line_observation、inbox 和 inbox_events，v1/v2 原表与事件游标保留。单写者执行换卡 epoch、旧 Prepared 取消、Submitting 转 Unknown、观察状态和 line 事件的同一事务。数据库重新打开先重置 ready 为 unavailable。

仅接受 State=RECEIVED、PduType=DELIVER/CDMA_DELIVER。Receiving、已发短信和送达报告不作为完整收件短信落库。UTF-8 原样保留，不解释字面 `\\n`；二进制 Data 单独保留，禁止非空 Text/Data 同时存在。Text/Data 各最多 4096 字节、号码 256 字节、时间戳 128 字节、对象标识 256 字节。

每个完整消息和 sms_received 事件在同一事务提交。相同 line/source 的相同内容重复读取返回原记录；同源内容变化返回 Conflict，不覆盖证据。不同对象或重启后的同内容消息保留新记录，用 duplicate_candidate 指向同一 SIM 指纹下的相同内容候选，绝不据此删除消息。因此长期重启可能积累候选重复；这是当前保留证据的明确取舍。

line 表示**首次观察这条 modem 对象时的线路代次**，无法证明 modem 内历史短信属于哪张 SIM。ME 历史短信、旧服务并发修改对象、同路径复用仍需更完整的对象/存储对账。当前不删除 modem 消息；超过 256 个对象或缺失/超限属性时停止本轮导入并保持硬件原件。一个读错误会阻止该轮后续导入，需要后续实现逐对象故障隔离和有界分批。数据库仍有 32 MiB 上限，满盘失败不能误报成功。

## 私有 API

所有命令使用协议 v2、现有公钥登录、短期会话及 Unix UID 门禁。读取时重新检查持久权限和 generation：

| op | scope | 响应 |
| --- | --- | --- |
| line_status | status_read | line/status，无 SIM 标识 |
| inbox | sms_read | 有界摘要页，无正文/号码 |
| sms_detail | sms_read | 单条完整正文、号码、时间、二进制 |
| inbox_events | sms_read | 有界持久事件页，无正文/号码 |

inbox/inbox_events 接受 `after: null` 或 `{db_epoch, seq}`，limit 为 1–32。游标对应各自序列表，不可混用；seq 为十进制字符串。跨恢复的 epoch、未来 seq、非法游标拒绝，需要从 null 重新同步。摘要中只有 UUID、line、binary 和 duplicate_candidate；最坏合法单条正文响应仍小于 64 KiB 帧。

CLI：`simhubctl line|inbox|inbox-events SOCKET SESSION_FILE`；正文用 `simhubctl sms SOCKET SESSION_FILE SMS_ID`。CLI 当前读取第一页，后续页可以直接用受鉴权 IPC。短信输出属于私有内容，不转贴到诊断/公开日志。

`--serve-authenticated` 不连接实际 modem，适用于纯鉴权/合成测试；`--serve-observing SOCKET UID STATE_DIR IDENTITY_DIR` 显式启用真实只读观察。两个模式都没有发送/拨号接口。旧内部 prepare/begin_submit 不能直接接到网络；实际 Send 尚需 ready/epoch、权限撤销、持久 Submitting 与 MM 对象对账的最终检查。

## 后续工作

1. 信号订阅、断线重连和轮询兜底；逐对象隔离、分批、大短信处理及 MM 对象复用对账。
2. 独立设备测试短信真实接收、多段拼接、换卡、PIN 锁与 MM 重启；当前单元测试不能替代这些硬件实验。
3. SmsSend 最终门禁、持久提交和 Unknown 查询恢复；不能通过重启自动重发。
4. 生产服务部署、TLS/远端传输、通话/音频、恢复闭环与系统镜像集成。

接口核对来源：[官方 Sms XML](https://github.com/linux-mobile-broadband/ModemManager/blob/main/introspection/org.freedesktop.ModemManager1.Sms.xml)、[官方 Sim XML](https://github.com/linux-mobile-broadband/ModemManager/blob/main/introspection/org.freedesktop.ModemManager1.Sim.xml)、[官方枚举](https://github.com/linux-mobile-broadband/ModemManager/blob/main/include/ModemManager-enums.h)。实机支持以当前安装的 MM 为准。
