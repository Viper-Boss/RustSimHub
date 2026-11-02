# D23：认证后的有限状态读取

2026-11-01（Asia/Shanghai）。`network_trial::provision::recovery_v1` 现在支持真实 Auth 的 ReadStatus 能力，正文严格为 `{}`，输出冻结 T5 的五字段 `{contract:1,source:'core',wifi,busy,recovery_available}`。新增 Rust Response::Status 变体；依赖原枚举做穷尽匹配的 Rust 调用方须接这个变体，JSON 字段不扩展。默认不开启，无新监听器或密码实现。

## 实际读取与证据边界

解码、SQLite、D-Bus 和最终授权复核共用原查询 **3 秒**截止；前后用同一 Auth 核确切动作/正文、原连接/session、期限和当前 grant。撤权/断连期间读到的后端事实不能作为成功响应返回。现有 Auth 把持久 grant 拒绝合并成 Storage 错误，T5 保留有限 `unknown` 错误，不冒充具体 `revoked` 原因。

生产分支使用实际 system bus 和原 NetworkManager 固定 RPC；测试注入的私有连接只存在于内部 Rust 接点，不从请求取地址。只读 GetDevices（最多16、路径/去重校验）、唯一 wlan0/type2、Managed、WirelessEnabled、State、ActiveConnection.State、Checkpoints、GetPermissions 和原 MM 有限 Calls/State；**不**调用激活、设置、checkpoint 创建或任何电话控制方法。

- connected 要求受管、无线启用、设备 State=100 和严格 ActiveConnection 路径的 State=2；disconnected 只取 State=30；其他/正在变化/不可信均 unknown。它是关联状态，不证明互联网可用。
- 独立状态代理禁用属性缓存，末尾重新读取设备类型/接口/管理/状态/无线及 NM/MM unique owner；服务替换或观察前后不一致丢弃事实。正常业务代理保持原行为。
- recovery_available 仅是当前检查时的就绪快照：SQLite 明确 schema9、当前 grant、原 fence、全局容量尚有 Begin 空位，没有任何 Unknown/非终态 Begin；原 Core actor/试连槽/网络电话协调不忙；NM 没有 checkpoint、MM 空闲，并且 network-control/checkpoint-rollback/settings.modify.system 均明确 `yes`。`auth` 需要交互时不称可用。
- 后端错误/缺能力降为 unknown、busy=true、recovery_available=false；授权错误返回有限错误，不返回状态。状态中没有 SSID、地址、设备 pin、Owner/grant 或业务数据。

SQLite 接点只有只读事务，不登记、领取、改行或恢复 Unknown；布尔不签发能力，也不保证稍后的 Begin/Confirm 会成功。写入仍独立核同正文、原 actor、唯一锁、电话、USB/获核恢复分支、WWAN/APN 及保存前后门禁。Core 协调锁只在状态取样时立即尝试并释放，不持锁跨 D-Bus await。状态没有新建/延长试连期限。

## 实际发现并修正的正文格式问题

serde 的结构体默认也可由位置数组反序列化。真实 Auth 接入后的测试证明，原候选解码器接受 Begin `[ssid,passphrase]`、Confirm/Cancel `[trial]`、Query `[target]`，与对象契约不符；新 ReadStatus 的空结构也接受 `[]`。四动作测试实际得到 `[true,true,true,true]` 并 FAILED。

共享 Core 解码器现在先要求第一个非 JSON 空白字节为 `{`，再走原 serde 字段/重复字段/类型/尾随值校验；Status 复用它。原已认证字节不规范化、HMAC 不变，也没有用 Value 保留凭据。新测试保留，之前的正文测试继续运行。这是可选候选 API 的格式缺陷，没有证明绕过 Auth、修改绑定或旧生产默认已受影响。

## 验证与余项

5项 Core 状态测试使用真实私有 dbus-daemon、合成 NM/MM 端点和真实 Auth/Identity/SQLite，覆盖连接/断开/未完成关联、无线/许可/checkpoint、正文格式、读取中撤权、无属性变更信号的状态变化、实际 unique owner 替换、原电话锁与 3 秒队列截止。另有1项四动作数组回归、2项真实 Storage 容量/Unknown/当前授权/fence/7与9模式测试，共8项新增。测试过程没有连接系统总线、设备或外部网络。

跨语言脚本直接校验实际 Rust 的 `T5_CORE_STATUS` 和原 `T5_CORE_RESULT`，使用未修改的前端 contracts.js。精确 HEAD、命令、数量/摘要见[交付报告](../deliveries/member-d-core-recovery-status.md)。

还需 A 实际安全会话及 host/前端接线、低权服务及生产角色、媒体适配和设备联验。本包不复制 A 密码、不更改其他成员或全局台账、不自行安装。2026-11-01 用户另已批准 #53/#56/#132 的后续**离线**实施；该批准不等同生产 UID/安装或实机通过。完整阶段按原验收闭环统计，不能以新增测试数计完成率。
