# RustSimHub 五人协作入口

2026-11-15 用户要求把其余可离线完成的工作集中拆成**两份**供学妹的Codex并行运行：[A包·生产代码/业务接线](OFFLINE-A-PRODUCTION-20261115.md)、[B包·离线集成/未签名交付](OFFLINE-B-INTEGRATION-20261115.md)。包含可直接复制的启动指令、固定基线、互斥文件范围、共同接口、执行步骤和逐动作映射：93项业务动作关联A、29项系统/发行动作关联B，27项蓝牙动作留原窗口；各动作的真实执行部分仍归负责人。这是新的执行分派，A/B不是历史成员账号；不改历史作者/审核/权重，不假定学妹账号或已授予权限。下文旧成员表保留历史，当前本轮离线执行边界优先看上述两份任务书。分派不增加验收百分比。

当前分配日期：2026-11-03。现为负责人加四位离线协作者；用户已要求改为本仓库直接分支协作。原A/B任务书从18349a8起登记，五人扩展从70313db起登记；实际开工从最新 `origin/main` 建立分支并记录起始SHA，不能退回旧提交覆盖新代码。当前P0–P7整体验收81.5%；分配任务或发出邀请不增加验收比例。

| 人员 | 责任 | 本轮任务 | 设备 |
|---|---|---|---|
| Viper-Boss / 项目负责人 | 总体设计、主线开发、设备集成、审核与合并 | UPD-01版本契约；维护主线CI、跨模块审核及设备/签名交付 | 由负责人操作 |
| 历史成员A：[fungi1220](https://github.com/fungi1220) | 当前无分配；原提交与审核保留 | 原A-01～30已转交wnfhsj88 | 无设备任务 |
| 成员B：[wnfhsj88](https://github.com/wnfhsj88)（原wnfhsj） | 网络/网页/PWA及接手的统计/NKN/安全/邮件离线实现 | [B任务书](MEMBER-B-NETWORK-ACCOUNTING.md)32步 + [A任务书](MEMBER-A-USAGE-STATS.md)30步 + 接手C25～C30六步，共68步 | 无需设备 |
| 成员C：[yh1030](https://github.com/yh1030) | NAS、系统更新恢复及发行证据 | [C任务书](MEMBER-C-NAS-COMPAT.md)，当前保留C01～C24共24步，已有审核保留；C25～C30已转交 | 无需设备 |
| [RedamancyGTW](https://github.com/RedamancyGTW)，成员D | 通信观测、Core BLE恢复及远程媒体 | [D任务书](MEMBER-D-SMS-RECOVERY.md)，累计26步 | 无需设备 |

四名成员均使用自己的GitHub账号和Codex；不用共享负责人账号、Token、NAS地址或SSH材料。旧Claude任务包仅供历史参考，本目录是本轮明确分工。yh1030与wnfhsj的Write最终回读已生效；fungi1220和RedamancyGTW的Write邀请已就绪，等待本人接受。此前wnfhsj与RedamancyGTW的公开主页有404现象，不能据此判断账号不存在或确定风控原因。权限明细见[仓库访问状态](ACCESS.md)。

原A/B/C/D各8步保留；T3为60步、T4为76步、T5为96步，原T6为30/32/30/26共118步。2026-10-30接手A、2026-11-03接手C25～C30后，当前分配为A 0/B 68/C 24/D 26，原A/B步骤ID、项目动作和验收标准不变。90个项目动作的离线部分有负责人，65项保留主线专属执行；所有155项仍按原验收闭环，169分母/估时权重保持；当前20/169完成、加权10.6%。C的9步既有离线审核保持。详细接口、步骤、文件权限和每项主线保留原因集中在[今天分工文档](ADDITIONAL-OFFLINE-PACKAGES-2026-10-27.md#t6-maximum-offline-delegation)，[逐动作映射](offline-ownership.json)可供程序核对。成员粗估不是实际/剩余工时，不与项目台账重复求和。

2026-11-03用户明确要求接手C25～C30：当前由wnfhsj88完成其离线实现，16项对应动作的离线负责人同步调整，总118步骤/90离线动作及原验收/权重不变。C01～C24历史作者和精确HEAD审核保持；具体范围与验收边界见[转交记录](TRANSFER-C25-C30-TO-WNFHSJ88-20261103.md)。

## 现在如何提交

统一使用 **克隆本仓库 → 本仓库中的个人功能分支 → PR到main → 负责人审核合并**，不再使用fork流程。先以本人账号接受仓库邀请，再向个人分支推送；未接受时仍可克隆公共仓库进行本地开发。Write权限用于提交分支；禁止直接推送main、强推主线、修改他人分支或自行合并未经负责人审核的PR。main的实际保护规则和回读结果单列在[访问状态](ACCESS.md)，不能把文档要求冒充已启用规则。

```sh
git clone https://github.com/Viper-Boss/RustSimHub.git
cd RustSimHub
git fetch origin
git switch -c codex/usage-stats-offline origin/main   # 成员A
# 成员B改为：codex/network-accounting-offline
# 成员C改为：codex/nas-readonly-history
# 成员D改为：codex/sms-offline-recovery
git rev-parse HEAD
# 开发、测试、提交；确认当前是自己的功能分支
git branch --show-current
git push -u origin HEAD
gh pr create --repo Viper-Boss/RustSimHub --base main
```

如果仓库已有克隆/remote/未提交工作，先检查，不能按示例强制重置或丢弃工作。确认推送remote确实指向 `Viper-Boss/RustSimHub`；原有个人fork可保留为备用remote，后续交付必须推送到本仓库的个人功能分支。每个人只维护自己的分支；主线变更用fetch后正常合并/变基处理，不覆盖别人的提交。

## 四条分支如何避免冲突

以下为原包的历史边界；最新T5/T6已经把多数离线实现转交成员，新的路径与审批阶段以当日分工文档为准。安全约束继续有效，未明确开放的路径不一并授权。

- A独占新增 `crates/usage-stats/`、相关文档与测试，以及新增该成员所需的workspace/Cargo.lock最小改动。
- B只改既有 `crates/network-agent/` 和明确列出的网络显示/测试文件，不调整workspace成员、wire协议或Core。
- C只改 `crates/compat/`、`tools/nas-adapter/`及本人的文档，不改Core/wire/storage接口。
- D只改明确列出的storage收件/操作回执及observer测试/最小修正，不改compat、network-agent、统计组件或Core授权/派发。
- 最新T6将Core恢复/远程媒体、网页装配、有限系统材料等离线实现按目录转给成员；主线保留接口审核、当前UPD-01收口、真实集成/签名/部署与最终验收。四位均不修改 `docs/progress/`、本目录的成员/审核数据、HANDOFF顶部或整体百分比；交付证据写各自报告，审核后由主线更新面板。
- 先交小PR，再继续下一包。合同或数据结构需要共同变化时，先在PR给出具体提案，不能自行将变更扩展到另一个人的范围。

## 交付与审核

使用[统一交付模板](PR-DELIVERY-TEMPLATE.md)。每个PR必须包含起始SHA、完整40位HEAD、真实测试命令和结果、范围内文件、未完成/未运行项、来源与许可说明、AI辅助情况。原始日志只放不含秘密的精简证据；不上传真实短信、号码、卡信息、设备身份、生产数据库或私有维护目录。

审核顺序：范围/契约 → 静态代码 → 对应HEAD的测试证据 → 主线集成 → 必要的ARM/设备验收。CI成功、PR标题、自测说明、合并状态均不能自行等同于全部功能通过。PR新增提交后，原审核结论必须按新的HEAD复核。

只有负责人在审核记录中写明对应HEAD与验收范围，才计入“已验收工作”；离线完成和设备完成分别记录。不能为刷代码量加入重复实现、生成物或无意义测试。

## 代码量、工作量与资料留存

[贡献统计口径](CONTRIBUTION-RECORDS.md)规定当前主线非空源码行、Git增删量、提交数、任务验收和自报工时各自的含义；所有数字必须附统计提交截面。未合并PR代码不进入主线代码量，分配给某人也不等于已完成。

为后续整理软件登记资料，保留版本名称/版本号、带时间的Git历史、人员与账号对应、任务书、逐HEAD审核、合并记录、来源/许可及发布说明。工程面板记录过程，不生成或裁定著作权归属、署名比例。合作成果的归属与使用约定由参与者另行明确，不能靠改Git作者或代码行数代替。

后续更新账号/权限：先更新 `roster.json` 中既有成员的login与显示名，保留稳定ID；核对真实Git作者映射后再刷新统计。实际仓库授权单独执行并记录结果，不通过JSON字段假装已授予权限。
