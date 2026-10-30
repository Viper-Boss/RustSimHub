# 成员B：事件循环暂停后的绝对回复时限

账号wnfhsj88（原wnfhsj，member-b）；Viper-Boss/RustSimHub 分支 codex/wnfhsj88-absolute-ui-deadlines，PR到main。
起始main `6b5de5f3ca5ff706adacd8f25ed7f638546739a9`；实现/受测源码HEAD `d348ef1ef0748a9876476da82d72d4e93ede0e62`；最终PR完整HEAD见正文，此后仅本报告。
对应B-21～24、B-17～20、B-29/30本人候选的失败边界；不改变接口DTO、Core/NM的40/45秒试连期限或设备授权。

## 复现和修正

原Promise.race只等待计时器；计时器在调用外部端口之后才注册。同步调用耗时或事件循环暂停后，已经就绪的回复可能先于逾期计时器被接受。注入单调时间前进到预算、倒退或NaN且故意不触发计时器，新12项回归均复现旧实现的问题；旧4项Abort竞态回归保留。

现在先注册计时器，回复后再检查单调elapsed必须有限、非负且严格小于3秒（统计10秒）。到期/无效则Abort并拒绝，仍先reject再Abort避免清理竞态。恢复写保留原UUID/提交标记、只查询；只读显示保持未知/原回执。无自动重试、无新ID或轮询。DiagnosticPanel新增与其他候选一致的可注入clock，生产默认performance.now，不从网络/URL取时钟。

UI无法抢占同步JS，只能在其返回后拒绝迟到结果；真实服务仍须异步、有界并核真实权限/期限。详见 [时限说明](../web/read-deadlines.md)，不以UI拒绝证明设备已取消或回滚。

## 真实验证

- `node tools/test-web-deadlines.mjs`：16项通过；12项新增覆盖四模块的到期/回拨/无效时钟，原4项Abort结果竞态保持。断言一次调用/Abort、预算在调用前注册、定时器清理、原记录保留。
- 全部25个tools/test-web-*.mjs与`node tools/test-ble-configurator.mjs`退出0；原合同、手势、SSID、丢回复和明确取消保持通过。
- 实际Node原生performance.now/计时器下，合成beginWifi同步阻塞3.05秒后返回ready：写入预期Unknown，保留原标记，无操作证明、一次调用/Abort，没有重提。不是假计时器或真实设备操作。
- `python3 configurator/tests/browser.py --output /workspace/setup-logs/configurator-absolute-browser`与usage-display浏览器三尺寸回归退出0：实际IDB CAS/丢回复查询、离线重开、等待SW保护/无强制刷新、统计/日常页面保持通过。
- `git diff --check`通过。红/绿、原生时钟、网页与浏览器日志仅setup-logs，不提交生成物。

没有Rust/发布流程改动，本批不重跑workspace/ARM，不把旧结果写为本HEAD通过。未做真实GATT、Core恢复认证、设备时钟/试连/取消、实际短信/呼叫/流量或第二端验收；远程CI按当前HEAD另回读。

主线正常合入即可，无数据迁移；回退恢复旧计时器顺序/回复检查，不删除原请求。整体32步验收/路由实际接线/生产provider前置仍由对应交付记录和主线审核决定，不改全局进度。

AGPL-3.0-only，无新依赖/第三方源码复制；OpenAI Codex辅助实现与实测，没有工时估造、他人作者或分支改写。无设备/NAS/生产数据访问、真实通信/流量、签名/部署/发布。
