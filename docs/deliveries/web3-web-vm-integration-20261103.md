# WEB3-WEB 网页夹具与真实入口集成交付

执行账号 wnfhsj88，日期 2026-11-04（Asia/Shanghai）。任务来源为 `codex/web-product-delegation-20261103` 的 WEB3-WEB，任务书 blob `64f0f7c821c827d6b685a8289689cb4aaa69315e`，父任务书 blob `77bbef1fa9a1166b879502375fd9692836443360`。未接管其他窗口，未自行合并、部署或操作设备。

最新 main 起点：`8f9cfeff1a4b5a804b8b138e890f7386caf5a006`。独立分支 `codex/web3-web-vm-integration-20261103`。最终受测代码/测试提交：`f71c7b1840c597f63353b4d9a005d993fda9c90a`；后续提交仅增加本报告。它不依赖未合并的 WEB3-AP/REC/UPD/REL 产品分支。

## 结果与范围

六个指定 VM 入口已修复，均能运行到原业务最终 PASS。新增原生产 NKN 实例/回执别名/IndexedDB/退出竞态检查、公共依赖完整性检查及真实主页装配驱动。生产代码、workflow、旧运行器、窗口1配网页、看板和成员分配均未修改。`git diff 8f9cfeff1a4b5a804b8b138e890f7386caf5a006 -- crates/edge/src/web crates/core crates/wire .github tools/mainline-checks` 为空。

浏览器同时发现一处**未修改的生产 UI 缺口**：退出后 NKN 只读按钮可能仍显示可点。初次“必须 disabled”断言真实失败并保留，不包装成全网页无缺口；会话检查阻止发送 RPC，迟到结果不能恢复成功显示。具体交接见下文。

| 任务动作 | 实际交付 |
|---|---|
| 1 最新 main 基线 | 六个入口逐条 exit 1，全部 `ReferenceError: NknProductPanel is not defined`；与 PR199 同类，但本次独立重跑，不复用旧组合的通过结论 |
| 2 全量依赖盘点 | app.js 有20个模块、38个绑定；盘点每夹具全部缺失导出，详见下表；不只补首个 ReferenceError |
| 3 真实组件 | 公共夹具使用 Node 原生 ESM 加载生产模块及传递依赖；nkn-product.js 原样导入 nkn-status.js，pending/archive 别名直接绑定实际导出；禁止空 NKN 类/替代函数 |
| 4 最小端口 | 保留六夹具现有业务类、请求次数、时钟、截止、IndexedDB 事务和断言；仅补新增真实构造所需 style/属性语义。NKN 新合成 IPC 明确匹配命令和协议，未知命令抛错；浏览器用真实 IndexedDB/时钟 |
| 5 防回归 | 六夹具均断言实际 NKN 实例和初始未知/Remote ON 禁用；实际 app 路径验证 pending/archive 读回、一次显式 manual 请求、损坏记录、退出/pagehide/visibility 迟到响应；初始/登录/只读不隐式启停 |
| 6 Node/浏览器 | 六入口、原 NKN 两检查及所有适合 Node 的原网页检查通过；实际 HTML/ESM/默认 CSP 三宽装配与权限边界通过，保留上述 UI 缺口 |
| 7 负对照 | 自建临时副本六入口正常接线全部通过；删除副本接线后全部因 NknProductPanel 缺失失败。另覆盖38个绑定逐一缺失、NKN 空替身拒绝与浏览器缺模块失败；副本删除并核验原脚本未变 |
| 8 独立交付 | 本报告和独立 PR 给 Viper-Boss 审核；失败/未运行/产品交接与后续组合命令明确记录 |

## 修改文件及依赖盘点

既有文件仅为任务指定六个：

- `tools/test-web-daily-phone.mjs`
- `tools/test-web-daily-sms.mjs`
- `tools/test-web-event-sync.mjs`
- `tools/test-web-session.mjs`
- `tools/test-web-test-center.mjs`
- `tools/test-web-wifi.mjs`

它们增加公共加载/实例检查，并将原 import 移除表达式替换成有限、可核验的 `appVmSource`。保留原生产构造语句和全部原断言。test-center 仅额外开放测试侧 NKN 访问并追加 app/NKN 集成场景。

新增：`tools/fixtures/web3-web/app-imports.mjs`、`assembly-browser.py`，以及 `tools/test-web-app-imports.mjs`、`test-web-app-imports-negative.mjs`、`test-web-app-assembly.mjs`。公共夹具解析当前有限 named-import 语法；遇新语法/重复绑定/缺导出立即失败。已有针对其他功能的隔离替身保持；未提供的导出从真实模块补齐，NKN 三个绑定强制要求真实导出身份。不会把已有真实目标组件替换成 mock。

所有六夹具共同缺失的10个绑定：`ApConfigurator`、`apPendingInput`、`deriveSeed`、`importKeyPair`、`seedPublicKey`、`NknProductPanel`、`nknPendingInput`、`nknArchiveInput`、`readNasJournal`、`clearNasJournal`。这包括当时尚未进入的分支，不把它们全算作已发生的 ReferenceError。

| 夹具 | 共同10个之外缺少的绑定 | 合计 |
|---|---|---:|
| daily-phone | prove / unhex / hex / archiveEndedOperation / smsListText / smsDetailText | 16 |
| daily-sms | prove / unhex / hex / archiveEndedOperation | 14 |
| event-sync | prove / unhex / hex / archiveEndedOperation | 14 |
| session | archiveEndedOperation / smsListText / smsDetailText | 13 |
| test-center | prove / unhex / hex | 13 |
| wifi | prove / unhex / hex / archiveEndedOperation / smsListText / smsDetailText | 16 |

盘点来自实际执行时 context 的注入集合；完整 JSON 仅在本包私有日志。原 forward/media 等目标测试继续使用各自既有隔离端口；NKN 没有复制一套算法。新增的 app/NKN 测试走实际 `atomicStored`、`nknPendingInput`、`nknArchiveInput` 和生产 reset/权限检查。

## 精确验证与原输出

环境复用 Node 24.19.0、Chromium 和已有 Python Playwright，无依赖安装或锁文件变更。`WEB3_WEB_BROWSER_PYTHON` 可选择已装 Playwright 的解释器，`WEB3_WEB_CHROMIUM` 可选择已装 Chromium，默认系统 Chromium；可选 `WEB3_WEB_BROWSER_ARTIFACT_DIR` 仅保存合成截图。不公开本机路径。

六入口在起点上逐一失败；在最终受测 SHA 上执行相同命令全部 exit 0：

| 命令 | 原业务最终输出摘要 |
|---|---|
| `node tools/test-web-daily-phone.mjs` | PASS: production daily call controls, missed-call confirmation, incoming/reject, preconnection, ACTIVE audio, lease renewal, durable single submits, uncertainty, cancellation/logout/storage/poll races; synthetic only. |
| `node tools/test-web-daily-sms.mjs` | PASS: production daily SMS, single confirmation and submit, durable markers, unknown receipts, storage/logout/draft races, safe content and list previews; synthetic inputs only. |
| `node tools/test-web-event-sync.mjs` | PASS: bounded catch-up and real-app SMS/calls/history pagination, candidate review wiring, stale selection/detail/source isolation, logout, retry and cursor commit races; no network or hardware. |
| `node tools/test-web-session.mjs` | PASS: signed continuation clock, conservative deadline, single flight, timeout/logout/replay guards, and real app/proof/phone wiring without media reconnect or new call submission; synthetic only. |
| `node tools/test-web-test-center.mjs` | 原五任务/组件挂载/回执/脱敏诊断/转发实际app与IndexedDB/配置读取及时限竞态 PASS；新增实际 app/NKN 别名、持久化、被动读、退出/损坏/迟到响应检查 PASS |
| `node tools/test-web-wifi.mjs` | Web Wi-Fi catalogue, selection, session fencing and retained trial query passed |

其他针对性命令：

- `node tools/test-web-app-imports.mjs`：20个真实模块/38个绑定、别名身份、38个逐一缺失负例、NKN 三个空替身拒绝，通过。
- `node tools/test-web-app-imports-negative.mjs`：六个正对照 exit 0，六个移除接线的副本 exit 1 且精确匹配 NknProductPanel ReferenceError；原脚本字节未变，临时目录已删除。
- `node tools/test-web-nkn-product.mjs`、`node tools/test-web-nkn-status.mjs`：原组件检查通过。原状态年龄、顺序、epoch、存储、Unknown/不重发及默认不启停约束保持。
- `node tools/test-web-app-assembly.mjs`（选择已有 Playwright 解释器）：真实主页 320/390/1280 三宽通过装配及权限边界检查；使用 Linux 入口的原静态资源集合和原默认 CSP，没有 unsafe-eval 或 bypass_csp。真实 ESM 传递依赖、WebCrypto 双向 Ed25519 证明、IndexedDB 重开、原ID查询、损坏 pending、503读取失败和退出迟到响应均检查；全部三宽记录 UI 缺口。另一次浏览器负对照阻断 nkn-product.js，正向“8个按钮挂载”断言确实失败，且无 API 请求。
- `node --check` 对六个修改入口及四个新增/公共 MJS逐一执行，通过；Python `ast.parse` 新浏览器脚本及 `git diff --check` 通过。

扩大探测时按起点 Git 树枚举29个根目录 `tools/test-web-*.mjs`，原始结果为28 exit 0、1 exit 1。失败者是 `test-web-recovery-session.mjs`：它是浏览器专用模块，却被误当 Node 入口，因缺 `/wasm/rustsimhub_recovery_session.js` 在导入前失败。其注释及旧运行器 README 均明确应由 `tools/run-web-recovery-session.py` 携 native peer/WASM/Chromium 执行。本包未修改它或旧清单，不补伪 WASM 让 Node 假通过；该原浏览器专用正入口本轮 **not_run**。28个适合 Node 的原网页入口均实际 exit 0，包含全部六入口、NKN、AP、首配、证明、媒体、网络、客户端及NAS等检查；不是29项全绿，也不是设备验收。

最终完整逐入口结果 JSON、原 stdout/stderr、基线、负对照、清理结果和浏览器合成截图仅在本包私有忽略目录，未提交原始日志或任何私钥。

## 真实失败与范围外产品交接

1. main 六入口真实基线：NKN 构造前 ReferenceError，各 exit 1。原失败记录保持；当前结果不能追溯更改 PR199 的历史失败。
2. 浏览器首次夹具调用 `wait_for_function` 使用表达式字符串，被原 CSP 的 unsafe-eval 禁止，exit 1。改成函数谓词后运行，未放宽生产 CSP，也未启用 Playwright CSP 绕过。
3. 第二轮浏览器真实断言 `logout 后只读按钮必须 disabled` 失败。原因是生产 `app.js:607` 的 logout 包装先 `nknProductPanel.reset()`，再执行原退出清 token；`nkn-product.js` 的 reset/update 因而看到旧 token。三个宽度均可复现。报告与后续浏览器输出均显式标 `KNOWN PRODUCT GAP`，没有把此 UI 断言改写成已修复。
4. 后续装配测试记录按钮实际外观，并明确验证点击仍由 `current()` 的 token/epoch 守卫拒绝，不产生 nkn_status 或启停 RPC；Remote ON/写按钮仍禁用，迟到回复不恢复服务成功显示。**这些权限边界通过不等于 UI 按钮禁用缺口修复。**
5. 交 Viper-Boss 的生产整合后续：退出时在 token 失效之后再重置/更新该面板控件，并在对应生产文件归属下补 UI 回归。当前任务书禁止窗口5修改任何生产组件，因此未顺手修改 app.js 或 nkn-product.js。定位来源固定为 main `8f9cfeff1a4b5a804b8b138e890f7386caf5a006`；该缺口与 VM 依赖缺失不同，不是正式浏览器 ReferenceError。
6. 扩大入口探测的 Node/browser 类型错误、上述两个浏览器失败及六个有意移除接线的负对照均保留，不称作云端 CI 通过。

## 来源、边界及后续组合

旧 PR195–200 开工逐个回读，均 open、非草稿、未合并，HEAD 分别为：

- 195：`6787addaf4ca0e79f8b8455cbb3175372c9c318c`
- 196：`395a07eea72a074f3126466d03a2b899787ac9eb`
- 197：`ecfb9fe7d8c4b7f940db82cd9ea52b1ebb71b810`
- 198：`32d340f9701cb73d36fbdc443bfcf6ef7c08ff6a`
- 199：`fb77eaf63a7ef50a8da229664fe37129a9a0bd6f`
- 200：`a690213cec950b273632f595b96a0bea8f8e5e64`

与父任务书固定值一致，未重做或合入无关产品分支。父任务书已注明第4包由负责人接手并交 PR205，本包没有重复领取。

主线接纳本包与其他产品包之后，应在新的独立组合分支记录完整 HEAD，再运行以下命令（本报告结果不背书未来组合）：

```sh
node tools/test-web-daily-phone.mjs
node tools/test-web-daily-sms.mjs
node tools/test-web-event-sync.mjs
node tools/test-web-session.mjs
node tools/test-web-test-center.mjs
node tools/test-web-wifi.mjs
node tools/test-web-nkn-product.mjs
node tools/test-web-nkn-status.mjs
node tools/test-web-app-imports.mjs
node tools/test-web-app-imports-negative.mjs
# 先选择已装 Playwright 的解释器，再运行真实主页装配：
node tools/test-web-app-assembly.mjs
# 若组合包含窗口1产品，使用其既有原入口：
node tools/test-web-ap-wifi.mjs
node tools/test-web-setup.mjs
```

未运行：上述独立 WASM/native peer 浏览器专用入口、Root/PID1、ARM 构建、物理 Wi-Fi/AP/RF/GATT、真实 NKN/TURN、设备/NAS、签名安装或部署。没有 Rust/生产代码变化，本包不借用其他分支 Cargo 通过数；真实主页外部 API 是合成设备，不是实际 Core/Root/引擎联调。

来源为仓库既有 AGPL-3.0-only 代码；新增夹具同许可证。使用 Codex 辅助实现与验证，没有复制第三方状态机、共享密钥或重写历史作者。

本包测试修复可供审核，生产 UI 交接和整机验收仍未关闭。整体仍 **81.5%**，20/169、加权10.6%；本包8个动作、测试入口数和 PR 数不增加原完成率。云端最终检查和精确 PR HEAD 在 PR 正文回读补充，不提前宣称 CI 通过。
