# RustSimHub 开发进度看板

在线地址：**https://viper-boss.github.io/RustSimHub-progress/**。2026-10-29已完成独立公开仓库Pages发布与实际HTTPS摘要核对，见[发布记录](PUBLICATION-20261029.md)。固件仓库仍私有。运行页面只有`index.html`、`progress.json`、`remaining.md`三个快照文件；独立公开仓库另外保留页面自身的对应源码、重建脚本、摘要和许可证，共13个白名单文件，不镜像固件或私有材料。

- [打开单文件HTML](index.html)：动画、原计划进度、全部模块、逐动作验收、依赖、估时、筛选、按人任务清单及更新记录；下载后也可离线阅读。
- [完整Markdown清单](remaining.md)：25模块、169个当前动作及9个搁置/按需动作。
- [结构化台账](progress.json)：唯一状态源；HTML和Markdown由它生成。

## 百分比

原P0–P7依据`docs/PROGRESS-SCALE.md`的100个固定权重点，2026-10-29经证据复核为81.5%，余18.5点；新增转发、NKNGuard、统计和BLE独立保留。行动台账另外以每项冻结估时中值为权重，只有完成且附证据才计分。当前交付分支20/169完成、余149、加权10.6%，包含待正常合入的SMS-05记录，不冒充固件main已全部合入。里程碑、收尾/扩展台账和D包交付三种口径详见[进度复核](../PROGRESS-AUDIT-20261029.md)，不能相加混算。

## 按人看任务

团队区五人并排，以同刻度竖条显示固定主线产品代码占比，点击详情才显示个人任务、步骤、PR与审核记录；窄屏横向滚动。118个任务书步骤目前分为lead接回20步、四位成员保留98步。主线实际执行和成员离线职责依唯一映射区分，审核/集成继续保留；步骤已审、整项闭环和代码贡献分别显示，合并PR不自动记验收。

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

示例是维护命令说明，不表示这些动作已经执行。`complete`要求依赖先完成、公开脱敏证据存在；不会自动提高原里程碑点数。重复同一完成回执不会重复加分；不能静默重开已完成动作。HTML/Markdown在同一次更新中重建，完成当前焦点后自动选择同模块已满足依赖的下一项，也可显式选焦点。没有新的SMS授权、用户配合或外部条件时，不能据此调用设备副作用。

提交台账、生成产物及证据并完成验证后，只将固定提交的白名单文件导出到独立公开仓库，正常推送，再等待Pages built并下载页面/JSON/清单核对摘要。私有仓库旧`.github/workflows/progress-pages.yml`本次没有改为自动跨仓库镜像，不宣称已配置自动同步。看板是仓库快照；发布不连接设备、不能自动判定验收。网页“读取最新进度”只读取同站公开JSON，失败保留当前快照。

新增/移除范围时，显式修订`baseline_version`并记录scope变更、动作数量和权重。搁置的关机问题不会因为生成看板被重新启用。未来BLE配置器计划位于同一Pages的`/configurator/`，应扩展统一静态发布产物，不能用另一条流水线覆盖看板。

## 本地重建

```sh
python tools/progress/build.py
```

标准库Python + 原生JS/CSS，无外部字体、CDN、统计脚本或设备后端。旧版有桌面及390/320px的DOM验收记录；最新五人并排版已通过Node、生成一致性及24项台账回归，线上部署/实际HTTPS内容核对通过，但浏览器工具的访问限制/超时使本轮未完成真实截图、详情点击、焦点及新窄屏布局验收，不能沿用旧版证据冒充新版通过。

2026-10-26 依赖基线 v1.1：AP-06 前置由 AP-03/05 修正为 AP-01/05，让联合材料先于 AP-03 的真实保存后冷启动验收准备。所有动作、验收、估时及权重保持；说明见 [P4Q](../VALIDATION-SETUP-AP-CLAIM-ADOPTION-P4Q.md)。
