# rustsimhub-forwarder

Rust原生短信转发后端的第一段：规则、请求编码、有界持久outbox及真实Auth/Store适配。当前没有daemon、HTTP发送循环、Core wire命令或网页开关，也不会启动任何进程/线程/网络访问；尚未交付棒子。

- `model.rs`：最多8个目标、32条规则；发送方/正文/线路匹配；通知或明确的完整正文；Webhook/Bark/Telegram请求编码与应答判定。不是实际HTTPS客户端。重叠规则对同一消息/目标按首条匹配规则只排一次。
- `queue.rs`：Core阻塞actor独占writer；私有0700目录/0600单链接文件；SQLite独立format1，不改变主库schema6。游标与全部收件目标同事务；256条未决/4096条回执的有界背压。重启中断、失去确认、停用后的迟到回复均不会回队重发。
- `authority.rs`：从真实Auth的设备身份、realm和最近登录Owner派生绑定；实际派发检查持久Owner代次、真实业务库和SIM线路。即便客户端具有system.manage，也不能冒充Owner配置。`Binding`本身不是凭证。
- `storage::Handle::inbox_event_head`：首次/重新启用只读当前真实事件尾，不扫描历史正文；需要补发历史时必须另做显式流程。

Core集成应使用 `Binding::from_recent_owner` / `Queue::configure_owner` / `capture_current` / `claim_current` / `check_current_claim`。原始队列API只供可信Core内部状态机，不能映射为未鉴权的IPC。HTTP worker需要受限独立通道，取得一次claim并在实际写应用数据前再次向Core检查，不能拿本地绑定或旧的“允许”布尔值代替。

`status().enabled`只是持久意图，`process_state_verified`始终为false。Root主管和服务尚未实现，绝不能据本库状态显示“进程已停止”。完整关闭还必须停服务、所有子进程和启动触发器；只有实际PID/单元状态核对后才能通过用户要求的关闭验收。

接口核对参考：

- [Telegram Bot API / sendMessage](https://core.telegram.org/bots/api#sendmessage)：HTTPS、JSON、ok响应、长度和link_preview_options。
- [Bark服务端API V2](https://github.com/Finb/bark-server/blob/master/docs/API_V2.md)：POST /push、device_key/body/title/group。

没有复制或运行Android转发器代码，没有访问真实推送目标。后续须完成HTTPS/路由门禁、Core命令与worker IPC、独立服务和关闭证明、网页及签名安装；邮件和其他通道、正则/时间段规则仍在总清单。
