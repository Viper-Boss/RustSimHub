# D23 原执行器的 T5 结果

2026-11-01（Asia/Shanghai）。承接[原 Core actor 接线](core-native-recovery.md)，新增显式 `network_trial::provision::recovery_v1(auth, AuthorizedRecoveryRequest)`。消费真实 Auth 私有能力及同正文解码结果，复用唯一 dispatcher、原 actor 和 SQLite 回执，输出冻结的 T5 `OperationV1` / `LookupV1` 或有限错误。

仍须 `recovery-execution` feature 和原 provision 显式启用。没有 A verifier 时仍拒绝，没有新增监听器、安全会话、前端 provider 或默认恢复服务。调用方须保留实际 accepted RecoveryChannel，并由 A 适配器按共同接口加密/路由结果。本包只固定**应用结果**，没有冻结加密 wire 或选择密码套件。普通 Wi-Fi 命令、USB、锁、电话、WWAN/APN 门禁不变。

## 输出依据

字段逐项沿用 `docs/team/interfaces/t5-offline-v1.json`：`source` 固定为 `core`，请求 UUID / action / trial 来自实际已核请求及原回执。没有 SSID、口令、设备公钥、grant/Owner 标识、指纹、原始正文或后端错误。DTO 只有 Serialize 和私有字段，没有 Deserialize、载荷构造器或从外部 RecoveryReply 转成功的公开转换。

| 实际证据 | T5 输出 |
|---|---|
| 本次新建原 actor | starting，authoritative=true，settings_saved=false；不声称已 Ready |
| 原回执 starting/connecting/ready，且同原活跃 actor、完整绑定、同状态、当前原 Auth/trial 门禁和原期限均有效 | 对应状态，authoritative=true，settings_saved=false，rollback_verified=null |
| 满足同样条件的 saving | saving，authoritative=true，settings_saved=null，rollback_verified=null；保存调用仍在途，不能断言没有持久改动 |
| 原 actor 已核后端保存且终态记账成功，或持久 saved/1 回执 | saved，authoritative=true，settings_saved=true，rollback_verified=null，expires_in_ms=0 |
| 原 actor 已核精确补偿且终态记账成功，或持久 rolled_back/0/1 回执 | rolled_back，authoritative=true，settings_saved=false，rollback_verified=true，expires_in_ms=0 |
| Reserved/Claimed/Rejected/Unknown，找不到对应活跃 actor、普通 owner、期限已过、原连接失效、权限失效或门禁忙 | unknown，authoritative=false，两个结果证据和期限均为 null；不宣布终结 |

活跃状态的剩余毫秒取**原 session 与私有 trial 的绝对 deadline 最小值**，最大 40000；不根据新连接、查询时间或 grant 再生窗口。Storage 最小新增只读 `same_trial_as` 比较 realm、设备、Owner/代次、grant/代次和 trial；Auth 私有 trial 转用它匹配快照。它们不返回权限，原活跃门禁仍单独检查；相同公开请求/trial 字符串不能跨库冒认。

这是某一观察时刻的快照；返回 Ready 不替代执行器之后的独立 Confirm/保存前后门禁。快照不签发能力，终态历史也不能恢复原 Claim。

## 查询与错误

查询目标仅从本次已认证正文取得。找到真实回执才返回 `lookup=found`，operation 的 UUID 保持原目标；真实无记录返回 `not_found, operation=null`，不是“未执行”的证明，不清除前端不确定记录、不自动发送。

仅 QueryOperation 的解码、只读查库和映射整体设 3 秒截止。超时返回 `{contract:1,code:'deadline'}`；已经排队的阻塞解码可能在池恢复后完成，只能完成只读工作，不能启动后端。写调用不套用这个查询截止；本地丢回复仍由原 actor 有界排空，不重发、不新建 UUID。

错误严格两字段 `{contract:1,code}`。Busy→busy；InvalidInput→invalid_input；Forbidden/Replay→permission_denied；Expired→deadline；Io/Storage→unknown；Entropy/CorruptIdentity→unavailable。现有 Auth 错误合并了具体存储错误，因此不臆造 revoked/not_preapproved/pin_mismatch 或把存储失败称 not_found。错误不含远端正文。ReadStatus/Disconnect 的应用分派仍未开放，本接口只支持当前已实现的开始/查询/确认/取消；transport close 保持独立且不声称取消成功。

## 验证和验收边界

真实原 actor/Auth/Identity/SQLite 与同正文 dispatcher 测试覆盖完整保存/取消、部分保存/补偿失败、活跃 Ready/Saving、原期限、普通 owner、重连、同公开 trial/UUID 的不同库绑定、孤立 Reserved/Claimed/Ready、Unknown 不升级、实际 Store 重开后的 saved/Unknown 历史，以及真实单阻塞工作池队列导致查询 3 秒超时。外部 A verifier 与 NM backend 仅 cfg(test) 合成，没有复制状态机。

Rust `--nocapture` 日志中的 `T5_CORE_RESULT` 只输出合成结果 DTO。[校验脚本](core-recovery-t5-contract-check.mjs)直接调用**未修改的** `configurator/src/contracts.js`，检查这些真实 Rust 序列化结果；不是手写一份期望 JSON 当互操作证据。没有运行浏览器 BLE 或真实设备。

初稿额外证据审查发现 Saving 被映射为 settings_saved=false；冻结字段校验允许布尔/null，格式通过不代表这项证据准确。将真实原 actor 的外部 commit 挂起后，新断言实际得到 Bool(false) 而不是 Null 并失败；修正仅将 Saving 的该字段保留 null，同一 Rust 断言及跨语言脚本均保留，不放宽前端校验。此为本次新映射初稿的问题，不称旧生产漏洞。

完整命令、精确代码 HEAD、数量及日志摘要见[交付报告](../deliveries/member-d-core-recovery-t5-results.md)。保持默认 schema6，无新 DDL、依赖或生产安装。A 实际会话/host/真实 UID 整装、ReadStatus、前端安全 provider、Root/发行 schema9 兼容与 NM/硬件联验仍未完成。完整阶段仍 21/26、剩 D22–26 五步，固定 P0–P7 约 80%。
