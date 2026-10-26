# RustSimHub 开发进度看板

在线地址：**https://viper-boss.github.io/RustSimHub/**（以Pages部署成功及实际访问结果为准）。本目录只包含公开开发资料；最终发布产物严格限定为`index.html`、`progress.json`、`remaining.md`。

- [打开单文件HTML](index.html)：动画、原计划进度、全部模块、逐动作验收、依赖、估时、筛选、模块接手复制及更新记录；下载后也可离线阅读。
- [完整Markdown清单](remaining.md)：25模块、169个当前动作及9个搁置/按需动作。
- [结构化台账](progress.json)：唯一状态源；HTML和Markdown由它生成。

## 百分比

原P0–P7仍依据`docs/PROGRESS-SCALE.md`的100个固定权重点，目前80%；余下20点与X1转发、X2 NKNGuard、X3统计、BLE全部保留。行动台账另外以每项冻结估时的中值作为权重，只有完成且附证据才计分。当前台账已有13个完成动作（含安装、默认OFF、Bark手机验收、Owner配网契约/网页、首次认领接管、联合安装权限验真、转发规则/隐私策略、关开游标及显式历史补发），剩156个；6.4%是这份新台账的加权率。整机仍按原P0–P7计算80%，实机首配与保存冷启动继续单列验收。

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

示例是维护命令说明，不表示这些动作已经执行。`complete`要求依赖先完成、公开脱敏证据存在；不会自动提高原80%。重复同一完成回执不会重复加分；不能静默重开已完成动作。HTML/Markdown在同一次更新中重建，完成当前焦点后自动选择同模块已满足依赖的下一项，也可显式选焦点。没有新的SMS授权、用户配合或外部条件时，不能据此调用设备副作用。

提交台账、生成产物及相应证据后，`.github/workflows/progress-pages.yml`验证再自动发布。看板是仓库快照；发布器不连接设备、不能自动判定验收。网页“读取最新进度”只读取同站的公开JSON，失败保留当前快照。

新增/移除范围时，显式修订`baseline_version`并记录scope变更、动作数量和权重。搁置的关机问题不会因为生成看板被重新启用。未来BLE配置器计划位于同一Pages的`/configurator/`，应扩展统一静态发布产物，不能用另一条流水线覆盖看板。

## 本地重建

```sh
python tools/progress/build.py
```

标准库Python + 原生JS/CSS，无外部字体、CDN、统计脚本或设备后端。桌面及390/320px手机的实际DOM检查、动画暂停/减少动态效果、搜索/筛选、失败保留和单文件离线读取均已验证；截图为开发验收资料，不用于伪造设备运行状态。

2026-10-26 依赖基线 v1.1：AP-06 前置由 AP-03/05 修正为 AP-01/05，让联合材料先于 AP-03 的真实保存后冷启动验收准备。所有动作、验收、估时及权重保持；说明见 [P4Q](../VALIDATION-SETUP-AP-CLAIM-ADOPTION-P4Q.md)。
