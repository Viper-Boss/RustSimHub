# A3：短信8项与电话10项逐动作证据

按任务书原ID保留18项，没有新增动作、改变权重或将离线测试当现场验收。
产品基线为main `6e1cd19bc9dd05fd150157c422590c1a801cc0b0`。
本轮实际测试来自组合tree `8e353f3fb412874a03e2808779b75b2140515a49`；
Core/Storage/Auth/Domain/Policy/Wire/Media及本文引用的短信/电话网页和测试，与上述main逐字节一致。
组合tree含PR263的网络面板修复，本表不是把该PR未合并的网络功能算入main。
命令、非零用例计数及原失败见[交付报告](../deliveries/OFFLINE-A-SMS-CALL-AUDIT-20261116.md)。

分类“已有实现、离线部分已复验”只覆盖本行写明的断言，不表示整项完成；
“缺证据/需外部输入”不等于没有代码。未实施的部分明确保留。

| ID | 分类、源码与实际证据 | 未证明或仍需执行 |
|---|---|---|
| SMS-01 | 已有实现、离线部分已复验。`crates/storage/src/inbox.rs`、`inbox_management.rs`；`inbox_recovery_tests::pagination_skips_tombstone_gaps_and_reopen_keeps_ids_and_event_sequences`比较不同页长及重开后的事件/ID；原`event-sync`、`daily-sms` Node通过 | 两个真实独立客户端同步、实际设备权限/刷新组合未验收；分页库测试不是两个真实浏览器 |
| SMS-02 | 已有实现、离线部分已复验。`inbox_recovery_tests`四项：墓碑跳页、事件写失败不前移、读取失败不消费cursor、旧/错误realm拒绝与恢复换代；同源`event-sync` Node通过 | 真断网离线追赶、跨设备恢复与长时间乱序未操作 |
| SMS-03 | 已有实现、离线部分已复验。`core/src/observer/checks/content.rs`三个原测试经真实MM属性解码→实际Storage，含中文/组合Unicode/emoji、4096字节边界、二进制、缺字段/错类型、不完整状态拒绝；`storage/src/inbox_content_tests.rs`四项重开/重复/恢复通过 | MM已组装属性不是自行重写PDU解码；真实运营商分片、编码与SIM样本未测 |
| SMS-04 | 已有实现、部分缺证据。`observer/checks.rs::reused_source_and_content_change_preserve_new_incarnations`、`checks/lifecycle.rs`对象退休/迟到/列表失败测试通过，保留确认行与新incarnation | 未实际制造SIM满；没有将MM属性异常/对象消失当SIM满拒绝证据。SIM满有限对端矩阵仍需专项核对/补缺 |
| SMS-06 | 已有实现、离线部分已复验。`storage/src/outgoing_recovery_tests.rs`六项：事件/permit落盘失败、原ID Unknown恢复、六个授权边界实际子进程退出、备份恢复隔离；`observer/checks/sender_recovery.rs`重启原结果查询、不再次Create/Send通过 | SQL触发器故障不是真实ENOSPC；本轮未新增SMS真实填满文件系统试验，不沿用转发ENOSPC为短信证据 |
| SMS-07 | 已有实现、离线部分已复验。`observer/checks.rs::sender_peer_proves_one_send_and_no_retries_after_errors_swaps_revocation_or_timeout`本机DBus八种模式及原结果查询；`checks/sender_recovery.rs`正向证明恢复通过 | 合成MM不是运营商投递证明；真实发送、网络失联及对账留负责人 |
| SMS-08 | 已有实现、离线部分已复验。`inbox_content_tests::restored_backup_keeps_duplicate_edges_and_tombstones_but_quarantines_old_owner`和`outgoing_recovery_tests::backup_restore_quarantines_old_prepared_and_preserves_submitted_receipts`通过；`inbox_management.rs`逐次checked_owner源码已核 | 委派客户端各视图、恢复后新Owner重新授权的完整网页矩阵尚缺逐项证据；不声称旧授权恢复有效 |
| SMS-09 | 已有入口、仅离线准备。原`web/app.js`、`test-center.js`、`sms-management.js`及daily-sms/sms-management/test-center等Node，实际主页与控制台三宽装配通过；固定来源可交B | 没有新签名候选或双端真短信终验；本报告本身不生成B的build/profile manifest |
| CALL-01 | 已有实现、离线部分已复验。`core/src/calls.rs`及`calls_checks.rs`两项`ringing_rejection_`、`private_voice_`与单次收尾；`storage/src/calls_tests.rs`保留历史/Unknown通过 | 双方真实先挂断与运营商时序未操作；真实媒体收听未验收 |
| CALL-02 | 已有实现、离线部分已复验。`calls_checks.rs::private_voice_dial_timeout_never_restarts_and_tone_timeout_never_repeats`实际私有DBus计数断言重复原ID只发一次tone；daily-phone Node通过 | 菜单音与真实DTMF效果留现场，不用合成DBus返回冒充 |
| CALL-03 | 已有实现、离线部分已复验。`private_voice_only_one_client_answers_and_revocation_cleans_up_only_owned_call`及`simultaneous_answer_requests_and_normal_shutdown_have_one_owner_and_one_cleanup`，两个合成客户端竞争、唯一控制者与清理通过；Storage唯一permit通过 | NAS适配的独立证据另见FWD/NAS表；本轮没有真实NAS/多浏览器联合电话，不把普通Client测试等同NAS部署 |
| CALL-04 | 已有实现、离线部分已复验。上述private_voice撤权、租约到期不误报idle；daily-phone/call-test/session Node的退出、迟到与状态隔离通过 | 未让实际手机后台挂起、关闭真实通话页面或听音；不修改受保护声音路径 |
| CALL-05 | 已有实现、离线部分已复验。`calls_tests::restart_and_sim_change_quarantine_operations_and_control_records`，异常重开保留Unknown；`calls_remote_media_tests::local_media_stop_does_not_create_another_call_or_renew_the_original_lease`通过 | 本机Host停止契约不是真实Media崩溃/音频欠载恢复；后者仍需受保护路径所有者与设备验证 |
| CALL-06 | 已有实现、离线部分已复验。`calls_checks::session_continuation_preserves_live_call_lease_and_media_binding`原Auth签名挑战续期后原call/lease不变、Create/Start仍各一次；session Node通过 | 短时合成测试未等待15分钟，不是跨15分钟实际听音。Root最长300秒政策未改 |
| CALL-07 | 需补证据/交B。`calls_remote_media_tests`和原电话状态有有限状态/lease输出；本轮只核接口，未新增质量或长通话测试 | 未证明原质量计数完整、长通话负载/丢包/欠载采样及预算；不靠短测试计通过，不动native/media生产实现 |
| CALL-08 | 已有实现、离线部分已复验。`storage/src/call_reobserve_tests.rs`四项真实SQLite：跨SIM来源碰撞、并发终态重复、失联重开、事件写失败回滚；Core终态重观察忽略无关SMS测试通过 | 真实MM对象生命周期/重复通知现场仍留负责人，测试不创建真实电话 |
| CALL-09 | 已有实现、有限契约已复验。原`calls_remote_media_tests`开启`remote-media-tests`后两项实际执行：错scope/lease/到期在media连接前拒绝，停止原host不新建Call/不续原lease | 无真实TURN/NAT/公网切网、无UDP中继/听音验收；两个拒绝/停止测试不证明整条远程链可用 |
| CALL-10 | 已有入口、离线部分已复验。daily-phone/call-test/incoming-call-test/test-center Node及实际主页装配；原结果/Unknown按原ID核对，未发新真实拨号 | 最终10010或真人来电/拒接/听音留现场；一键入口存在不等于全部按钮场景真机通过 |

本轮53项Rust来自五组Storage与八组Core的非空筛选；不把八条`--lib`零用例或缺feature的两条零用例计通过。
未新增产品代码/协议/迁移/权限；没有虚构旧实现失败的负对照。SQL故障/合成DBus/实际子进程退出分别记录，不互相替代。
此前48项表加本表18项，共66项有更具体入口，仍有27项NKN/统计/远程逐项审核待完成；**66项不是已完成数量**。
A包真实签名/提交、原ID网络限额持久恢复等缺口继续推进，热点专用部分延期。
整体已验收进度保持 **81.5%**。
