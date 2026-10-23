# 通话控制与租约（P3 初始）

实现位于 domain/calls、storage/calls 与 core/calls。使用
[MM 1.24 Voice](https://github.com/linux-mobile-broadband/ModemManager/blob/1.24.0/introspection/org.freedesktop.ModemManager1.Modem.Voice.xml)
及 [Call](https://github.com/linux-mobile-broadband/ModemManager/blob/1.24.0/introspection/org.freedesktop.ModemManager1.Call.xml) 的固定接口。
只允许 CreateCall、Start、Accept、Hangup、SendDtmf；没有任意 D-Bus/AT/shell 接口，不做多方/呼叫转移或紧急呼叫能力声明。

## 模式与接口

`--serve-call-observing SOCKET UID STATE IDENTITY` 只观察呼叫；不能 prepare/submit/renew 控制。
`--serve-telephony SOCKET UID STATE IDENTITY` 显式开放短信及通话控制。
原 authenticated、observing、messaging 模式保留原边界；通话历史可在鉴权只读接口查询。

calls/call_detail/call_events/call_status 要求 calls_read。prepare_call/submit_call/call_operation/renew_call 要求 calls_control。
principal 从会话推导，不能注入。操作只能由原 principal 查询/提交。
查询返回 UUID、line、方向、映射状态、modem state/reason、availability 与 media_state=none；不公开号码、路径或 SIM 指纹。
通话 Active 仅表示控制状态，不能当作声音已正常。

prepare_call 输入 line_epoch、idempotency_key、action(dial/accept/hangup/dtmf)、call_id(可空)、payload。
拨号 payload 是 3..32 字节数字号码，允许首位 +；DTMF 仅接受一个 0-9/A-D/*/#；其他动作 payload 为空。
普通请求不能使用 __ 开头的内部清理键。紧急服务受限标志为 true 时拒绝正常拨号。

CLI 用 `prepare-call SOCKET SESSION PRIVATE_DRAFT_JSON`，号码不放 argv。
`submit-call SOCKET SESSION OPERATION_ID PRIVATE_LEASE_FILE|none normal|hangup`；hangup 提示只能用于真正的挂断操作。
`call-operation` 查询操作；`save-call-lease PRIVATE_OPERATION_JSON NEW_PRIVATE_LEASE_FILE` 保存收到的租约；
`renew-call SOCKET SESSION PRIVATE_LEASE_FILE` 续租；calls/call/call-events/call-status 查询。
所有会话、草稿和租约文件遵守私有 0700/0600 所有权与无符号链接检查。

## 持久动作与控制权

SQLite v5 保存 calls、call_events、call_operations；迁移保留 v1-v4 数据与游标。
拨号动作先提交 Submitting/creating；CreateCall 后验证对象方向/状态/号码，绑定 call UUID、路径和观察 revision，
再次检查活跃 principal/generation/scope、当前 SIM/line/backend 和租约，再持久提交一次 permitted，随后调用 Start。
接听、挂断和 DTMF 同样先保存许可。不确定创建/拨号/接听/DTMF 不自动重试。

返回 Submitted 依据 Dial 已进入拨号/活动状态、Accept=Active、Hangup=Terminated；不表示接听方收到声音。
DTMF 只有方法成功响应才 Submitted，不证明远端菜单收到音调；丢失响应保持 Unknown。
同进程/连接可只读对账 Unknown 的可观察动作；DTMF 不凭 Active 状态确认。

服务端独占租约 30 秒，建议客户端每 10 秒续期；receipt 是 fencing 信息，不是 bearer credential。
校验 call UUID、随机 boot instance、SIM line、服务端 generation、原 principal 完整快照与服务端单调期限。
输入 expires_in_seconds 不延长服务端期限。并发接听只有一个成功；别的客户端不能续期或挂断已拥有通话。
租约过期不等于基带空闲；已有外部通话阻止新拨号，不会自动接管/挂断原有 Go 服务的通话。

## 观察、失效与清理

每个 backend 首次 ListCalls，之后读 Calls 属性及目标属性，消费唯一 owner 的 CallAdded/Deleted、StateChanged 和属性信号。
最多 64 个呼叫对象，修复周期 5 秒，整轮 6 秒；方法最长 5 秒。丢失、未知/错误、SIM 或 owner 改变时标为 Reconciling，不能伪造 Ended。
路径删除重用或号码/方向变化创建新历史 incarnation。事件游标沿用 db epoch，并在恢复备份后要求重新读取。

挂断有 2 容量优先队列；普通控制 8。待处理挂断先于普通队列，拨号下一阶段发现待挂断会停止继续推进。
优先队列不能撤回已进入 MM 的方法；当前有界调用/查询仍可能占用数秒。

过期或撤销后，仅清理本核心本次创建/接听的对象，使用原始操作关联和当前 live binding 的内部一次挂断许可。
清理失败/超时不假装结束，不重复清理，不开放新拨号绕过仍活动的通话。
正常关闭先给控制器 15 秒清理，再停止观察器、关闭 DB；不能保证 SIGKILL/断电时挂断。

core/连接重启后清空租约，未完成动作 Unknown，旧呼叫记录 Reconciling；现存活动对象作为外部/未接管呼叫阻止拨号。
跨重启对象身份尚未达到安全自动接管的证明要求，因此此版不自动挂断重启后发现的孤儿/外部通话。
SQLite 与基带无法跨进程原子提交，仍有极短物理换卡竞态；权限线性化点是持久许可，不撤回已进入 MM 的动作。

真实拨号/音频/来电/VoLTE、生产非 root 权限、重启孤儿管理及最终 UI 还需各自实机验收。当前验证见 VALIDATION-CALLS-P3.md。


## 一键来电需要的当前目录

call_status.live_calls是刷新成功后的当前目录摘要，最多64条、按ID排序，目录不可用时为空；只含既有CallSummary，不含号码或ModemManager路径。调用仍要求calls.read，不授予接听权。历史calls分页不适合判定可接听对象；客户端必须拒绝reconciling和不确定目录。实际接听仍使用原prepare_call/submit_call对象/线路/租约门禁。见[P3L](VALIDATION-ONE-CLICK-INCOMING-P3L.md)。
