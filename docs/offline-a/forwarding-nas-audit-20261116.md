# A3转发/NAS差距表（离线部分，不是全范围验收）

基线 `b15ea3f561bf9e2a1268f8df5b6f0973cc83bbbd`，本轮受测tree
`a9fbeca7ac01734bce22997d61d07cb5e82cbcd8`。准确实现SHA、命令、失败和原始证据见
[交付报告](../deliveries/OFFLINE-A-FORWARDER-20261116.md)。下表的“本轮通过”仅适用于列明的源码/测试范围。
已有实现不重写，原作者和历史交付保持。全部93项动作仍按任务书保留；此表只深化其中20项的证据。
表内`compat/`缩写为`crates/compat/src/`，`Core`为`crates/core/src/`；其余转发模块在`crates/forwarder/src/`，
`tests/email_sender.rs`和`tests/email_compatibility_proposal.rs`在`crates/forwarder/`下。

## 转发12项

| ID | 已核源码/测试入口 | 本轮实际结果与仍缺部分 |
|---|---|---|
| FWD-06 | `crates/forwarder/src/delivery.rs`；`delivery/tests.rs:verified_https_sends_exactly_one_json_request_and_bound_receipt_id`、`real_authority_queue_and_https_ack_commit_once_and_remain_terminal` | 真实本机TCP/TLS、JSON/认证/原ID和队列完成通过；真实Webhook账号送达留主线 |
| FWD-08 | `model.rs:PreparedRequest::acknowledged`；`delivery/tests.rs:provider_json_error_is_not_a_successful_delivery`、`client_errors_rate_limits_and_gateway_errors_with_retry_after_are_single_unknown_attempts` | Telegram编码及ok=false/429/Retry-After等原分支通过，不重试；未访问Telegram或声称公网可达 |
| FWD-09 | `email/mod.rs`、`smtp_candidate.rs`、`worker.rs`、Core `forwarding.rs`；`tests/email_sender.rs` | 已有SMTP实际接线复用；默认18项SMTP库测试、生产feature21项通过，含真实TLS、证书/hostname、七次原claim fence、DATA前后失败/迟到/取消；真实邮箱/安装留主线 |
| FWD-13 | `delivery.rs`及两个`ready_fence_*`新回归；原`queue/tests.rs:disabling_cancels_only_unsent_and_fences_late_result`；Core `forwarding/ipc/tests.rs` | 修复ready回调内部撤权/阻塞后的额外IO；旧生产分别实际失败101，新代码通过；四UID IPC及五种进程场景通过。未重跑历史PID1/systemd完整关闭安装链 |
| FWD-14 | `egress.rs`、`egress/tests.rs`、`supervisor/network.rs`；真实worker进程的wrong_route/wrong_ca | 固定设备/ifindex/源地址、有限DNS/别名与拒绝回退通过；两个进程反例HTTP为0。只用私有回环，没有修改主机路由/普通蜂窝或访问真实USB/Wi-Fi |
| FWD-16 | `authority.rs`、`queue.rs`；Core `forwarding/tests.rs:disabling_via_actual_ipc_revokes_inflight_and_restart_preserves_unknown_and_credentials`；SMTP七阶段 | 原Owner/realm/generation、凭据/不可变claim、关闭/撤权和秘密隐藏通过；真实通道凭据不使用 |
| FWD-17 | `queue/tests.rs:sim_change_cancels_unsent_jobs_and_empty_or_stale_line_does_not_send`、`binary_and_duplicate_candidates_never_enter_outbox` | 合成换卡/换代、二进制及疑似重复隔离通过；没有SIM写操作 |
| FWD-18 | `queue/tests.rs:default_off_has_no_producer_read_and_cannot_claim`；四UID夹具重启关闭状态 | 默认OFF及重开后disabled、Unknown、租约失效保持通过；设备实际冷启动保留主线 |
| FWD-19 | `worker.rs`、`supervisor.rs`、`supervisor/network.rs`；原默认/租约/固定网络拒绝测试 | 已有有限调度/新鲜网络前置复验；当前不把保存ON当作Root授权或进程已启动。获授权的PID1启动、延迟联网和断电后的完整服务组合尚待B/主线隔离集成与设备验收 |
| FWD-20 | `compatibility.rs`、`inspection.rs`、`queue/migration.rs`、`tests/email_compatibility_proposal.rs` | 默认outbox3/worker4、候选outbox4/worker5的API/只读检查和拒绝测试通过；不扩大Root写入表。B的真实3→4迁移/回退/签名组合不冒称本轮已运行 |
| FWD-21 | `queue/tests.rs:bounded_backpressure_keeps_cursor_and_unknown_receipts`、`receipt_capacity_rejects_new_jobs_and_owner_pruning_never_removes_unknown`；原TLS响应/预算测试 | 有界队列/回执、响应、单次IO及预算通过；默认和SMTP各一个2MiB真实ENOSPC隔离测试通过；设备并发性能/功耗/长稳未测 |
| FWD-22 | `crates/forwarder/README.md`、Core `forwarding.rs`、网页`forwarding.js`；两项原Node suite | 文档已纠正“邮件缺实现”的旧阶段误读，页面按email_revision显示功能；Node通过。真实主页/CSP/IndexedDB三宽仍归A4，发行组合归B |

## NAS八项

| ID | 已核源码/测试入口 | 本轮实际结果与仍缺部分 |
|---|---|---|
| NAS-01 | `crates/compat/src/lib.rs`、`session.rs`、`legacy-golden.json`；`tools/nas-adapter/{main,readonly,http}.go` | 原签名/版本/只读状态含义、四个有限查询、来源/身份/未知值边界通过；NAS权限不会升级为Owner或写权限 |
| NAS-02 | `tools/nas-adapter/http_test.go`、`cli_linux_test.go`；`crates/core/src/compat_api.rs` | 本机TLS/CLI及原协议只读分支通过；适配器不是完整旧NAS应用。没有可审核固定旧页面源码，因此原NAS页面本身未复验，需负责人提供准确来源/版本，不以自制页面替代 |
| NAS-03 | `tools/nas-adapter/control/client.go`、`client_test.go`、`request-vectors.json` | 25项Go顶层测试中的10项有限控制测试通过；Prepare/Submit/Operation/Cancel/Renew发出原Core v2格式，用已批准普通Client token，不把NasKey升级为通话权限；对端为本机合成Core回复，非真实电话或NAS |
| NAS-04 | `crates/compat/src/nas_audio.rs`及`nas_audio/tests.rs` | normal-nas-candidate组合58项通过，包含六项候选PCM/原wire向量契约；实际Authority→Permit/Pump，外部PCM为合成。未改Native/AirSIM/声音生产路径；没有旧PCM传输或跨主机声音兼容结论 |
| NAS-05 | `compat/history.rs`、`history/preflight.rs`及原input_matrix测试 | 合成实际SQLite表格式的预检/差异、坏行/未知列/上限/来源/权限拒绝通过；不复制生产NAS数据库 |
| NAS-06 | `compat/history/tests.rs`、`compat/history_recovery_tests.rs` | 重复导入、冲突回滚、源绑定、父子进程中断恢复及旧ID保持通过；父测试真实启动worker，普通suite中的worker入口仍按原ignore保留 |
| NAS-07 | `compat/session/tests.rs:signed_legacy_sessions_are_approved_device_bound_scope_limited_and_revocable`；Go `TestDropAfterMutationPreservesUnknownAndDoesNotRetry`等 | 合成撤权/代次/重启和原恢复ID/Unknown不重试通过；没有真实NAS重启或绑定转换 |
| NAS-08 | 上述Rust/Go套件、`tools/nas-adapter/README.md`、`control/README.md` | 给B明确支持原签名只读桥/另行批准普通Client有限控制/PCM候选契约；不支持“整套旧UI及旧PCM已兼容”的声明。真实NAS端到端及未知旧页面/声音来源仍缺，不能由本机模拟判全通过 |

Go只读15项+控制10项不计子用例、不计无测试CLI包；Rust默认及feature组重叠，不累加成独立测试总数。
原旧NAS来源与许可证沿用 `crates/compat/src/legacy-golden.json` 和已有来源报告；没有将既有Go客户端改名为Rust实现。
本表不更改任何全局动作状态、权重、成员审核或整体 **81.5%**。
