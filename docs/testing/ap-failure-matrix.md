# C21 · AP故障覆盖盘点与C22/C23测试接入方案

日期：2026-11-01。成员yh1030 / member-c；固定main `9c8d36b4b5959cd441f2bc181ff612c96cea5ac5`。
前一UPD-07包PR #143已由负责人合并，本PR开始AP-04离线包的第一阶段。
范围是C21来源、实际接口、10类case及最小失败样例规格；C22/C23代码和C24执行结果尚未交付。

本页只静态检查现有代码。源码中存在测试、下列期望结果和命令方案均不能记作本轮执行通过。
完整来源/测试目录和静态校验结果随本轮交付资料包提供，精确HEAD/命令/未运行项见[成员报告](../deliveries/member-c-nas.md)。

## 接手规则与本轮范围

调用既有station_trial::run和station_commit::run，不新增配网器、生产trait、权限、期限或另一个结果状态机。
正式case ID严格取自[冻结清单](https://github.com/Viper-Boss/RustSimHub/blob/9c8d36b4b5959cd441f2bc181ff612c96cea5ac5/docs/team/interfaces/t5-offline-v1.json#L60)。
只能在预定的两个t5_matrix.rs测试文件及既有tests.rs最小mod接入中实现后续测试，
Core/NM/执行器/生产期限只读，外层确需新接入时先交最小提案。

本PR请审核：

1. 10个稳定case的覆盖层级及所列缺口；共用同一真实run与有限外部后端，不能靠case名称伪称密码/SSID实测。
2. caller_dropped按watch关闭但run仍被持有并await执行收尾；完整任务abort和掉电不在此离线保证内。
3. stale_completion以真实Gate/取消控制流与既有外层Intent/Journal/Core证据分别记录；不能只靠Mock代次状态自证跨代次拒绝。
4. 后续只增加获分配的cfg(test)接入；无新依赖、不延长window/租约/清理预算、不删除失败断言。

## 最小失败样例规格（本PR未执行）

以下是测试夹具输入/动作与断言，不是新产品HTTP/IPC DTO或伪造Outcome。
所有凭据/状态均为合成；必须在C22/C23用真实run返回值与实际调用trace验证。

| 样例 | 注入点与输入 | 原run期望及必须观察的调用 |
|---|---|---|
| cancel_before_start | window=1秒，watch初值true；或调用前丢弃最后sender | Err(Denied)；begin/usable/restore/consume/persist/finish/release均0；与预检后并发取消区别 |
| partial_persist | 初始gate/guard通过，consume一次，begin/usable成功，persist先记录本次副作用再Err(Io)，restore_ap也Err(Io) | Ok(CommitOutcome::Unknown)，persist=1、restore=1，finish=release=0；不能靠先清空副作用获得通过 |
| caller_dropped during persist | Notify确认persist已进入后关闭sender；测试仍持有并await原run | 恢复证实才RestoredAp，否则Unknown；保留一次保存及清理调用事实 |
| cancellation after finish entered | Notify确认finish已开始，发送stop=true后允许实际finalizer和guard.release成功 | 可以Saved；不得丢弃清理或强制猜测回退，persist/finish/release各1、restore=0 |
| stale_completion | 旧owner generation/原trial不匹配的取消或回执样本 | 外层实际校验拒绝的证据独立列出；两个低层run没有发布新事务回执的接口，不能在Fake里编写一个“通过拒绝”状态机 |

partial_persist与当前简单fail=persist不同：后者在通用step返回Err，并没有先标记外部保存副作用。
现有NM假D-Bus测试已有相关更强覆盖，仍不能等同真实NM/物理恢复或本轮新执行。

## C21 · 实际接口、阶段与调用顺序

来源main `9c8d36b4b5959cd441f2bc181ff612c96cea5ac5`。这是对真实生产代码的只读说明，未实现新接口或状态机。

| 层级 | 已有接口及责任 | 不能据此推导 |
|---|---|---|
| trial | [StationNetwork](https://github.com/Viper-Boss/RustSimHub/blob/9c8d36b4b5959cd441f2bc181ff612c96cea5ac5/crates/setup-ap/src/station_trial.rs#L35)：begin/usable/restore_ap；StationProvider创建会话；TrialGuard核验隔离；Gate核验现行权限 | 不保存配置，不提供互联网探测或发布新回执 |
| commit | [CommitNetwork](https://github.com/Viper-Boss/RustSimHub/blob/9c8d36b4b5959cd441f2bc181ff612c96cea5ac5/crates/setup-ap/src/station_commit.rs#L13)追加persist/finish，CommitAudit.consume消费精确确认；CommitProvider创建同一AP下会话 | 静态always-allow Audit不是Owner授权；persist成功不是最终Saved |
| 外层AP | [Gate/Guard](https://github.com/Viper-Boss/RustSimHub/blob/9c8d36b4b5959cd441f2bc181ff612c96cea5ac5/crates/setup-ap/src/transaction.rs#L41)和原transaction::run管理AP激活/回退/隔离生命周期 | 原AP RolledBack不等于station试连UsableAndRestored |
| 身份与回执 | [Intent/cancels](https://github.com/Viper-Boss/RustSimHub/blob/9c8d36b4b5959cd441f2bc181ff612c96cea5ac5/crates/setup-ap/src/channel/station.rs#L93)、Journal、executor、Core复核binding/owner generation/trial/确认和结果持久化 | 低层run没有代次/发布能力，不能自行证明完整跨代次拒绝 |

### Trial真实顺序

window有效 → 初始stop/sender检查 → Gate（3秒）→ TrialGuard（4秒）→ work内Gate/Guard → begin →
循环Gate/Guard/usable（不就绪等待150ms；整个work受window限制）→ 就绪后Gate →
无论work失败/取消均await restore_ap + TrialGuard（共8秒）→
只有usable、restored、stop仍有效且最终Gate（3秒）通过时返回UsableAndRestored；
恢复已证实但前述成功条件不足为FailedAndRestored，恢复不能证实为Unknown。

初始stop=true/调用前sender已关闭返回Denied且没有网络动作；初始Gate/Guard错误同样在work前返回。
work开始后即使再次Gate/Guard拒绝、begin尚未真正调用，原代码也会保守执行一次restore；
测试不能将“开始前取消”与“预检后并发取消”统一断言成restore=0。

### Commit真实顺序

window/初始stop → Gate（3秒）→ TrialGuard（4秒）→ 同步audit.consume → work内Gate/Guard → begin →
循环Gate/Guard/usable → Gate → persist → Gate/Guard → prepared。

- prepared失败或stop/sender已失效：await restore_ap + TrialGuard（8秒），成功RestoredAp，否则Unknown；不release。
- prepared成功且未停止：await finish（8秒），不成功立刻Unknown，不盲目再restore。
- finish成功后await Guard.release（5秒），成功才Saved，否则Unknown。
- finish启动后，即使watch取消/关闭sender，也继续等待有限finalizer；现有测试明确允许成功返回Saved。
  不能把“取消了”写成“保存肯定没发生”，或在已开始销毁回退资源后强制恢复AP。
- finish接收Gate，具体provider还会再检查；Gate计数不能脱离provider假设硬编码。

### 期限和上下文

station trial/commit的window均必须大于0且≤40秒；父AP的MAX_WINDOW_SECONDS为3600是另一层窗口，不能互相替换。
初始预检、work和清理有各自期限，window不是整项总墙钟截止；同步CommitAudit.consume没有本函数内的异步timeout，
因此不能宣传“整个commit必定40/60秒结束”。测试假后端必须异步可取消，不用阻塞线程模拟超时。
当前Cargo中tokio未启用test-util，方案不假设可用time::pause/advance，不改Cargo或产品常量。

两个run不spawn清理任务，也没有可执行async恢复的Drop实现。关闭watch sender并仍await run是现有受支持路径；
直接drop/abort整个run不是同一语义。本轮只静态指出验收边界，不宣称已复现生产缺陷。

### 现有测试截面

26个固定公开文件中静态发现62个测试函数：trial直接6个、commit直接7个，余项为AP事务、协议/journal、executor、假D-Bus后端及Core集成相关。
每个测试可能有多个循环子场景，62不等于10个固定case的验收数量；本轮没有运行这些测试。
完整名称、源文件、行号和固定Git链接见完整测试目录（见本轮资料包）。



<a id="cases"></a>
## C21 · 10类故障的已有覆盖与缺口

固定main `9c8d36b4b5959cd441f2bc181ff612c96cea5ac5`。只读盘点，以下测试名称表示源码中存在，不代表本轮已执行或通过。

| case ID | 已发现覆盖层级 | 主要限制 |
|---|---|---|
| `wrong_password` | static_related | 已有fail_begin通用Io，不区分真实WPA认证失败；不能由参数命名推断密码实测。 |
| `ssid_missing` | static_related | 现有通用begin错误相关；未发现明确以SSID不可见为独立故障注入语义的直接run测试。 |
| `dhcp_deadline` | static_related | 已有ready=false加20ms窗口导致失败恢复；并非真实DHCP期限或150ms轮询的完整计时矩阵。 |
| `cancel_before_start` | static_direct | 直接覆盖预置stop=true；commit还覆盖初始sender关闭，trial初始关闭需单列。并发预检后取消不是同一零副作用前置。 |
| `cancel_during_trial` | static_related | trial直接测试主要是关闭sender；显式取消有executor层覆盖；commit保存中取消已有直接覆盖。还缺begin/usable/restore阶段的统一trace。 |
| `caller_dropped` | static_direct | caller loss目前指watch sender消失；不是drop整个run Future/abort任务的异步恢复保证。commit后段测试为显式取消，sender关闭还须补变体。 |
| `partial_persist` | static_related | 直接trait fixture的fail=persist在step返回错误，未先标记已保存；真实NM代码+假D-Bus有lost_save_reply等较强覆盖，但不是实机。 |
| `restore_failed` | static_direct | 已有restore返回Err及guard损坏相关覆盖；restore挂起到8秒和恢复后verify失败需与返回Err分开。 |
| `revoke_before_commit` | static_direct | 已有初始拒绝、persist后撤权和NM finish前拒绝；audit之后但persist之前各检查点需单列。 |
| `stale_completion` | outer_layer_only | 两个run不接受owner_generation/trial_id/receipt发布端口，真正代次与回执归属在Intent/Journal/executor/Core。不能仅用新Mock自报stale被拒绝就称run已覆盖。 |

### wrong_password

已有静态来源：

- [successful_or_failed_test_returns_to_ap_and_never_saves_or_releases_isolation](https://github.com/Viper-Boss/RustSimHub/blob/9c8d36b4b5959cd441f2bc181ff612c96cea5ac5/crates/setup-ap/src/station_trial/tests.rs#L69)
- [begin_or_save_failure_and_revocation_restore_exact_ap_without_releasing_guard](https://github.com/Viper-Boss/RustSimHub/blob/9c8d36b4b5959cd441f2bc181ff612c96cea5ac5/crates/setup-ap/src/station_commit/tests.rs#L218)

范围限制：已有fail_begin通用Io，不区分真实WPA认证失败；不能由参数命名推断密码实测。

C22：独立合成begin失败样例，begin进入后返回Io；await原run的restore并核对真实Outcome。

C23：显式确认已消费后begin失败；不得persist/finish/release。

后续必须断言：

- begin=1, restore=1；恢复已证实才FailedAndRestored/RestoredAp
- persist=finish=release=0；无自动重试
- 不声称真实口令错误已识别

### ssid_missing

已有静态来源：

- [successful_or_failed_test_returns_to_ap_and_never_saves_or_releases_isolation](https://github.com/Viper-Boss/RustSimHub/blob/9c8d36b4b5959cd441f2bc181ff612c96cea5ac5/crates/setup-ap/src/station_trial/tests.rs#L69)

范围限制：现有通用begin错误相关；未发现明确以SSID不可见为独立故障注入语义的直接run测试。

C22：独立SSID缺失后端场景，分别在begin错误和usable不就绪路径观察清理；不同模式只代表模拟后端。

C23可复用不就绪路径，成功试连本身不授权保存；不得伪造SSID扫描结果。

后续必须断言：

- 始终记录begin/usable/restore次数与顺序
- 只由实际run给Outcome；无可达互联网断言
- 保留原AP/隔离，零persist

### dhcp_deadline

已有静态来源：

- [successful_or_failed_test_returns_to_ap_and_never_saves_or_releases_isolation](https://github.com/Viper-Boss/RustSimHub/blob/9c8d36b4b5959cd441f2bc181ff612c96cea5ac5/crates/setup-ap/src/station_trial/tests.rs#L69)
- [begin_or_save_failure_and_revocation_restore_exact_ap_without_releasing_guard](https://github.com/Viper-Boss/RustSimHub/blob/9c8d36b4b5959cd441f2bc181ff612c96cea5ac5/crates/setup-ap/src/station_commit/tests.rs#L218)

范围限制：已有ready=false加20ms窗口导致失败恢复；并非真实DHCP期限或150ms轮询的完整计时矩阵。

C22：usable持续false或在可控await阻塞，短窗口到期后restore；用进入握手证明确实到达后端。

C23：同类不就绪发生在persist前，audit只消费一次、persist=0，恢复失败时Unknown。

后续必须断言：

- 不延长产品40秒窗口
- usable不等于互联网通；no persist/finish/release
- 截止只终止work，继续await有界restore

### cancel_before_start

已有静态来源：

- [initial_cancellation_or_invalid_authority_or_wrong_guard_has_no_network_effects](https://github.com/Viper-Boss/RustSimHub/blob/9c8d36b4b5959cd441f2bc181ff612c96cea5ac5/crates/setup-ap/src/station_trial/tests.rs#L99)
- [invalid_gate_guard_stop_audit_and_window_never_start_the_network](https://github.com/Viper-Boss/RustSimHub/blob/9c8d36b4b5959cd441f2bc181ff612c96cea5ac5/crates/setup-ap/src/station_commit/tests.rs#L190)

范围限制：直接覆盖预置stop=true；commit还覆盖初始sender关闭，trial初始关闭需单列。并发预检后取消不是同一零副作用前置。

C22：分开预置true和调用前drop(sender)，原run返回Denied，记录gate/guard/网络均未调用。

C23：相同两变体且consume=0；另测窗口0/>40s为Invalid，不混作取消。

后续必须断言：

- begin=usable=restore=persist=finish=release=consume=0
- 并发在预检后取消可保守restore，不错误要求其restore=0
- 所有请求均合成

### cancel_during_trial

已有静态来源：

- [caller_loss_after_start_still_awaits_exact_ap_restoration](https://github.com/Viper-Boss/RustSimHub/blob/9c8d36b4b5959cd441f2bc181ff612c96cea5ac5/crates/setup-ap/src/station_trial/tests.rs#L122)
- [exact_local_cancel_waits_for_restore_but_wrong_owner_cancel_is_ignored](https://github.com/Viper-Boss/RustSimHub/blob/9c8d36b4b5959cd441f2bc181ff612c96cea5ac5/crates/setup-ap/src/executor/configuration/tests.rs#L452)
- [cancellation_during_save_waits_for_ap_restoration](https://github.com/Viper-Boss/RustSimHub/blob/9c8d36b4b5959cd441f2bc181ff612c96cea5ac5/crates/setup-ap/src/station_commit/tests.rs#L311)

范围限制：trial直接测试主要是关闭sender；显式取消有executor层覆盖；commit保存中取消已有直接覆盖。还缺begin/usable/restore阶段的统一trace。

C22：Notify/watch握手后分别在begin与usable阻塞时send(true)，await原run恢复；restore阻塞时取消不丢清理。

C23：保存阶段取消沿用已有断言；finish启动后的取消另按产品现有语义处理。

后续必须断言：

- work取消后restore恰一次并等待完成
- 无失败后begin重试或新操作
- Unknown不能被记作隔离已释放；调用次数按所在阶段断言

### caller_dropped

已有静态来源：

- [caller_loss_after_start_still_awaits_exact_ap_restoration](https://github.com/Viper-Boss/RustSimHub/blob/9c8d36b4b5959cd441f2bc181ff612c96cea5ac5/crates/setup-ap/src/station_trial/tests.rs#L122)
- [invalid_gate_guard_stop_audit_and_window_never_start_the_network](https://github.com/Viper-Boss/RustSimHub/blob/9c8d36b4b5959cd441f2bc181ff612c96cea5ac5/crates/setup-ap/src/station_commit/tests.rs#L190)
- [cancellation_after_finalization_started_does_not_drop_cleanup_or_guess_rollback](https://github.com/Viper-Boss/RustSimHub/blob/9c8d36b4b5959cd441f2bc181ff612c96cea5ac5/crates/setup-ap/src/station_commit/tests.rs#L328)

范围限制：caller loss目前指watch sender消失；不是drop整个run Future/abort任务的异步恢复保证。commit后段测试为显式取消，sender关闭还须补变体。

C22：begin进入后drop(sender)，由仍被持有的run执行恢复；记录完整先后。

C23：persist等待与finish等待分别关闭sender，保留并await work；不能abort它来测试清理。

后续必须断言：

- 不声称Rust Future被直接丢弃时可执行async恢复
- finish已开始时继续await，成功可为Saved
- finish不明则Unknown且不盲目restore/release

### partial_persist

已有静态来源：

- [begin_or_save_failure_and_revocation_restore_exact_ap_without_releasing_guard](https://github.com/Viper-Boss/RustSimHub/blob/9c8d36b4b5959cd441f2bc181ff612c96cea5ac5/crates/setup-ap/src/station_commit/tests.rs#L218)
- [cancellation_during_save_waits_for_ap_restoration](https://github.com/Viper-Boss/RustSimHub/blob/9c8d36b4b5959cd441f2bc181ff612c96cea5ac5/crates/setup-ap/src/station_commit/tests.rs#L311)
- [lost_or_unverified_save_never_commits_and_exact_checkpoint_restores_ap](https://github.com/Viper-Boss/RustSimHub/blob/9c8d36b4b5959cd441f2bc181ff612c96cea5ac5/crates/setup-ap/src/nm/tests/commit.rs#L138)

范围限制：直接trait fixture的fail=persist在step返回错误，未先标记已保存；真实NM代码+假D-Bus有lost_save_reply等较强覆盖，但不是实机。

C22只说明trial没有persist接口；不得在trial夹具中另造保存器。

C23：后端先记录本次持久副作用再Err或阻塞丢回执；分restore成功/错误/超时三变体，原run决定RestoredAp/Unknown。

后续必须断言：

- consume=1,persist=1,finish=release=0,restore=1
- 原操作/凭据标识一致，不新建重试
- 无法证实恢复时Unknown，保留部分状态证据

### restore_failed

已有静态来源：

- [successful_or_failed_test_returns_to_ap_and_never_saves_or_releases_isolation](https://github.com/Viper-Boss/RustSimHub/blob/9c8d36b4b5959cd441f2bc181ff612c96cea5ac5/crates/setup-ap/src/station_trial/tests.rs#L69)
- [failed_restore_keeps_unknown_and_isolation](https://github.com/Viper-Boss/RustSimHub/blob/9c8d36b4b5959cd441f2bc181ff612c96cea5ac5/crates/setup-ap/src/station_commit/tests.rs#L254)
- [unknown_station_restore_retains_full_ap_marker_and_isolation](https://github.com/Viper-Boss/RustSimHub/blob/9c8d36b4b5959cd441f2bc181ff612c96cea5ac5/crates/setup-ap/src/executor/configuration/tests.rs#L356)

范围限制：已有restore返回Err及guard损坏相关覆盖；restore挂起到8秒和恢复后verify失败需与返回Err分开。

C22：restore错误、挂起、恢复后guard拒绝，全部Unknown；保留从begin到清理的trace。

C23：未准备完成分支相同；finish开始后的失败不能套此分支去回退。

后续必须断言：

- Unknown不能伪造成恢复成功
- guard不release；不清证据或换ID重试
- 真实8秒timeout用外部有界执行预算，绝不改产品常量

### revoke_before_commit

已有静态来源：

- [invalid_gate_guard_stop_audit_and_window_never_start_the_network](https://github.com/Viper-Boss/RustSimHub/blob/9c8d36b4b5959cd441f2bc181ff612c96cea5ac5/crates/setup-ap/src/station_commit/tests.rs#L190)
- [begin_or_save_failure_and_revocation_restore_exact_ap_without_releasing_guard](https://github.com/Viper-Boss/RustSimHub/blob/9c8d36b4b5959cd441f2bc181ff612c96cea5ac5/crates/setup-ap/src/station_commit/tests.rs#L218)
- [revoked_gate_before_final_checkpoint_keeps_automatic_inner_recovery](https://github.com/Viper-Boss/RustSimHub/blob/9c8d36b4b5959cd441f2bc181ff612c96cea5ac5/crates/setup-ap/src/nm/tests/commit.rs#L211)

范围限制：已有初始拒绝、persist后撤权和NM finish前拒绝；audit之后但persist之前各检查点需单列。

C22：初始/循环/恢复后gate撤权分别拒绝或FailedAndRestored，损坏guard导致Unknown。

C23：初始gate拒绝时consume=0；在usable后persist前撤权则consume=1,persist=0,restore=1；persist后撤权保留一次保存事实。

后续必须断言：

- 不能把初始拒绝与已消费intent混为一类
- 恢复不依赖继续拥有业务权限
- 不以总gate次数替代其对应阶段断言

### stale_completion

已有静态来源：

- [station_cancel_requires_exact_original_ap_owner_and_trial](https://github.com/Viper-Boss/RustSimHub/blob/9c8d36b4b5959cd441f2bc181ff612c96cea5ac5/crates/setup-ap/src/channel/station/tests.rs#L58)
- [station_terminal_receipt_cannot_be_overwritten_or_forge_https_verification](https://github.com/Viper-Boss/RustSimHub/blob/9c8d36b4b5959cd441f2bc181ff612c96cea5ac5/crates/setup-ap/src/channel/files/tests.rs#L130)
- [wrong_generation_missing_cleanup_or_foreign_observation_cannot_confirm_save](https://github.com/Viper-Boss/RustSimHub/blob/9c8d36b4b5959cd441f2bc181ff612c96cea5ac5/crates/core/src/setup_ap_control/station/commit/tests.rs#L297)
- [only_exact_successful_trial_admits_confirmation_and_durable_consumption_survives_restart](https://github.com/Viper-Boss/RustSimHub/blob/9c8d36b4b5959cd441f2bc181ff612c96cea5ac5/crates/setup-ap/src/channel/files/tests/commit.rs#L35)

范围限制：两个run不接受owner_generation/trial_id/receipt发布端口，真正代次与回执归属在Intent/Journal/executor/Core。不能仅用新Mock自报stale被拒绝就称run已覆盖。

C22只测已撤销Gate和取消后的原run不返回UsableAndRestored，记录作用于原凭据的清理。

C23只测finalizer复核Gate及结果未知边界；旧回执不能覆盖新事务的断言复用现有外层证据，若要改外层测试先提最小授权路径。

后续必须断言：

- 不在生产trait补发明的epoch参数
- 历史Saved允许按原intent查询，不一概拒绝所有旧boot历史
- 没有外层生产拒绝证据不得把完整stale case记为通过


## C21 · C22/C23/C24 后续测试实施方案（尚未执行）

资料基线 `9c8d36b4b5959cd441f2bc181ff612c96cea5ac5`。本轮只给方案，不写Rust测试或新配网器，不调整生产接口/期限。

### 最小文件边界

C22新增 crates/setup-ap/src/station_trial/tests/t5_matrix.rs；C23新增 crates/setup-ap/src/station_commit/tests/t5_matrix.rs。
两个既有tests.rs仅增加 mod t5_matrix;，必要时显式path，只接cfg(test)，不移动旧测试。
C24正式文档路径为 docs/testing/ap-failure-matrix.md 和本人报告。上述测试改动仍是计划；本PR只提交本页与本人报告，没有修改测试接入或实现。
Core/执行器/NM/生产trait/期限/Cargo/其他成员源码保持只读。

### 后端与观察方式

直接调用真实station_trial::run和station_commit::run；Fake仅实现外部StationNetwork/CommitNetwork、Guard/Gate/Audit，
不返回伪造Outcome，不造第二状态机。纯合成凭据不打印；每个场景独立夹具、原凭据/intent不变。
共享有限事件记录至少含 case_id、phase、序号、gate/guard/audit/begin/usable/persist/restore/finish/release的次数与顺序。
Gate/Guard每次调用记录，允许在指定到达点撤权/拒绝；Audit只有同步消费和失败，不充当生产Owner身份。

用Notify/watch握手证明到达begin/usable/persist/restore/finish，然后触发错误、取消、关闭sender或释放等待。
work future始终由测试持有并await，不spawn后abort来假装能恢复。不得靠sleep猜测注入点。
DHCP/窗口过期保留真实小窗口（例如250ms）与有界外层预算；8秒restore/finish期限测试保持原值，
先校验原路径是否值得新增，避免为同一错误断言重复等待。无test-util新依赖。

### 两个正向控制与重要反例

Trial正向控制必须由原run返回UsableAndRestored，begin/restore各一次，零persist/finish/release；
这只代表DHCP/本地station与AP恢复，不是互联网或真实Wi-Fi实测。
Commit正向控制由原run返回Saved，consume→begin→persist→finish→release各一次，不restore；
每case记录Gate/Guard检查，但finish内部可能附加Gate，不能仅按总数推权限已验证。

失败目标见[10类矩阵](#cases)和机器可读矩阵（见资料包）。
partial_persist需先记录部分副作用再返回Err/失去回执，区分已有简单persist错误；
finish已经开始后的取消继续await，允许Saved或Unknown，绝不反向强制RestoredAp。
恢复失败/超时及恢复后verify失败必须Unknown且不释放隔离；不清日志/改预算/重建操作来通过。

### 代次与旧完成的范围缺口

stale_completion的强保证跨越低层run与外层Intent/Journal/Core。C22/23可补撤权/取消后的控制流与调用序列，
但不应在允许目录之外改Core或在假后端添加一个自证的generation状态机。
原有外层wrong_generation/immutable receipt等测试只标静态相关证据；如负责人要新增真实跨代次发布测试，
先提出最小外层测试接入许可。已保存历史回执按原intent查询可以合法，不能简单把旧boot一律作废。

### 未来WSL2验证计划

正式实现获审后在固定Git归档、普通Linux UID、原生source/target/TMPDIR中执行，记录确切SHA/tree、环境和退出码：

```sh
cargo test -p rustsimhub-setup-ap --lib station_trial::tests::t5_matrix --locked --offline -- --nocapture
cargo test -p rustsimhub-setup-ap --lib station_commit::tests::t5_matrix --locked --offline -- --nocapture
cargo test -p rustsimhub-setup-ap --lib station_trial::tests --locked --offline
cargo test -p rustsimhub-setup-ap --lib station_commit::tests --locked --offline
cargo clippy -p rustsimhub-setup-ap --all-targets --locked --offline -- -D warnings
cargo fmt --all -- --check
```

这只是命令方案，尚未执行；最终选择器以实际新增模块及Cargo --list回读为准，不能让零匹配变成通过。
是否扩大到setup-ap全包/外层假D-Bus回归，按真正改动与失败需要决定，不重复现有无关测试或暗中连接系统NM。
C24交逐case真实结果、调用序列、失败/未运行项、来源许可及主线补验；固定10case缺项保持未完成。

### 明确保留给主线的实测

真实WPA错误、SSID可见性/DHCP、Root防火墙/检查点、实际断连及保存回执、物理网络切换和恢复验证；
不触发设备、真实AP/NM/D-Bus、普通蜂窝/APN或任何通信。原AP-03/04实机前置和负责人最终验收不因离线测试改变。


## 固定来源索引

下表记录本轮实际读取的公开Git对象；sha为文件SHA256，不是制品通过证明。

| 来源 | 字节数 | SHA256 |
|---|---|---|
| [docs/team/MEMBER-C-NAS-COMPAT.md](https://github.com/Viper-Boss/RustSimHub/blob/9c8d36b4b5959cd441f2bc181ff612c96cea5ac5/docs/team/MEMBER-C-NAS-COMPAT.md#L1) | 12271 | `666336b66a76e3662c58c084a8a8689750518abdd01a88fb0c4820addf3002d3` |
| [docs/team/ADDITIONAL-OFFLINE-PACKAGES-2026-10-27.md](https://github.com/Viper-Boss/RustSimHub/blob/9c8d36b4b5959cd441f2bc181ff612c96cea5ac5/docs/team/ADDITIONAL-OFFLINE-PACKAGES-2026-10-27.md#L1) | 63138 | `0764e05e99275ad82a8b711c384e76443c98a093d6b49f392b5949503741b0c6` |
| [docs/team/interfaces/t5-offline-v1.json](https://github.com/Viper-Boss/RustSimHub/blob/9c8d36b4b5959cd441f2bc181ff612c96cea5ac5/docs/team/interfaces/t5-offline-v1.json#L1) | 4839 | `b1f2ba1bff009755339481c0b288bc92294d80c528abb390d042b0c93120089e` |
| [crates/setup-ap/Cargo.toml](https://github.com/Viper-Boss/RustSimHub/blob/9c8d36b4b5959cd441f2bc181ff612c96cea5ac5/crates/setup-ap/Cargo.toml#L1) | 959 | `78d479bda98283e384f4599c80075b32727b739c33d2028528fae2c6f2f72eb7` |
| [crates/setup-ap/src/lib.rs](https://github.com/Viper-Boss/RustSimHub/blob/9c8d36b4b5959cd441f2bc181ff612c96cea5ac5/crates/setup-ap/src/lib.rs#L1) | 7410 | `a43447a553e10b65c2d57aafa54bf96f2640a1d80036f48b91cc0b3a763b7417` |
| [crates/setup-ap/src/station_trial.rs](https://github.com/Viper-Boss/RustSimHub/blob/9c8d36b4b5959cd441f2bc181ff612c96cea5ac5/crates/setup-ap/src/station_trial.rs#L1) | 5043 | `f37dc7c3ed89195158ea3e5b647b2f03c0078b044d9ab30e379eff1af4f2ae2f` |
| [crates/setup-ap/src/station_trial/tests.rs](https://github.com/Viper-Boss/RustSimHub/blob/9c8d36b4b5959cd441f2bc181ff612c96cea5ac5/crates/setup-ap/src/station_trial/tests.rs#L1) | 6687 | `25b0a318bbebdaad34be60263fb16112ba922cba48ceb9789001f6e6ace91006` |
| [crates/setup-ap/src/station_commit.rs](https://github.com/Viper-Boss/RustSimHub/blob/9c8d36b4b5959cd441f2bc181ff612c96cea5ac5/crates/setup-ap/src/station_commit.rs#L1) | 4659 | `f426ee5c127f3ff580f36fa7a7133b03bdc03863671eb498ea021c201ad1c33c` |
| [crates/setup-ap/src/station_commit/tests.rs](https://github.com/Viper-Boss/RustSimHub/blob/9c8d36b4b5959cd441f2bc181ff612c96cea5ac5/crates/setup-ap/src/station_commit/tests.rs#L1) | 9600 | `7a9e2ff836be0026feb6ea266fd419ca44aefdee97138b52760bc8164ed6f3bf` |
| [crates/setup-ap/src/transaction.rs](https://github.com/Viper-Boss/RustSimHub/blob/9c8d36b4b5959cd441f2bc181ff612c96cea5ac5/crates/setup-ap/src/transaction.rs#L1) | 5626 | `82ab42ef5775cf6597125d166d7fca93ba6a16a243cc29cab82e74c7bb0eb32e` |
| [crates/setup-ap/src/transaction/tests.rs](https://github.com/Viper-Boss/RustSimHub/blob/9c8d36b4b5959cd441f2bc181ff612c96cea5ac5/crates/setup-ap/src/transaction/tests.rs#L1) | 5768 | `419cffa18978164ea3464cd4fb9a7da746ac7178769d57c0431333f2c7ae71b3` |
| [crates/setup-ap/src/executor/configuration.rs](https://github.com/Viper-Boss/RustSimHub/blob/9c8d36b4b5959cd441f2bc181ff612c96cea5ac5/crates/setup-ap/src/executor/configuration.rs#L1) | 11754 | `15446e9cde39579d7a17b02a3b7ec3d8924fdd503543df1ffb917cfca7b6823f` |
| [crates/setup-ap/src/executor/configuration/tests.rs](https://github.com/Viper-Boss/RustSimHub/blob/9c8d36b4b5959cd441f2bc181ff612c96cea5ac5/crates/setup-ap/src/executor/configuration/tests.rs#L1) | 17001 | `d4720c90e885c77d12b98efbcd20ed2a3ceb4214b43cc4dfed38254b571c5753` |
| [crates/setup-ap/src/executor/configuration/commit.rs](https://github.com/Viper-Boss/RustSimHub/blob/9c8d36b4b5959cd441f2bc181ff612c96cea5ac5/crates/setup-ap/src/executor/configuration/commit.rs#L1) | 5958 | `1ee907841d112450da4a89a6479d9c6cd70da2d865f363cca02ceaedd64d463b` |
| [crates/setup-ap/src/executor/configuration/tests/commit.rs](https://github.com/Viper-Boss/RustSimHub/blob/9c8d36b4b5959cd441f2bc181ff612c96cea5ac5/crates/setup-ap/src/executor/configuration/tests/commit.rs#L1) | 8029 | `447c6b8521791e8bc6bd6e97ccc4636c1eacfc3d99d2960f12c6e80656b69e2e` |
| [crates/setup-ap/src/channel/station.rs](https://github.com/Viper-Boss/RustSimHub/blob/9c8d36b4b5959cd441f2bc181ff612c96cea5ac5/crates/setup-ap/src/channel/station.rs#L1) | 5891 | `11a8428ffe05798c734ccbfd9273d6f7efd7726d21b81c7f6003691a232fecd4` |
| [crates/setup-ap/src/channel/station/tests.rs](https://github.com/Viper-Boss/RustSimHub/blob/9c8d36b4b5959cd441f2bc181ff612c96cea5ac5/crates/setup-ap/src/channel/station/tests.rs#L1) | 3554 | `130498e83d82a0083e3cee7f4f6e9774c3c24a473d18976dbd0115c9d29331a8` |
| [crates/setup-ap/src/channel/commit.rs](https://github.com/Viper-Boss/RustSimHub/blob/9c8d36b4b5959cd441f2bc181ff612c96cea5ac5/crates/setup-ap/src/channel/commit.rs#L1) | 5858 | `8a8461bfe8963b48e650577f2540e91941a8c060ad9500f54494d360a03537d6` |
| [crates/setup-ap/src/channel/commit/tests.rs](https://github.com/Viper-Boss/RustSimHub/blob/9c8d36b4b5959cd441f2bc181ff612c96cea5ac5/crates/setup-ap/src/channel/commit/tests.rs#L1) | 4083 | `df4f7756cdd84ab0df550583ca56e8428a80f06148468dd8e0140e62750e4339` |
| [crates/setup-ap/src/channel/files/commit.rs](https://github.com/Viper-Boss/RustSimHub/blob/9c8d36b4b5959cd441f2bc181ff612c96cea5ac5/crates/setup-ap/src/channel/files/commit.rs#L1) | 6391 | `0965dd7fdc29e1931142b9c6efcd5b0b5ddad79d0de9091dc897a16f2737dd4d` |
| [crates/setup-ap/src/channel/files/tests/commit.rs](https://github.com/Viper-Boss/RustSimHub/blob/9c8d36b4b5959cd441f2bc181ff612c96cea5ac5/crates/setup-ap/src/channel/files/tests/commit.rs#L1) | 4746 | `333413d6e8a5dd303f0a42c119b3bf1f4b64ee0233028c6c123f21e99fafd92b` |
| [crates/setup-ap/src/channel/files/tests.rs](https://github.com/Viper-Boss/RustSimHub/blob/9c8d36b4b5959cd441f2bc181ff612c96cea5ac5/crates/setup-ap/src/channel/files/tests.rs#L1) | 5720 | `f9bb9e3ecdc088de07b4208df04c07fed0fec820c5d090066f3f4fc42ff7d5a3` |
| [crates/setup-ap/src/nm/station.rs](https://github.com/Viper-Boss/RustSimHub/blob/9c8d36b4b5959cd441f2bc181ff612c96cea5ac5/crates/setup-ap/src/nm/station.rs#L1) | 23372 | `e155c905c53fd4849886edfb4ed563c1a25b47d94da8323b809d437bae079afc` |
| [crates/setup-ap/src/nm/tests/commit.rs](https://github.com/Viper-Boss/RustSimHub/blob/9c8d36b4b5959cd441f2bc181ff612c96cea5ac5/crates/setup-ap/src/nm/tests/commit.rs#L1) | 7999 | `f56777b3dee3e1f40400db61d778e2e1fb51c03c3fdf5e679be0e6bfc275283f` |
| [crates/core/src/setup_ap_control/station/commit.rs](https://github.com/Viper-Boss/RustSimHub/blob/9c8d36b4b5959cd441f2bc181ff612c96cea5ac5/crates/core/src/setup_ap_control/station/commit.rs#L1) | 16929 | `eca7af75b85d0566f5f43b9aefd809c47732b92fa1eba336de45213a409b0696` |
| [crates/core/src/setup_ap_control/station/commit/tests.rs](https://github.com/Viper-Boss/RustSimHub/blob/9c8d36b4b5959cd441f2bc181ff612c96cea5ac5/crates/core/src/setup_ap_control/station/commit/tests.rs#L1) | 15042 | `91c2624d11b0f9185f2591ad5ddfc9e864ae860dee4bb0b1873abdbbc57c28a1` |

代码/接口声明和文档链接的静态校验不证明62项测试已运行。正式执行需固定后续实现HEAD、实际测试名称/数量、退出码和未运行项；整体固定P0～P7仍约80%。
