# 成员 B：超时与同步 Abort 回复竞争

账号 wnfhsj88（原 wnfhsj，member-b）；Viper-Boss/RustSimHub 分支 codex/wnfhsj88-ui-deadline，PR 到 main。
起始 main `5238670246e7c6e43184534a7a6ee4b13c293ffd`；实现/受测源码 HEAD `beb8e35d4900251421861f521dd8e95e895f3cbf`；文档后的完整 PR HEAD 见 PR 正文。
对应 B-21～24、B-17～20、B-29/30 的本人候选组件失败路径；不改变接口或另建提交后端。

## 实际缺陷及修复

四个计时器先 controller.abort()、后 reject(deadline)。外部端口若在 AbortSignal 的同步监听器里 resolve 普通结果，Promise.race 会先接受该结果：统计显示成功、配置显示运行证明、诊断替换原未知回执，恢复配置器甚至接受已超时的 ready 回执。

新增回归直接加载四个实际模块，用可控计时器触发原有 3 秒/10 秒期限，外部端口仅在收到 Abort 时返回正常结果。修复前四个场景均失败，日志保留在工作区 member-b-deadline-red.log；不是修改断言后通过。

四处最小修正为先 reject(deadline)、再发送 Abort。于是超时先使调用失败，同时端口仍收到取消通知。统计/配置转为未确认，诊断保留原未知回执；配置器保留写前提交标记和原 ID、不给超时回复授予 ready 证据。没有延长期限或重提请求，不把 Abort 当作设备已取消/回滚。

回归还断言每个端口仅调用一次、确实收到一次 Abort、计时器已清理、按钮/忙状态恢复；配置器仍保留一笔 submit_started 原请求。此测试使用 Node 的真实 Promise/AbortController 与合成外部端口，没有替代产品控制器。

## 验证

以下在实现字节上真实执行且退出0：

```sh
source /workspace/toolchains/activate-rustsimhub.sh
node tools/test-web-deadlines.mjs
node tools/test-ble-configurator.mjs
# 按路径逐个执行全部 tools/test-web-*.mjs：共25个脚本，0失败
python3 tools/fixtures/usage-display/browser.py --output /workspace/setup-logs/deadline-usage-browser
python3 configurator/tests/browser.py --output /workspace/setup-logs/deadline-configurator-browser
git diff --check
```

新增四项竞争回归通过；全部25个网页脚本通过，原配置器合同/手势/防重/SSID边界通过。真实 Chromium 1440/390/320 的统计/实际日常页布局和键盘检查，以及配置器真实 IndexedDB CAS/丢回复查询、离线重开、waiting SW 未决/多窗口拒绝及不强制刷新通过。Abort 竞争本身由确定性的 Node 测试复现，浏览器项是原行为回归，不冒充真实设备期限测试。

没有 Rust 改动，本批未重跑全 workspace，也未用以前的测试数宣称此 HEAD 全仓通过。新增脚本没有自动加入负责人维护的 Rust CI workflow；审查可直接运行上面的命令。远程 CI 状态单独回读，不用本地通过代替。

## 集成和保留项

本 PR 独立于尚待审的 #87；两者分别修复明确取消和截止时间。主线正常合入即可，不需数据迁移；回退本 PR 恢复旧超时顺序，不删除请求记录或身份绑定。静态 Rust 路由、A 授权统计读取、D 生产 backend/握手/设备期限及发布路径前置仍见 member-b-usage-and-gates.md。没有将候选组件、合并或本地测试计为全32步最终验收。

未访问设备、NAS、真实通信/身份数据或生产库；未发送短信/拨号/启用流量，未部署、签名或发布。只改本人四个 JS 模块、独立回归及本人报告，不改 A/D 实现、Core/wire/Auth、名册/贡献/全局进度。沿用 AGPL-3.0-only，无第三方源码复制或产品依赖；OpenAI Codex 辅助实现与实测，不估造工时或改写他人作者。
