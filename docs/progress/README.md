# RustSimHub 开发进度看板

在线地址：**https://viper-boss.github.io/RustSimHub-progress/**（以Pages部署成功及实际访问结果为准）。本目录只包含公开开发资料；最终发布产物严格限定为`index.html`、`progress.json`、`remaining.md`。

- [打开单文件HTML](index.html)：动画、原计划进度、全部模块、逐动作验收、依赖、估时、筛选、按人任务清单及更新记录；下载后也可离线阅读。
- [完整Markdown清单](remaining.md)：25模块、169个当前动作及9个搁置/按需动作。
- [结构化台账](progress.json)：唯一状态源；HTML和Markdown由它生成。

## 百分比

原P0–P7依据`docs/PROGRESS-SCALE.md`的100个固定权重点，当前 **81.5%**，还剩18.5点。行动台账另按冻结估时中值加权：**20/169**项完成，**10.6%**，剩余149项；另9项搁置/按需动作不进分母。新扩展不混入原P0–P7。

2026-11-03从负责人实际发布仓库的固定快照对齐，修正源JSON/里程碑表和本说明落后；没有新增验收或改权重。SMS-05原报告未在main找到，沿用负责人公开完成记录并注明缺口；STAB-03是用户后来接受6.4小时样本，不冒称24小时。来源、固定摘要和边界见[全看板复核](../VALIDATION-DASHBOARD-REFRESH-20261103.md)。

PR178/180/184/187/191均已有负责人合并记录。D线26/26离线候选集中PR170，PR162已关闭并删除重复分支，后续统一交Viper-Boss审核。各模块展示代码交付证据及原验收条件；源码、隔离测试、合并、正式安装和实机验收分别记录。

## 按人看任务

团队区将任务直接放在五人各自卡片下。四位成员共118个具体步骤，取自任务书；90个离线负责项目动作和65个主线执行动作分别列出，主线继续审核/集成成员交付。步骤已审、整项闭环和代码贡献分别显示；合并PR不自动记为验收。PR记录同时显示待合并/已合并/已关闭与逐HEAD审核结论，最新PR在前。

可按姓名、步骤ID或动作搜索，也可跳转人员、显示全部任务分组、复制个人清单。展开步骤可看工作、验收和来源；点击项目动作可看原始依赖、估时与实测前置。详见[T7验证记录](../VALIDATION-PERSON-TASK-BOARD-T7.md)。

任务书变化后先运行`python tools/progress/team.py --ref origin/main`刷新团队与步骤快照，再运行重建命令；需要已更新的主线引用和GitHub读取能力。生成器核对任务书SHA-256，拒绝悄悄沿用过期来源。普通样式编辑直接重建即可。

工时是首次工程工作量粗估，不是机器运行时长或保证交付日期。页面可按每天4/8/12/16/24个有效小时换算；换卡、真人听音、第二设备、服务器、BLE授权等待单独说明。24/72小时长测可与其他开发重叠。后续记录实耗，再校准未开展任务的预测；原始权重的变更需明确基线修订，不能静默改分母。

## 每次完成怎么更新

```sh
python tools/progress/update.py state FWD-03 --state in_progress --note "开始已准备好的实机维护安装"
python tools/progress/update.py complete FWD-03 --evidence docs/实际验证记录.md --note "完成情况与验收结果" --actual-hours 1.0
python tools/progress/update.py focus AP-01 --note "后续准备开展的具体动作"
python tools/progress/build.py --check
python -m unittest discover -s tools/progress -p 'test_*.py'
node tools/progress/test-dashboard.mjs
```

示例是维护命令说明，不表示这些动作已经执行。`complete`要求依赖先完成、公开脱敏证据存在；不会自动提高原P0–P7百分比。重复同一完成回执不会重复加分；不能静默重开已完成动作。HTML/Markdown在同一次更新中重建，完成当前焦点后自动选择同模块已满足依赖的下一项，也可显式选焦点。没有新的SMS授权、用户配合或外部条件时，不能据此调用设备副作用。

当前线上内容由`Viper-Boss/RustSimHub-progress`发布；本仓库保留`.github/workflows/progress-pages.yml`。此次仅提交PR，不触发Pages发布。负责人审核后须核对实际发布仓库、流程及线上文件，只有发布成功并实际访问核验后才称上线。看板是仓库快照；发布器不连接设备、不能自动判定验收。网页“读取最新进度”只读取同站的公开JSON，失败保留当前快照。

新增/移除范围时，显式修订`baseline_version`并记录scope变更、动作数量和权重。搁置的关机问题不会因为生成看板被重新启用。配置器当前冻结基路径仍是`/RustSimHub/configurator/`，与实际看板仓库不同；最终路由/统一发布组合由主线审核，本轮不改配置器路径或发布配置。

## 本地重建

```sh
python tools/progress/build.py
```

标准库Python + 原生JS/CSS，无外部字体、CDN、统计脚本或设备后端。桌面及390/320px手机的实际DOM检查、动画暂停/减少动态效果、搜索/筛选、失败保留和单文件离线读取均已验证；截图为开发验收资料，不用于伪造设备运行状态。

2026-10-26 依赖基线 v1.1：AP-06 前置由 AP-03/05 修正为 AP-01/05，让联合材料先于 AP-03 的真实保存后冷启动验收准备。所有动作、验收、估时及权重保持；说明见 [P4Q](../VALIDATION-SETUP-AP-CLAIM-ADOPTION-P4Q.md)。
