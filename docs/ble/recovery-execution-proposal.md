# D-23：当前 grant 格式上的持久 Wi-Fi 请求提案

2026-10-30；**待审接口/迁移，不是已实现或可安装的恢复事务**。
起始主线 `6b5de5f3ca5ff706adacd8f25ed7f638546739a9`；依赖 #97/#101 的授权/运输接点，
但本提案的失败样例不依赖它们合并，不修改旧分支或引入旧迁移。

## 当前代码与需补的边界

- #84 的 `recovery_grant_receipts` 是批准/撤销管理快照，没有 trial/state/settings_saved；
  不是恢复请求→原 Wi-Fi 事务的执行账本。不能把该查询返回成 T5 Operation/Lookup。
- 当前 `network_trial::provision` 已有唯一 ACTIVE/TRIALS、global coordination network 锁、
  ManagerGate、40秒试连、有限NM checkpoint、确认前持久 Audit 和失败精确补偿。
  Session/Audit/控制只绑定完整正常 Principal，不能给恢复公钥造一个 Owner/Client。
- 当前 `run_trial` 保存前后均核 Gate，失败时调用原 backend rollback；7项既有真实函数
  测试覆盖确认/部分保存/撤权/失败回执/权限，不能另造一个恢复 Wi-Fi 状态机。
- 旧 #63 `ad8b87a812317018994bcb5f6df4d00ce3a1842f` 的账本依赖旧 #60/#61 schema7。
  原 schema8 的 format1/字段没有当前设备/Owner约束，不能直接合入或改号冒充转换。

## 显式 schema9 候选

采用 **7→9** 的独立显式迁移，保留8作为不支持的旧实验格式；不复用版本号。
[逐字 SQL 候选](recovery-execution-schema9.sql)只由本次 cfg(test) 在 SQLite 内存副本执行。
没有生产 include/迁移/OpenMode/CLI，默认 open/restore 仍最高6，显式 grant 入口仍只接受7。

计划中的 `recovery-execution` feature 依赖 recovery-grants，新增显式 migrate/open/restore
执行入口。迁移必须在同一个 writer/锁内：验证当前精确7目录 → 完成 Backup API快照 →
事务创建候选meta/表/index、验证 → 提交9。失败保持原7和完成备份，不解锁重开迁移。
不能从6直跳、从8猜字段、删表降级或伪改 user_version；源8保持原字节并拒绝。

9入口须验证完整 grant DDL及新 meta/model、表/index、FK、行/容量/跨行关系，
不能只看 user_version。grant管理和Auth读取适配9时必须走已验证的9模式，不能让普通
7入口自动接纳9。主线 UPD-01/当前候选/回退组均未声明9支持；生产启动仍6，禁止部署迁移。

表只保存：格式2、realm/device pin、Owner id/generation、grant id/generation、规范request UUID、
三种写动作、固定宽请求指纹、有限状态、原64hex trial id及保存/回滚证据。
meta固定 `fixed-ed25519-execution-v2` / grant_format1，避免和旧8同名结构误读。
UUID/十进制代次/动作/各状态语义仍须Rust严格验证，SQL约束和JSON active字段不代替授权。

### 指纹与秘密

不存SSID、口令、会话/私钥、原始包或可反序列化能力。旧#63裸SHA body_digest不直接复制：
持久化可猜口令的裸摘要会提供离线字典验证材料。候选用已有HMAC-SHA256机制、独立
Identity私有域派生去重key；域/长度编码/向量须单独冻结，不重用SIM或会话key、不选A密码套件。
输入绑定完整设备/realm/Owner/grant代次、request UUID、动作、target trial、约束和确切规范正文。
该指纹只作同请求参数比较，不是密码证明；数据库单独泄露不能直接拿它验证猜测口令。
此PR没有实现派生或宣称秘密备份被盗后仍可保密；正式Identity/业务备份仍须既有私有保护。

## 领取、执行与状态

| 阶段 | 候选内部动作 | 不能推出的结论 |
|---|---|---|
| reserve | 验证当前grant并原子登记UUID/指纹；相同输入返回既有记录，不给执行permit | existing/not_found均不是允许重发的证明 |
| claim | 同一writer事务再次核grant与无锁存活/单调期限fence，reserved→claimed，返回私有一次性permit | 没有permit不得调用NM；claimed也不表示NM已成功 |
| begin | 分配原引擎trial id并在首次外部副作用前写入claim，走同一个网络锁/actor/backend | 不创建第二网络事务或副本Principal |
| confirm/cancel | 必须关联同realm/grant代次/原trial，当前动作能力、原actor状态和剩余期限 | 新连接不延长40秒，不拿300秒会话替代试连上限 |
| observe | 仅可信backend完成后记录starting/connecting/ready/saving及有证据的终态 | 不接受网页JSON的saved/rollback_verified布尔作事实 |
| reopen/restore | 非终态保留为unknown；原trial/请求保留，不排队重新执行 | 不重发、不造新UUID、不因换realm或缺内存actor清Unknown |

同UUID不同指纹/动作/target拒绝Conflict。所有已有/Unknown查询为只读，不重复claim。
容量全局256（含旧realm/墓碑）；begin/confirm最多占到255，给唯一活动事务预留一次cancel。
满时拒绝新动作、保留查询和预留cancel；不以TTL/自动清库释放墓碑。容量后续归档需独立设计，
不将行数限制写成已具备长期无限运行能力。

存活fence只做内存原子标记/单调时间读取，不能在writer回调里取Auth mutex、调用Store或await，
否则会死锁。密码/动作/正文由同Auth私有能力提前核，writer内再原子核数据库授权与领取。
这是可信源码接点，不能从网络提供一个bool/closure当证明。外部NM调用后的撤权由原有限补偿
处理，claim不是后续永久许可；save前后仍走同一Gate。需评审这个claim线性化点再实施。

`saved`仅在实际持久保存确认后 `settings_saved=true`；`rolled_back`仅在精确补偿验证后
`settings_saved=false,rollback_verified=true`；其余证据为空。SQL防止空值穿过这些终态条件。
Begin回执可观察同trial后续确认产生的终态，但没有“begin自动保存”动作；跨行证据由writer核对。
内部reserved/claimed/rejected不是新的T5 wire枚举，映射必须单列，不能原样透传。

Unknown不得通过“无内存Session/没找到profile/恢复换realm”自动标成功或未执行。
未有充分原trial对账证据时保留Unknown，阻止新的BLE写入；主线的Owner人工诊断/对账流程另审。
不把未知行永久占着普通网络锁，也不将正常Owner显式维护当作旧请求已被证明解决。

## 接现有 Wi-Fi 唯一权威

候选用内部 `TrialOwner::{Normal(Principal), Recovery(binding)}` 替代仅Principal的所有权字段，
保持普通AuthCommand和正常权限原样。Recovery binding由私有能力建立，绝不造Owner/Client。
仍使用原ACTIVE/TRIALS、coordination network锁、电话协调、40秒和原NM checkpoint/保存/回滚。

USB门禁只在获核的Recovery来源分支明确例外；未认证无USB仍拒绝，其他原门禁不放宽。
不启普通APN、不改WWAN/IMS，不隐式蜂窝回退。recovery max_trial_ms只可缩短原40秒预算。
普通Owner/Client与Recovery trial相互不能操控；同grant新连接可只读查询原回执，
旧连接失效触发原有限补偿，不自动接管/重开/延长trial。Cancel关闭运输与取消业务严格区分。
恢复actor后台Gate须有派生的原trial授权快照，不能把前台Begin capability简单当成Confirm许可。

## 当前真实样例与待审点

本次新增4项真实Store/SQLite测试：管理快照没有执行字段/表；在当前7盲叠旧8被两入口
NewerSchema拒绝且源库字节/行保持；真实Backup API快照恢复8被拒绝、源保持且目标保留隔离标记；
候选9只在内存副本执行，grant DDL和原7不变、证据缺失/无trial等非法状态被SQL拒绝。
旧DDL夹具来自#63：blob `8de2ba8111c13865b1241411154a0a2a69c098a1`，
SHA256 `f27762490288549feff40ddc09050f050835f9d7c6851adf032bf59935aa0ac1`；
构造的是“新7+旧账本DDL”的故意混合8，不冒称原旧#63数据库或正式迁移。
7项现有真实run_trial测试运行通过，不重复编造新恢复状态机。

请按PR精确HEAD审核：跳过旧8、显式7→9/默认拒绝；指纹独立私有域；唯一claim/fence；
容量cancel预留；Unknown与正常Principal/Recovery分支；跨行终态证据和Owner对账边界。
审核前不接生产；#97/#101和A会话/GATT/真实UID依赖、更新/回退9兼容、ARM/设备验收仍保留。
完整数量/测试见[交付报告](../deliveries/member-d-recovery-execution-contract.md)。
