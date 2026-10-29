# 成员 B：测试中心样式与实际 Edge CSP

账号 wnfhsj88（原 wnfhsj，member-b）；仓库 Viper-Boss/RustSimHub；分支 codex/wnfhsj88-test-center-csp；PR 到 main。
起始 main `5238670246e7c6e43184534a7a6ee4b13c293ffd`；实现/受测源码 HEAD `1577d837893bedbdd8d76f1fcc854aa4577eddd2`；最终完整 PR HEAD 见正文，此后仅本报告。
对应 B-09～12/B-29 候选组件与网页装配前置，样式路径按 T6 明确开放范围修改。

## 已复现及修正

TestCenter 原先动态插入 style 元素。实际 Edge 默认/媒体 CSP 均是 style-src 'self'，该内联样式会被拦截；旧夹具没有 CSP，因此原浏览器结果未覆盖这条生产限制。

先将夹具脚本外置、加载现有 style.css，并由服务器读取 crates/edge/src/linux.rs 的实际默认 CSP。原组件在真实 Chromium 下退出1，记录 style-src-elem / blocked=inline；失败日志保留在工作区 member-b-csp-red.log。

将原规则原样移到已有 style.css 的 .test-center-component 范围，组件不再创建 style 元素。没有修改 CSP、增加 unsafe-inline、变更 Rust 路由或业务行为。夹具只显式提供 HTML/fixture.js/真实组件/真实样式四个本地资源，所有回调仍为离线合成。

## 验证

```sh
source /workspace/toolchains/activate-rustsimhub.sh
python3 tools/fixtures/test-center/browser.py --output /workspace/setup-logs/test-center-csp-green
python3 tools/fixtures/usage-display/browser.py --output /workspace/setup-logs/csp-daily-browser
# 当前 main 基线全部24个 tools/test-web-*.mjs 逐个运行
git diff --check
```

以上均退出0。真实 Chromium 1440/390/320 在实际 CSP 下没有策略违规；断言 flex 布局、按钮 min-height≥44px、键盘 focus-visible 实际生效，长中文无溢出，调试默认收起，禁用原因/Tab/Enter及一次明确检查回调保持。实际日常页折叠/展开电话、短信和AP三尺寸无溢出，未触发业务提交。

当前主线24个网页脚本通过，日志 member-b-csp-web.log。独立待审 #89 新增的第25个脚本不在本分支基线，并非删除测试；#87/#89/#90均没有夹带到本 PR。

无 Rust 改动，未重跑 workspace，不把旧测试结果计作此 HEAD 通过；远程 CI 单独按当前 HEAD 回读。截图/日志仅在工作区，不提交生成物。

## 提交后同步最新主线

负责人随后已合入 #87/#89/#90，以及 D 的 #88 前置提案、C 的 #86。当前分支正常合入 main `81a822e3f916dd1324e2cac24145d3f426159807`，无冲突，合并/补验源码 HEAD `c07b000e32455880547f35f72d0539d110261c4f`。
在该 HEAD 重跑全部25个网页脚本（含已合入的超时回归）、配置器 Node 合同/明确取消回归，以及本严格CSP浏览器三尺寸，均退出0。日志 member-b-csp-latest-web.log，截图 test-center-csp-latest。此前24项结果仅对应原基线，不将不同提交的数量混为一个验收。
D 的新增交付明确仍为 IPC 前置提案/拒绝样例，没有可消费的生产 RecoveryClientV1 或 A 安全会话。此同步保留他人作者/提交，不把它算成本 PR 实现或生产后端已接通。

## 主线接手与边界

负责人按精确 HEAD 审查后正常合入；既有日常页已提供 style.css，独立宿主也需加载它。回退本批会恢复旧组件内联样式及其 CSP 不兼容问题；无迁移或设备状态变化。

本修复使组件满足现有样式策略，未完成生产挂载/静态模块路由或其他32步最终验收；具体前置仍见 member-b-usage-and-gates.md。没有自行将合并/自测算入全局进度。

未访问设备/NAS/生产数据，没有真实短信/拨号/流量、签名/部署/发布。只改本人组件、T6允许的最小CSS、夹具和本人文档；不修改 A/D、Core/wire/Auth、全局台账或作者历史。沿用AGPL-3.0-only，无第三方源码复制/新产品依赖；OpenAI Codex辅助实现和检查，不估造工时。
