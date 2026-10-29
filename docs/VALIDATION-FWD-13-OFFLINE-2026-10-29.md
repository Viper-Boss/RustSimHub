# FWD-13 并发关闭与迟到响应 — 离线核对（2026-10-29）

范围：只在电脑/WSL 上核对代码与测试；**未做实机**。FWD-13 的验收需要真实棒子上的转发进程与真实递送目标（`needs: device`，执行人 lead），本记录不把它标为完成。

## 各关闭时机与离线证据（`crates/forwarder`）

| 关闭时机 | 期望 | 证据（测试） |
|---|---|---|
| 新消息、尚未认领 | 关闭后不再认领；未发出的排队任务变为 cancelled | `queue::tests::disabling_cancels_only_unsent_and_fences_late_result`、`default_off_has_no_producer_read_and_cannot_claim` |
| 已认领、未联网 | 一开始就取消或丢失监督者：不打开出口套接字 | `delivery::tests::initial_cancel_and_loss_of_supervisor_cannot_open_an_egress_socket` |
| DNS/连接中（**本次新增**） | 未发送（Cancelled）；连接器 future 被丢弃；不进入 TLS 检查 | `delivery::tests::cancellation_during_dns_is_not_submitted_and_drops_the_resolver` |
| TLS 中 | 未发送；套接字关闭；对端从未收到 `POST` | `delivery::tests::cancellation_during_tls_closes_the_owned_socket_without_http` |
| 每道 Core 实时检查（连接前/TLS前/提交前） | 任何一道拒绝都不发 HTTP | `delivery::tests::every_live_fence_can_stop_before_any_http_request`、`actual_owner_revocation_after_tls_prevents_application_data` |
| HTTP 已发出 | 结果为 Unknown，不重发 | `delivery::tests::lost_http_reply_cancellation_is_unknown_and_closes_the_connection`、`real_received_request_with_lost_ack_survives_as_unknown_without_redelivery` |
| 关闭后迟到的回执 | 旧代次 finish 被拒（Stale），已发任务保持 Unknown，不复活 | `queue::tests::disabling_cancels_only_unsent_and_fences_late_result`；Core 拒绝在 worker 侧映射为 Forbidden → `worker::run` 正常退出（`crates/forwarder/src/worker.rs`） |
| 进程崩溃/重启 | in_flight 一律 Unknown，不再认领 | `queue::tests::crash_after_mark_before_ack_is_unknown_and_never_reclaimed` |

设计说明（未改）：配置变更时，已认领但实际未发的任务也记为 Unknown（Core 无法证明 worker 没发），这是保守选择，避免较低信任的 worker 把“可能已发”改写成可重发的失败。

## 本地结果

- `cargo test -p rustsimhub-forwarder`（WSL）：delivery 19/19 通过（含新增）；全 crate 其余失败仅为已知环境项 egress ×5（需要可绑定接口/root 网络命名空间）。
- `cargo clippy -p rustsimhub-forwarder --all-targets -- -D warnings`：无告警。

## 仍需实机（主线）

在棒子上开启转发、对已授权目标逐个时机关闭总开关，核对：旧代次无新递送、已发请求结果准确或 Unknown、关闭后 `rustsimhub-forwarder` 无 PID。需要用户授权真实递送目标与次数。
