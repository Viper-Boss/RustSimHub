# D23 持久请求登记、私有票据与一次领取

2026-10-31（Asia/Shanghai）。继#135显式schema9、#136身份私有指纹，实现已合并#103提案的reserve/claim/只读Lookup阶段。Owner的[流程批准](https://github.com/Viper-Boss/RustSimHub/pull/103#pullrequestreview-5708198127)明确没有独立技术审核，本包不把它当实现验收。默认启动/6/旧7入口不变，无生产迁移或Wi-Fi派发。

## 实际调用和票据

可信Core解析同一已认证正文后，先用#136的`prepare_recovery_execution`固定动作/target/指纹，再走：

1. `Auth::reserve_recovery_execution(request)`消费私有准备对象；实际Auth门禁前后核验，writer事务核当前数据库grant/Owner/realm/pin及原子存活和单调期限。
2. 首次插入返回`RecoveryExecutionAdmission::Reserved(Box<RecoveryExecutionPending>)`。Pending没有公开构造器/Clone/Debug/Serde，保留原请求、原连接fence及Storage刚插入时唯一返回的私有票据。相同UUID/指纹/动作/target只返回`Existing`只读快照，**包括旧Reserved**；不能由快照重建票据。
3. `Auth::claim_recovery_execution(pending, new_trial)`消费Pending。Begin由原引擎预分配64小写hex trial id，Confirm/Cancel不得新分配；在首次外部副作用前，writer再次核当前数据库授权/原fence、reserved→claimed并提交，才返回私有`RecoveryExecutionClaim`。
4. 之后每个业务await/save仍须调用有效门禁；`check_recovery_execution_claim`同时核原能力、当前数据库授权及对应持久claimed行/trial。本阶段只接纳仍为claimed的记录，尚未接业务观察状态。

丢弃Pending/Claim、发送者消失或回复丢失，不把持久记录改回reserved、不重排队。重新连接只读旧记录；原Pending保留的原连接fence也已死亡，不能接管。中途失败可能保留reserved/claimed墓碑；这会阻止新Begin，重开后变Unknown。没有自动清理/TTL、归档或人工对账API，不把这份有限离线实现称为完整长期服务。

## writer与线性化边界

Storage的`from_trusted_auth`是可信源码适配构造器，不是鉴权证明，禁止从JSON/IPC直接构造。网络入口只能先走真实Auth私有能力。Storage不实现另一套密码或接受payload中的authenticated/saved布尔。供writer的fence只保存Auth实际Connection的Arc原子存活标记和原始最小Instant期限；Connection/transport/Auth丢弃及verifier更换会失活原标记，不接收客户端时钟或延长期限。

登记/领取在现有单SQLite actor的Immediate事务内完成，复用原grant当前性检查（精确grant、唯一active Owner及代次、realm、设备pin）。writer内不取Auth mutex、递归Store或await。末次事务内fence读取是取消/期限决定点；提交后再读fence，Auth返回前再核完整能力。**取消与SQLite提交不是一条跨内存/数据库原子指令**：取消若晚于决定点，可能留下已提交claimed行；不会因失败交出重试票据。后续副作用仍需原actor门禁，不能把claim说成永久授权。

已有记录只在确切指纹/动作/绑定/target相同时返回；不同参数拒绝Conflict。Begin全局互斥比较完整realm/grant/generation/request键，不能只比较UUID忽略另一个grant的同UUID。Confirm/Cancel必须有同绑定、同trial的非终态Begin；不跨grant、不操控已保存/回滚/未知事务。这里仅验证持久父行，不证明原内存actor仍在或Ready/40秒预算有效，业务接点必须继续核验。

任意历史realm的Unknown阻止新BLE写入；已有和缺失查询保留只读，不将NotFound当重发许可。全局256行、非Cancel最多255，预留一次Cancel；墓碑/旧realm计入容量，不删除换空间。恢复备份导致realm换代/Owner隔离后，即使正常Owner恢复并重新批准当前grant，旧Unknown仍阻断新Begin。

## 真实失败矩阵

新增15项Storage测试和10项Auth测试，均调用真实模块/SQLite/Backup API；外部A verifier只在cfg(test)为合成适配器。包括：

- 相同请求幂等、动作/正文/target冲突、Begin全局互斥和另grant相同UUID；并发writer领取只提交一个trial。竞争测试**仅在cfg(test)内部复制私有票据**挑战DB状态防线，生产类型不支持复制。
- 排队后实际原子标记失活、排队前一writer提交撤权，均拒绝领取，原reserved不变；无sleep/Auth回调或放宽权限。
- 真正SQLite `max_page_count`触发Full，事务回滚且不存在部分回执/可领取票据；255非Cancel+1Cancel边界。
- 原Auth丢弃即实际fence失活，尽管transport token仍存活；正文篡改、当前撤权、disconnect/verifier替换/确定性过期仍拒绝。
- 完整Auth/Store重开保留指纹/Unknown，不派发；Backup API恢复→正常Owner恢复→重新授权新realm后，旧Unknown仍阻断；查询前后完成快照库字节相等。

初稿领取只要求准备对象+reserved行，新增“断连后重连读取旧Reserved不得领取”的实际Auth回归断言**失败**。已改为两层不可构造/不可复制的首次登记票据；原场景现在得到Existing且原Pending被旧连接fence拒绝。保留修正前日志，不声称主线此前已上线相同功能/漏洞。没有通过删除拒绝场景、伪造结果或新增ignore变绿。

本阶段仍无Core dispatch、Recovery/Normal trial owner分支、业务状态观察写入/终态证据、T5 wire映射或原Wi-Fi引擎接线。没有凭claim调用NM、把Begin当Confirm能力或构造假Owner。未来接线必须复用唯一ACTIVE/TRIALS、coordination网络/电话锁、40秒（grant只可缩短）、有限NM checkpoint与保存前后Gate；业务失败的rollback证据只能由实际backend观察。A实际会话、Root/UID与发行schema9兼容、硬件验收保留。

精确HEAD/命令/结果见[本包交付](../deliveries/member-d-recovery-execution-ledger.md)。无新依赖/schema变更、网络/设备/生产库/口令、WWAN/IMS/APN或签名部署。完整阶段仍21/26，剩D22–26五步，整体约80%。
