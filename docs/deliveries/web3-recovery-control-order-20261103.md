# WEB3-REC：原Core恢复控制准入/消费顺序修复

交付日期：2026-11-04（Asia/Shanghai）；任务书日期2026-11-03。本独立包十个开发动作均已执行并交付实际产品修复，验证失败/边界如下保留，不等于设备验收或全部检查全绿。

## 固定来源与范围

任务：[WEB3-REC](https://github.com/Viper-Boss/RustSimHub/blob/codex/web-product-delegation-20261103/docs/team/WEB3-REC-20261103.md)，开工读取blob `b8bc7d6bf41205aa0f73306b553fe890dbe5b0c3`，同时读取全面分派文档及Core竞争诊断交接。开工成功fetch真实最新main：`8f9cfeff1a4b5a804b8b138e890f7386caf5a006`；开放PR中没有同包产品实现。独立分支 `codex/web3-recovery-control-order-20261103`，独立工作树/target/私有日志，不接用其他窗口缓存写入锁。

- 初始确定性负例：`9442d7a25838f83deba698c0c3da90ecaa92bc6e`，仍为旧产品准入路径。
- 更严格、真实Gate返回Busy的负例来源：`aaffdf1c89bec4682ceafab998f3a7795a1df3bf`，保留旧产品，Auth持续持有到旧Gate真正返回Busy再放行控制；完整源码在本PR历史中。
- 产品修复提交：`541b55a67cfbc1eba6650084fc4b3f1709a34b4d`。
- 最终正式受测实现/测试HEAD：**`15bc12f9f9840f7be368666a111168812e563b6c`**。正常合入负例分支，手工解决自己测试文件的冲突，保留完整负例历史；相对541b仅增强测试同步，不改生产代码。
- 本报告之后只有文档提交；最终PR完整HEAD在PR描述回读记录，不用报告后续HEAD冒称另测新产品。

只改 `network_trial/provision.rs`、`provision/recovery.rs`、其 `t5.rs`、测试模块导入及新增 `tests/control_order.rs`，加本报告。没有修改Auth/Storage公共文件、Core公共接线、wire、setup_ap_control、网页、密码/GATT、其他窗口健康模块、依赖/锁/默认开关或全局台账。无需跨窗口公共Auth接线提交。没有合入任务分派/看板分支或重做旧Root/音频交付。

## 原机制与实际修复

原control_recovery在actor外预留队列位置后，持Auth执行check_trial、reserve、claim、bind_control；真正发往actor在这些工作之后。后台actor的独立Gate可以在该Auth临界区触发try_lock Busy，完成实际回滚；准入恢复后发现Begin已结束，再经原Auth/Storage边界返回Storage。源交接仅证明同类竞争机制，仍不能追认过去所有环境失败的根因。

现在先在原容量1的控制队列取得位置，再把未授予执行权的pending input交给原actor。**原actor自己**在收到控制后完成原body解码、完整验证、持久reserve/claim、原trial绑定，然后消费为原私有RecoveryTrialControl。公开recovery和T5入口的Confirm/Cancel都在解码之前进入这条序列，不能只移动claim却留下actor外的控制解码Auth临界区。Begin/Lookup/Status沿用既有入口；没有第二个网络事务或旁路后台执行器。

Pending input携带原Auth Arc，并在消费前比较同一Auth对象；实际Auth继续验证UUID、精确body/action/target、Owner/grant/realm/代次、原连接和Begin、独立Confirm及原绝对deadline。输入不是授权证明；错误Auth/body/动作/目标或非Ready确认在claim之前拒绝，不结束仍合法的原trial。队列无位置/原session已结束时不执行Auth admission、不留下永无消费者的claim。

锁序为：短时ACTIVE与state读取 → 释放这些守卫 → try_reserve原控制队列 → 原actor收到 → blocking Auth try_lock/原SQLite检查与claim → 释放Auth → 原控制复核/记Saving/后端commit/保存后复核 → 原终态记账。ACTIVE/state不跨Auth临界区持有；没有把Auth改成无限阻塞lock，没有将Busy转成许可，也没有全局跳过Gate。

原150ms后台检查分支保持；只将等待抽成内部trait方法供确定性调度测试，生产默认仍sleep150ms。处理自身控制准入时原actor不同时跑自己的后台Gate；外部长期持Auth仍Busy并有限回滚，原busy_auth_mutex_cannot_hide_an_actual_backend_rollback原断言保持且通过。正常Owner控制分支的许可规则不变。

原40秒最大trial及更短grant/能力deadline不延长；成功重查不续期。回复等待仍是单一8秒绝对界限，同时覆盖准入decision与终态回复，不能串两个8秒。发送后actor持有意图，丢回复/取消等待者不创建新提交；真正连接撤销/过期仍经原Auth拒绝。blocking admission不持有后端对象：deadline取消actor等待后，迟到工作只能在原检查下失败/保守留下不确定记录，不可复活网络保存。清理由原backend rollback与原Store终态完成；失败或副作用不明保留Unknown，不制造Saved/未执行结果。未改原nonce、floor、预算或Storage恢复规则。

## 修复前失败与修复后正反例

新增7组用例都运行原run_trial、真实Auth/SQLite及原解码/claim；外部verifier与NM为既有合成端口。测试手动poll原actor，并用Notify使原150ms分支就绪，Auth临界区由有限释放屏障控制，不靠sleep碰繁忙机器概率，没有生产HTTP测试开关。

更严格负例aaffdf1，命令 `cargo test -p rustsimhub-core --locked --offline --features recovery-execution legitimate_control_admission_cannot_race --lib -- --nocapture`：**退出101，0通过/1失败**。实际受控记录：

```text
CONTROL_ORDER gate_busy=1 control_error=Some(Storage) begin_row=Some(("rolled_back", Some(0), Some(1))) control_row=None
background Gate ran against its own control's Auth guard: left 3, right 2
```

这是真实旧Gate的Busy和真实Begin回滚/控制Storage，不把Storage称成已证磁盘I/O。最终15bc相同控制屏障：确认gate_busy=0，两份回执saved；取消gate_busy=0，两份回执rolled_back；在准入持锁期间断连，Forbidden、无控制claim，Begin真实rolled_back。原外部长期占锁回归仍回滚；没有“后来单项绿了”就推断修复。

| 新用例（control_order.rs） | 核验事实 |
|---|---|
| legitimate_control_admission_cannot_race_its_own_actor_gate | 原actor控制消费与其Gate序列一致；合法确认/取消及准入持锁中断连，真实Gate/SQLite屏障 |
| raw_control_reserves_capacity_before_decode_or_claim_and_queries_do_not_resubmit | 原始控制在消费前无DB行；队列满第二请求Busy且无行；结束/旧请求不能复活；实际重复证明在Auth finish处Replay；两次查询都只回原saved，后端persist恰好一次 |
| queued_disconnect_or_revoke_is_rechecked_before_any_control_claim | 排队后、消费前原连接失效/实际grant撤销拒绝，无新控制claim，真实临时副作用恢复 |
| rejected_pending_body_or_binding_does_not_claim_or_destroy_the_live_trial | 错target/body/Auth及错动作有限拒绝，无claim；合法原trial仍能按可信关闭清理 |
| original_deadline_ends_admission_and_late_auth_worker_cannot_start_a_save | 原1秒grant截止到达时，即使测试屏障持Auth也真实清理；放行迟到worker后仍无控制行/保存，deadline未变 |
| dropping_enqueued_reply_keeps_actor_ownership_and_reopen_does_not_create_a_second_commit | 消费前丢等待者，actor仍只提交一次；真实关闭并重开Auth/SQLite，两个saved回执保持 |
| queued_actor_loss_leaves_no_claim_and_reopen_fences_the_abandoned_begin | 生命周期故障取消未消费actor，队列失去消费者，控制Busy/无行；真实重开Auth/SQLite把原Begin保留Unknown，不重执行。这是生命周期模拟，非OS SIGKILL验收 |

其他连接/realm/Store绑定、正常Owner不得控制恢复、存储失败、save前后撤权/断连、部分persist+失败补偿、原deadline启动中断、Saving阶段丢回复以及查询/重开，均继续由原恢复/T5/Auth矩阵验证；没有改弱旧断言。内部Busy/Forbidden/Expired/InvalidInput与后端Io保留原类型和真实副作用事实；T5客户端仍只映射有限错误码/Unknown，不输出SQL、SSID、密码、身份或原始错误文本。

## 精确验证（正式HEAD15bc，除明确标注的基线）

已有工具链rustc 1.98.0 (88d9e12ae 2026-08-18)，既有Cargo.lock及离线缓存；本窗口独立CARGO_TARGET_DIR、CARGO_INCREMENTAL=0、CARGO_PROFILE_DEV_DEBUG=0。以下Cargo均使用--locked --offline，无依赖或锁改动。日志/合成私钥仅本窗口.local，不上传原始内容。

| 实际命令 | 退出码与结果 |
|---|---|
| cargo test -p rustsimhub-core --features recovery-execution control_order --lib -- --nocapture | 0；7组通过，0失败/忽略 |
| cargo test -p rustsimhub-core | 0；lib15、bin195通过（共210，0失败/忽略） |
| cargo test -p rustsimhub-core --all-features | 0；lib97通过/1原忽略、bin211通过/4原忽略（共308通过，0失败/5原专项忽略）；与默认重叠不相加 |
| cargo test -p rustsimhub-auth -p rustsimhub-storage --all-features | 0；Auth132、Storage129通过，0失败/忽略 |
| cargo clippy -p rustsimhub-core --all-targets --all-features -- -D warnings | 0 |
| cargo clippy -p rustsimhub-core --lib --bin rustsimhub-core -- -D warnings | 0；默认生产目标严格检查 |
| cargo clippy -p rustsimhub-core --all-targets -- -D warnings | **101；四处未修改的forwarding/ipc/tests.rs useless_conversion，见下述基线，不称全绿** |
| 原run_recovery_kernel_uids.py的--application流程 | 0；1个实际Rust fixture、16个UID_CASE；详情如下 |
| rustfmt --check（本包文件）/git diff --check | 0 |

默认全部目标严格Clippy在实际独立main `8f9cfeff1a4b5a804b8b138e890f7386caf5a006` 上用同命令重现退出101、同四处179/206/218/426行`.into()`警告。文件Git blob在main及本HEAD均为 `5dc7a36d8fd4905edc13e17fecb89dba70d805ea`。本包不在获准范围外改forwarding测试或加allow来取绿；全特性全部目标/默认生产目标严格检查已通过，默认全部目标这项基线失败明确留给主线。

原多角色驱动源未改：通过私有导入执行原main，只把WORK输出设置到本窗口.local；--application仍执行原编译/容器/结果校验。实际Core UID1001、BLE UID1002，拒绝UID0/1001/1003/1004/1005/1006；六种拒绝与十种派发/取消/超时/断连/撤权情形共16个标记。内核SO_PEERCRED/UnixStream、Core/Auth/SQLite及异步清理真实；verifier/sealer仍cfg(test)-synthetic，不称真实密码或NM/RF通过。源HEAD15bc且tracked_dirty=false。只用原新建私有容器、network=none、只读根、合成UID，无主机NSS/防火墙/系统总线或设备访问。

- 镜像固定 `python@sha256:2d44738ebf4ce5f63b9d9ea871e9c7cfdc0426f7b0c97cac915fd88ded1d86bb`，实际本地镜像ID `sha256:bdfc738a8a869292ec1b01f1b8e620db533e09d1fe68fe980a7118b22dae9cec`；没有pull或公网中继。
- 实际驱动Rust制品SHA256：`11f4eefe49b69d46715e64e01758e838dbe7059f930e2660ed2bd8a78bd0e723`。
- 原runner SHA256：`72709d37cdb0a1d5a09fab097ce0572c510b5923dcc5075903abb301fe2fba1a`。
- 私有多角色日志摘要：`9c88c940a92801f28b671597ece3f4c620c86ff50c537687bb8ce30effdb3b9e`；只发布摘要，不上传正文。

首轮新夹具误把Replay当作prepare阶段拒绝，而原Auth实际在finish验证阶段拒绝，导致新用例失败；按原真实接口修正夹具，不改Auth/旧断言。首轮严格检查发现新增PendingInput大枚举，已按建议给原始请求加Box，不降低预算或关闭lint。原始初次失败与所有受测日志保留.local。早期541b的矩阵/UID也通过，正式最终结果已对15bc重跑，不混用旧结果。

## 接纳与保留项

交付原actor真实产品修复和负/正证据，十项包内动作完成，剩余独立开发0。默认全部目标Clippy四处主线既有失败仍存在；云端最终检查在PR创建后读回，不提前称通过。未自行批准/合并、部署或操作设备；真实NM/无线、实体BLE/GATT、设备重启与负责人最终验收not_run。生命周期重开测试不冒称真实设备崩溃/掉电。没有接管窗口1/3/4/5或原成员独立任务。

复用仓库AGPL-3.0-only原Core/Auth/Storage/驱动与合成夹具，无新增第三方源码或依赖；AI辅助实现、验证与审查，Git作者/历史原样保留。整体81.5%、20/169、加权10.6%及原分母/权重保持；本包10动作和PR数不折算全局验收比例。
