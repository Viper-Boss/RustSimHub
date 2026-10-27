# RustSimHub 五人协作入口

日期：2026-10-27。现为负责人加四位离线协作者；用户已要求改为本仓库直接分支协作。原A/B任务书从18349a8起登记，五人扩展从70313db起登记；实际开工从最新 `origin/main` 建立分支并记录起始SHA，不能退回旧提交覆盖新代码。原P0–P7整体80%；分配任务或发出邀请不增加验收比例。

| 人员 | 责任 | 本轮任务 | 设备 |
|---|---|---|---|
| Viper-Boss / 项目负责人 | 总体设计、主线开发、设备集成、审核与合并 | 主线开发暂停，UPD-01待续；负责跨模块审核、设备与签名交付 | 由负责人操作 |
| 成员A：[fungi1220](https://github.com/fungi1220) | NKN活跃安装统计的独立Rust组件 | [A任务书](MEMBER-A-USAGE-STATS.md)，STAT-01～07、追加NKN-01/02和SEC-05离线部分（16步） | 无需设备 |
| 成员B：wnfhsj（用户已确认，主页查询异常） | 网络月计量、异常边界与展示 | [B任务书](MEMBER-B-NETWORK-ACCOUNTING.md)，NET-01～03、追加WEB-01/05和STAB-01离线部分（16步） | 无需设备 |
| 成员C：[yh1030](https://github.com/yh1030) | NAS只读兼容、历史导入与幂等 | [C任务书](MEMBER-C-NAS-COMPAT.md)，NAS-01/05/06、追加REC-04/UPD-06及REL-05草稿（16步，已审4步） | 无需设备 |
| [RedamancyGTW](https://github.com/RedamancyGTW)，成员D | 短信事件同步、对象换代、故障恢复 | [D任务书](MEMBER-D-SMS-RECOVERY.md)，SMS-02～06、追加CALL-08离线实现（12步） | 无需设备 |

四名成员均使用自己的GitHub账号和Codex；不用共享负责人账号、Token、NAS地址或SSH材料。旧Claude任务包仅供历史参考，本目录是本轮明确分工。yh1030与wnfhsj的Write最终回读已生效；fungi1220和RedamancyGTW的Write邀请已就绪，等待本人接受。此前wnfhsj与RedamancyGTW的公开主页有404现象，不能据此判断账号不存在或确定风控原因。权限明细见[仓库访问状态](ACCESS.md)。

原A/B/C/D各8步保留。2026-10-27追加后分别16/16/16/12步，共60个成员交付步骤，映射原有项目动作，不向169个动作加28个，不重复累加项目工时。原成员C的4步已审记录保持；分母变化来自新增分工，不是撤销验收。计划区间含原包与追加包，不代表剩余或实际工时。详见[追加任务与执行顺序](ADDITIONAL-OFFLINE-PACKAGES-2026-10-27.md)。

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

以下为原任务边界；追加授权只按本人任务书末节和追加总表扩展，不一并开放其他路径。

- A独占新增 `crates/usage-stats/`、相关文档与测试，以及新增该成员所需的workspace/Cargo.lock最小改动。
- B只改既有 `crates/network-agent/` 和明确列出的网络显示/测试文件，不调整workspace成员、wire协议或Core。
- C只改 `crates/compat/`、`tools/nas-adapter/`及本人的文档，不改Core/wire/storage接口。
- D只改明确列出的storage收件/操作回执及observer测试/最小修正，不改compat、network-agent、统计组件或Core授权/派发。
- 主线继续处理Core授权/派发、AP、Edge主入口、系统单元、应用签名与设备维护。四位均不修改 `docs/progress/`、本目录的成员/审核数据、HANDOFF顶部或整体百分比；交付证据写各自报告，审核后由主线更新面板。
- 先交小PR，再继续下一包。合同或数据结构需要共同变化时，先在PR给出具体提案，不能自行将变更扩展到另一个人的范围。

## 交付与审核

使用[统一交付模板](PR-DELIVERY-TEMPLATE.md)。每个PR必须包含起始SHA、完整40位HEAD、真实测试命令和结果、范围内文件、未完成/未运行项、来源与许可说明、AI辅助情况。原始日志只放不含秘密的精简证据；不上传真实短信、号码、卡信息、设备身份、生产数据库或私有维护目录。

审核顺序：范围/契约 → 静态代码 → 对应HEAD的测试证据 → 主线集成 → 必要的ARM/设备验收。CI成功、PR标题、自测说明、合并状态均不能自行等同于全部功能通过。PR新增提交后，原审核结论必须按新的HEAD复核。

只有负责人在审核记录中写明对应HEAD与验收范围，才计入“已验收工作”；离线完成和设备完成分别记录。不能为刷代码量加入重复实现、生成物或无意义测试。

## 代码量、工作量与资料留存

[贡献统计口径](CONTRIBUTION-RECORDS.md)规定当前主线非空源码行、Git增删量、提交数、任务验收和自报工时各自的含义；所有数字必须附统计提交截面。未合并PR代码不进入主线代码量，分配给某人也不等于已完成。

为后续整理软件登记资料，保留版本名称/版本号、带时间的Git历史、人员与账号对应、任务书、逐HEAD审核、合并记录、来源/许可及发布说明。工程面板记录过程，不生成或裁定著作权归属、署名比例。合作成果的归属与使用约定由参与者另行明确，不能靠改Git作者或代码行数代替。

后续更新账号/权限：先更新 `roster.json` 中既有成员的login与显示名，保留稳定ID；核对真实Git作者映射后再刷新统计。实际仓库授权单独执行并记录结果，不通过JSON字段假装已授予权限。
