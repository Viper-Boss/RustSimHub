# rustsimhub-forwarder

Rust原生短信转发后端：规则、请求编码、有界持久outbox及真实Auth/Store适配。已有Core管理/执行IPC、固定出口HTTPS、独立worker及通过临时systemd验证的有限服务管理候选；Core自动协调、网页和签名安装仍缺，未交付棒子。构造库不会启动进程/线程或网络访问。

- `model.rs`：最多8个目标、32条规则；发送方/正文/线路匹配；通知或明确的完整正文；Webhook/Bark/Telegram请求编码与应答判定。不是实际HTTPS客户端。重叠规则对同一消息/目标按首条匹配规则只排一次。
- `queue.rs`：Core阻塞actor独占writer；私有0700目录/0600单链接文件；SQLite独立format1，不改变主库schema6。游标与全部收件目标同事务；256条未决/4096条回执的有界背压。重启中断、失去确认、停用后的迟到回复均不会回队重发。
- `delivery.rs`：真实rustls/Hyper HTTPS、10秒总截止、取消/主管消失关闭socket；无孤立driver、重试或重定向。与固定出口和实际Core Fence组成worker，仍仅在合成回环验证，参见[HTTPS组件](../../docs/VALIDATION-FORWARDER-HTTPS-X1B.md)与[实际进程验证](../../docs/VALIDATION-FORWARDING-WORKER-PROCESS-X1E.md)。
- `authority.rs`：从真实Auth的设备身份、realm和最近登录Owner派生绑定；实际派发检查持久Owner代次、真实业务库和SIM线路。即便客户端具有system.manage，也不能冒充Owner配置。`Binding`本身不是凭证。
- `storage::Handle::inbox_event_head`：首次/重新启用只读当前真实事件尾，不扫描历史正文；需要补发历史时必须另做显式流程。
- `core::forwarding`：[实际Owner管理接线](../../docs/VALIDATION-FORWARDING-CORE-X1C.md)，提供状态、凭据隐藏的规则、有限回执、最近登录下的配置与清理。Core持有被动句柄，默认查询不建目录或启动任务；配置重查保持第一次实际事件起点。
- `worker_ipc.rs`：[实际Root/worker通道与主开关](../../docs/VALIDATION-FORWARDING-WORKER-IPC-X1D.md)，按真实UID分流、有限能力、唯一Claim与实际内容摘要核对。只有Core明确表示尚未受理的锁竞争可有界重查；已取得任务或丢失回复不能归类为可重试。
- `egress.rs`：Root显式选择网卡名/ifindex/源IP/DNS；每个DNS和TCP socket都绑定设备和源IP，无系统DNS、代理、连接池或默认路由回退。Hickory解析有界DNS帧，支持A/AAAA、有限CNAME与同一DNS服务器的截断TCP回退。
- `worker.rs` / `main.rs`：`rustsimhub-forwarder --worker`从有界stdin取得私有配置，独立非Root且有效/许可/环境能力均为0；Root拥有的有限CA包、真实Core UID复核、单任务HTTPS、SIGTERM/SIGINT取消和收尾。没有自行fork、驻留子进程或后台driver；失败/Unknown不自动重发。
- `supervisor.rs`及其子模块：[有限服务管理](../../docs/VALIDATION-FORWARDING-SUPERVISOR-X1F.md)，Root只按需协调/核验固定单元；PID1以独立低权限用户和空能力直接运行`--service-worker`，使用私有只读LoadCredential。真实UID/GID/附加组、五类能力、NoNewPrivileges、整个cgroup和额外进程都需实际核对；Owner/代次/boot/Core实例/能力摘要绑定报告。Root管理动作不常驻。

Core集成应使用 `Binding::from_recent_owner` / `Queue::configure_owner` / `capture_current` / `claim_current` / `check_current_claim`。原始队列API只供可信Core内部状态机，不能映射为未鉴权的IPC。HTTP worker需要受限独立通道，取得一次claim并在实际写应用数据前再次向Core检查，不能拿本地绑定或旧的“允许”布尔值代替。

`Queue::status().enabled`只是持久意图，基础Queue/Core查询的`process_state_verified`仍为false。独立Root报告只能在本机单元/整个cgroup/专用UID进程核对后为true；尚未接入Core/网页，不得把基础查询当成停止证明。完整关闭还必须清理启动触发器及私有凭据，并另做设备冷启动验收。

接口核对参考：

- [Telegram Bot API / sendMessage](https://core.telegram.org/bots/api#sendmessage)：HTTPS、JSON、ok响应、长度和link_preview_options。
- [Bark服务端API V2](https://github.com/Finb/bark-server/blob/master/docs/API_V2.md)：POST /push、device_key/body/title/group。

没有复制或运行Android转发器代码，没有访问真实推送目标。Root已能从新鲜Root网络报告选择明确Wi-Fi/USB接口，蜂窝选择未开放；后续须接Core自动协调及新鲜Root报告、网页、独立账户和签名安装、真实设备关闭/冷启动。有限失败策略、邮件和其他通道、正则/时间段规则仍在总清单。不得给当前worker配置盲目自动重启，必须先由Root撤销旧能力、处理旧任务和检查当前策略。
