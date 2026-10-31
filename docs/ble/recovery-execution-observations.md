# D23 原trial进度与原子终态记录

2026-10-31（Asia/Shanghai）。承接已合并#103提案、#135显式9以及本人#136～138的私有指纹、一次领取、原trial授权，补齐给唯一Core执行器使用的持久观察接点。未启动网络或修改普通Wi-Fi业务。#103已有Owner流程批准，本包新实现另按精确HEAD审查，不把批准/合并当技术或设备验收。

## 已实现的记录路径

- Begin只允许claimed→starting→connecting→ready，ready可回connecting；同状态观察可重入，禁止跳步/倒回起始。写入前后核真实Auth、当前数据库grant、原连接fence与单调期限。
- 保存前必须有同绑定/同trial的独立Confirm私有Claim。原Begin必须ready、Confirm必须claimed，**同一SQLite Immediate事务**把两者改为saving；Cancel/Begin不能代替Confirm。两者已saving时可重复记录。此处是持久确认记录，不意味着已保存。
- 可信原backend报告Saved时，两者必须都saving，且明确关联Confirm；同事务记录begin.saved与confirm.saved，settings_saved=1、rollback_verified=NULL。不能把Begin单独标为saved。
- 可信backend报告精确补偿成功时记录rolled_back/0/1；报告不明则unknown/NULL/NULL。只保留原trial、请求与绑定，不创建新幂等键。
- 同trial的其他已登记而未由本actor执行的请求变为Unknown，不替它们制造成功/回滚结论。Unknown继续全局阻断新BLE写入，不能被迟到成功或晚到rollback覆盖。

`RecoveryExecutionProgress`/`RecoveryExecutionCompletion`是可信源码观察类型，无Deserialize，不是T5 wire枚举或网络请求。Storage仍需要原始不可构造的私有Claim匹配确切持久请求/指纹/绑定；不存在从回执或JSON重建Claim的入口。**这些枚举本身不证明NM或硬件结果**，Core必须在原backend实际完成/核验后调用，不能直接映射前端saved/rollback_verified布尔。本批测试提供合成backend事实，未宣称实机保存或补偿通过。

## 权限与记账的区别

活跃Gate现在可跟随Begin的claimed/starting/connecting/ready/saving、Confirm的claimed/saving、Cancel的claimed；终态/Unknown不再提供执行权限。初始trial派生另用严格的claimed检查，已经progress的Claim不能重新派生40秒窗口。

进度和saving写入仍必须有当前授权；撤权/断连/到期后的**完成事实记账**保留原私有Claim及同Auth/同trial/同session检查，但不要求授权仍有效，否则实际rollback后会永远不能记录。该路径只返回快照、不签发权限、不改grant、不清Unknown；副作用仍须原actor逐await/保存门禁。其他Auth实例/外部控制Claim无法替原trial记账。

终态记录只允许活跃行结束，已完成的同一终态可幂等读回；相矛盾终态拒绝。重开将非终态变Unknown，原进程晚到的私有Claim也不能把它改回成功。完成快照恢复后的saved历史保持，恢复realm/Owner隔离仍令旧执行能力失效。

## 真实失败验证

新增10项Storage、8项Auth测试，复用真实单writer、SQLite、Identity/签名Owner、私有Auth与Backup API；外部A verifier仍仅cfg(test)合成。

覆盖完整进度/确认/保存两行、缺确认/动作混淆、非法跳步/反向观察、进度后禁止重新派生、撤权/断连/到期后的有限事实记账、跨Auth/Claim/指纹、终态竞争、Unknown/重开与迟到结果、保存历史备份恢复、未执行竞争控制保持Unknown。

在实际actor连接创建**仅测试使用的TEMP trigger**，令第二张Confirm回执UPDATE发生真实SQLite ABORT，分别在saving和saved两行更新中检查：第一张Begin也回滚、两行仍是原状态、没有半份settings_saved/rollback证据；移除TEMP trigger后同一生产函数通过。没有生产故障开关、替代状态机或修改持久DDL来通过测试。两个不同终态的真实writer竞争只允许一个结果提交，两行始终一致。

验证命令/精确HEAD/数量/摘要见[交付报告](../deliveries/member-d-recovery-execution-observations.md)。实现后正常合并最新main `342adaa2fee6f3ea36a82f0145dc7931e12e36de`，保留其已有统计/SMTP/NKN等来源作者，未修改他人实现或全局台账。主线已有的其他模块测试不算成本包新增。

无新DDL、依赖版本或默认入口；原6/旧7仍严格拒绝9。尚未接Core实际actor/正文语义解析、Normal与Recovery身份分支、USB Recovery分支、T5状态映射或标准A会话。A/Root/实际UID/发行schema9兼容和实机验收保留。完整阶段仍21/26、剩D22–26五步，固定P0–P7约80%。
