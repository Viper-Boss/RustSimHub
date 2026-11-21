# A3：短信/电话逐项审计与原实现定向复验

本包完成A0/A3中18个原ID的更细证据核对，不代表18项全部实现或验收。
没有新产品实现，不将原功能重写、改名或重新记完成；准确缺口见[18项表](../offline-a/sms-call-audit-20261116.md)。
账号yh1030，同仓库本人`codex/offline-a-sms-call-audit-20261116`分支提交main，由负责人审核，不自合并。

## 固定来源

- 起始main：`6e1cd19bc9dd05fd150157c422590c1a801cc0b0`，包含任务书产品基线`2ae840fbffff04691ccf8cedd49d554093dd4032`。
- 实际受测tree：`8e353f3fb412874a03e2808779b75b2140515a49`，完整Git归档SHA256 `6729d68a9402233ad02defa96c8a02dfa5b3856c0f92136c6ba2f5f0a3d6ff4b`。
- 该tree为本人PR263与main的组合；本包仅新增两份文档，不依赖PR263的网络面板产品变更。已用Git对象差异核验Core/Storage/Auth/Domain/Policy/Wire/Media、app.js、phone.js、call-test.js、sms-management.js、event-sync.js、test-center.js及相关短信/电话测试与上述main相同。
- 当前完整文档HEAD另见PR正文，避免报告自包含自身提交SHA。没有不存在的implementation_head；此包为证据/缺口交付。

## 本轮真实命令与非空用例

WSL2 Ubuntu24.04、普通UID1000，独立Linux原生source/target/TMPDIR、固定Rust/Cargo1.98.0、原Cargo.lock，全部`--locked --offline`。
每条最终命令检查实际非零用例，13组共53通过/0失败/0忽略；不同过滤不重叠，不把过滤掉的用例计通过。

| 命令 | 实际结果 |
|---|---|
| `cargo test -p rustsimhub-storage --locked --offline inbox_content_tests -- --nocapture` | 0；4通过 |
| `cargo test -p rustsimhub-storage --locked --offline inbox_recovery_tests -- --nocapture` | 0；4通过 |
| `cargo test -p rustsimhub-storage --locked --offline outgoing_recovery_tests -- --nocapture` | 0；6通过，含实际子进程退出边界 |
| `cargo test -p rustsimhub-storage --locked --offline calls_tests -- --nocapture` | 0；8通过 |
| `cargo test -p rustsimhub-storage --locked --offline call_reobserve_tests -- --nocapture` | 0；4通过 |

Core下列六条共同前缀为
`cargo test -p rustsimhub-core --locked --offline --features product-providers-candidate,email-production-candidate --bin rustsimhub-core`，每条附`<filter> -- --nocapture`：

| filter | 实际结果 |
|---|---|
| `observer::` | 0；17通过，真实MM属性解码/私有DBus及Storage，不是公网或真实Modem |
| `ringing_rejection_` | 0；2通过 |
| `private_voice_` | 0；3通过 |
| `session_continuation_preserves_live_call_lease_and_media_binding` | 0；1通过 |
| `simultaneous_answer_requests_and_normal_shutdown_have_one_owner_and_one_cleanup` | 0；1通过 |
| `terminal_reobservation_ignores_unrelated_sms_notices_but_call_retirement_keeps_new_history` | 0；1通过 |

其余两条共同前缀为
`cargo test -p rustsimhub-core --locked --offline --features product-providers-candidate,email-production-candidate,remote-media-tests --bin rustsimhub-core`，每条附`<filter> -- --nocapture`：

| filter | 实际结果 |
|---|---|
| `actual_normal_core_denies_missing_scope_bad_lease_or_expiry_before_media_connect` | 0；1通过，七种错误/到期入口在media连接前拒绝 |
| `local_media_stop_does_not_create_another_call_or_renew_the_original_lease` | 0；1通过 |

同受测tree的27组非热点Node、Edge71/严格Clippy/fmt、实际主页和控制台三宽结果已有独立
[PR263组合报告](OFFLINE-A-NETWORK-WEB-20261116.md)，不重复计入53项Rust或冒充本包新实现。
本包不改产品代码，不再为文档重复跑全仓/ARM或未授权热点专项。

## 原失败、未完成与交接

1. 第一次Core筛选使用`--lib`，实际八条均0用例，尽管Cargo退出0也全部拒绝计通过。原私有runner的passed字段不能单独证明用例执行；另存`core-zero-tests-rejected.json`，之后改为实际daemon `--bin rustsimhub-core`。
2. 第二次在生产feature组合下六组实际25通过，但两项远程测试仍为0，因为需要原`remote-media-tests`。该轮runner已要求非空并实际退出1；保留原日志，后续仅补两项所需原feature，实际各1通过。没有删断言或改测试适配产品。
3. 本次没有修改生产行为，因此不虚构“旧代码失败/修复后通过”的对照。真实数据库事务失败、合成DBus权限/单次副作用及实际子进程退出已有原断言。
4. 真实SMS ENOSPC、SIM满对端矩阵、委派视图全组合、质量计数/长通话证据仍需进一步核对/补缺；更未运行设备/真实NAS、实际PDU/运营商发送、15分钟听音、TURN公网或新签名候选。
5. B可复用上述精确源码和实际命令，独立验证生产profile/候选/迁移；本报告不生成替代build.json的“绿色清单”，不签名/部署。

原始日志/私有runner与资格核验摘要归档32项、25,820字节，ZIP CRC核验通过，SHA256
`77525716cc4d707afd35a4a8dac7357b7b0a0658f805f8e4c301bce18efe51b3`。
只含合成数据库/测试日志，不含用户真实短信、SIM、凭据、生产密钥或设备日志；原始归档留任务忽略目录，不上传二进制。
新文档AGPL-3.0-only，原实现来源/许可保持，AI辅助仅用于本次审计/测试与文字整理。

下一步继续其余27项NKN/统计/远程逐ID核验，以及已确定的网络限额Unknown持久原ID缺口；
Core固定用途统计签名/有界Root候选遵照负责人最新提案审核边界，不开放未证明的confirmed或延长300秒权限。
不改全局看板、审核记录/成员权限/作者，不接管B/蓝牙/AirSIM/native/media生产文件。
热点专用工作按最新任务书延期；整体已验收进度保持 **81.5%**。
