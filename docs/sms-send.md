# SMS 发送边界（P2）

核心仅在显式 `--serve-messaging SOCKET ALLOWED_UID STATE_DIR IDENTITY_DIR` 模式开放发送；
`--serve-authenticated`、`--serve-observing` 均拒绝三个发送相关命令。现阶段只有受限本地 Unix socket。

## 接口

v2 `prepare_sms(token,line_epoch,idempotency_key,recipient,text)` 保存 Prepared，
`submit_sms(token,operation_id)` 显式提交，`sms_operation(token,operation_id)` 返回状态并尝试只读对账。
三者要求当前活跃 `sms_send` 会话；principal ID 来自会话，不能由请求注入。
只能操作本人创建的发送记录；收件历史仍独立要求 `sms_read`。
响应只有 operation UUID、line epoch、state，不包含号码或正文。

CLI：`prepare-sms SOCKET SESSION_FILE PRIVATE_DRAFT_JSON`，草稿为
`line_epoch`、`idempotency_key`、`recipient`、`text` 四字段的 JSON；必须为本用户拥有的 0600 文件，父目录 0700。
`submit-sms SOCKET SESSION_FILE OPERATION_ID` 和 `sms-operation SOCKET SESSION_FILE OPERATION_ID`。
令牌、号码和正文不放 argv；号码 3..32 字节，可选首位 + 后只允许数字；正文 1..4096 UTF-8 字节且不含 NUL。
示例号码/内容仅用于模拟，真实查询输入和回复只留私有文件。

## 持久提交

SQLite v4 添加 `send_attempts`，保留旧 inbox/operation/event/cursor。
单写者事务同时校验 principal 的活跃状态、完整快照、generation、scope，及当前 ready line/backend/SIM 指纹。
执行硬件前先提交 Submitting + event + creating。失败不返回可复用许可。
Create 仅接受 number/text，得到路径后读取 Sms 类型、状态、号码、正文、空二进制载荷并与草稿比较。
随后保存路径及观察 revision，并在另一个事务将 created 变为 sending；此许可只能消费一次。
其间 SIM/owner/连接变化、消息路径删除复用、事件提示溢出、监听器退出、客户端撤销都会关闭门禁。
Send 前再次核验新鲜硬件快照和会话。发送 actor 串行执行，队列最多 8；不遍历 outbox 自动提交。

Send 的 void 返回只说明队列接受；只有同一绑定对象 State=SENT 且内容、SIM、当前权限仍符合要求，才保存 Submitted。
Submitted 不表示接收方收到。Create/Send 失败或超时、排队后状态未确认均保留 Unknown；再次 submit 不发送。
同运行连接的 Unknown 可通过状态查询确认 SENT；新的进程或连接绑定不继承旧对象的可信性。
重启将 Submitting 恢复为 Unknown，Prepared 也不会自动提交。恢复备份沿用既有撤销所有授权与 Unknown 规则。

## 实际约束

SQLite 提交和基带操作无法形成跨进程原子事务：权限线性化点为持久 sending 许可。
撤销后会检查并停止尚未调用的动作，但无法撤回已经进入 ModemManager 的 Send。
watch revision 保护已观察到的路径删除/重用；信号遗漏下相同路径和完全相同内容的物理对象身份无法绝对证明。
因此连接/进程重启不依靠路径或正文相同确认旧操作，不自动重发。ModemManager 方法调用仍有极短物理换卡竞态。

此版不删除已创建但取消/未知的 modem 草稿；必须后续单独做保留策略和明确清理，不能通过重建草稿绕过 Unknown。
长短信分段/投递报告、归档/已读/tombstone、磁盘满实机、SIM 存储满、中文/emoji 实机发送尚待验收。
使用 [ModemManager 1.24 Messaging](https://github.com/linux-mobile-broadband/ModemManager/blob/1.24.0/introspection/org.freedesktop.ModemManager1.Modem.Messaging.xml)
及 [Sms](https://github.com/linux-mobile-broadband/ModemManager/blob/1.24.0/introspection/org.freedesktop.ModemManager1.Sms.xml) 固定类型接口，不开放任意 D-Bus/AT/shell。

当前实现源代码和失败路径测试已落地；本轮编译、CI 与实机结果见 VALIDATION-SMS-SEND-P2.md。
