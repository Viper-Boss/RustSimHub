# A-21/A-23/A-24：NKN只读状态组件交付

- 提交账号：wnfhsj88（成员B，按用户指示接手成员A任务）；起始主线：80db24e9b101dd3fb4b8c951aa7f1e0e1858e65d。
- 实现及集成说明完整HEAD：2c87d97e2fd133d059e269d559a505d01072b97f。此后的交付记录提交不改变测试代码；最终PR HEAD见GitHub。
- 范围：新增`crates/edge/src/web/nkn-status.js`、Node测试、三个尺寸的本地浏览器夹具、`docs/nkn/status-integration.md`和本记录。不改Core、Root、Rust路由、首页装配或全局验收数据。

## 已实现

真实组件严格消费冻结T5的14字段状态；拒绝额外字段和非法枚举，保留精确u64字符串，独立显示开关/进程/DHT/路径，显著标明模拟和未知。只有显式手动读取；默认无provider，不能操作引擎或网络。连接节点数明确不代表用户数/安装数。

3秒绝对读取截止、双时钟年龄核验、5秒陈旧降级、单调序号、来源/代次变化锁定至显式reset、隐藏/销毁/迟到回复隔离均已实现。具体边界与固定上游字段盘点见集成说明。Abort只取消本地等待，不冒充后端引擎已停止。

## 对应实现HEAD的真实验证

工具环境：Node24.19、Python3.12/Playwright1.64及系统Chromium。激活`/workspace/toolchains/activate-rustsimhub.sh`后执行：

- `node tools/test-web-nkn-status.mjs`通过：实际解析/组件、冻结字段枚举、精确u64、输入拒绝、时钟/年龄/序号/代次/reset/隐藏/dispose以及真实3秒abort竞态。
- 逐一执行`tools/test-web-*.mjs`全部26个脚本通过。最终文字标注调整后，NKN专用脚本再通过；其余25个脚本的实现和测试未改变。
- `python3 tools/fixtures/nkn-status/browser.py --output /workspace/setup-logs/nkn-status-browser`通过：实际模块在1440/390/320宽度、严格CSP、44px点击/键盘、精确大整数/长中文无溢出、原生5秒陈旧且无自动读取、上下文锁定/reset。最终文字标注调整后已重跑三尺寸。
- `git diff --check`通过。未改变Rust代码，不借用其他PR的Rust测试结果。

本地日志：`member-a-nkn-status-{node,web,browser}.log`，位于工作区`setup-logs`；截图仅为合成夹具。未上传原始设备数据。

## 未实现/未验收

A-22真实引擎输出适配、授权Core provider、实际首页静态路由/装配、真实NKN/设备/ARM/Root/Android/第二端及无PID退出均未完成。本包默认不可读取，浏览器provider明确为合成数据，不能将测试当成真实统计、接入成功或安装数。待负责人审核本组件及实际provider/路由后再接线；不把A-21～24整组标成已验收。

沿用AGPL-3.0-only；无新依赖或第三方代码复制。上游只读来源/许可在集成说明与既有NKN提案中列出。OpenAI Codex辅助实现、审查和验证；不修改他人历史作者。
