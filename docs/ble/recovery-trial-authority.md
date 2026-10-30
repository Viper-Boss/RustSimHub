# D23 原trial后台授权及独立Confirm/Cancel门禁

2026-10-31（Asia/Shanghai）。在已合并#103提案及本人#135/#136/#137的显式schema9、私有指纹、持久一次领取之上，新增Auth内部派生的原trial授权；没有接Core业务派发或另造网络事务。流程依据为#103 Owner APPROVED并合并，同时保留记录中“未逐行技术审核/独立测试”的限制。

## 已实现的私有类型

`Auth::begin_recovery_trial`只消费已提交、仍有效的Begin私有Claim，派生`RecoveryTrialAuthority`。它保留原claim及原trial，期限固定为派生时起最多40秒、grant.max_trial_ms、原Begin能力单调期限的最小值；后续连接/控制不能修改期限。它没有公开构造器/Clone/Debug/Serde/Principal转换，不能给恢复公钥造一个正常Owner/Client。

`check_recovery_trial`是原后台Gate：前后核固定期限和真实Auth/Storage claim。断连、Auth丢弃、verifier更换、撤权/Owner或realm变化、原能力或trial到期仍拒绝；不因观察到grant.active就延长。

Confirm/Cancel各自须先经过真实外部证明适配→Auth私有能力→准确正文指纹→首次登记票据→提交claim，再用`bind_recovery_trial_control`消费其Claim。只有同原trial、完整grant及**相同Auth/连接/运输/verifier/session**才生成`RecoveryTrialControl`。新连接虽然可查询旧回执，也不能用新控制claim接管旧trial。Begin不能转成Confirm，Confirm不能转成新Begin，Cancel也不能充当确认。

`check_recovery_trial_control(trial,control,expected_action)`要求显式ConfirmWifi或CancelWifi，并重核原trial及控制Claim；计划在保存前后都以ConfirmWifi调用。原trial期限保持，控制会话剩余时间只能进一步收紧许可，不能替代40秒。此类型不证明实际内存actor仍在/Ready、NM保存或rollback成功；Storage当前检查仍只认可claimed行，后续观察状态需单独接线。

## 真实测试

新增11项实际Auth/Identity/Owner签名/Store9测试，仅A verifier在cfg(test)是脚本适配器。同一脚本提供不同已验证Begin/Confirm/Cancel动作，session保持相同；不替换verifier来假装原会话的独立确认。

- 60秒能力仍把原trial限定在40秒；较短能力直接限定trial，12340毫秒grant预算进一步缩短；期限在派生调用前后Instant间夹验，确认前后精确相等，无sleep。
- 独立Confirm/Cancel正确，动作互换/Begin充当Confirm/Confirm变Begin均拒绝。
- 外部Auth/grant、断连后的新连接不能操控原trial；verifier替换、当前撤权、确定性trial过期均令后台与保存Gate拒绝。
- 已过期Begin Claim不能重新派生一个新40秒窗口。没有改租约或生产时钟。

完整受测HEAD/命令/摘要见[交付报告](../deliveries/member-d-recovery-trial-authority.md)。没有真实A密码实现、设备、NM、网络/Wi-Fi/电话/短信或蜂窝操作。

## Core接线保留的具体边界

当前T5合同的`crypto_wire_frozen=false`，外部标准安全会话/正文编码还未提供；Core接线不得自行把另一来源的SSID、口令、target trial或查询UUID混入已认证正文。当前Auth准备/查询接口仍要求可信Core从**同一认证正文**严格解析语义。没有在本包另定A的加密wire、增加普通AuthCommand或payload“verified”布尔。

原Core `network_trial::provision`已有唯一ACTIVE/TRIALS、global coordination网络/电话锁、原ManagerGate、40秒/checkpoint/有限rollback。下一次实际接线须将Session所有权改成互斥Normal(原Principal)/Recovery(原trial授权)分支，Control携带独立私有RecoveryTrialControl；普通路径保持原身份/权限。保存前后必须核对应Confirm控制，不能仅调用原Begin后台Gate。Recovery与普通Owner/Client不能互操。

还须接同writer的业务观察/可信终态证据、原actor仍存活/Ready/剩余期限、T5有限状态/Lookup映射；保留claimed/Unknown墓碑和丢响应只查原ID。现有Backend的USB检查不能全局移除，只能在持有获核Recovery来源的明确分支处理；不改WWAN/IMS/普通APN/蜂窝回退。A安全端口缺失仍拒绝，Root/实际UID/发行schema9兼容和设备验收保留。

本包只交可复用Auth授权组件及真实测试，不声称Core已开始后台trial、Confirm已保存或端到端恢复可安装。完整步骤仍21/26、剩D22–26五步、整体约80%。
