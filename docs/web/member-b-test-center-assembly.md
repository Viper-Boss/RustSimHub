# B-29/B-30：首页装配与只读状态

PR #100 已实际注册五条静态模块路由并装配首页。路由落实已合入 #93 的精确 GET/include_str diff；Host、中间件、CSP、缓存和设备 API 语义保持。最新实际源码/受测 HEAD：`923c3827d3c49707a376c2fabc1ba0d3c3b05778`，已正常合入 main `aac1ec246bba9d77c98d42350866d6eb06a414d6`。审核、合并、部署与设备验收由负责人执行。

原装配候选保留在 [c1ed002 历史](https://github.com/Viper-Boss/RustSimHub/blob/c1ed002d101483459ca9d3ca6cfb0f3eff4b9111/docs/web/member-b-test-center-assembly.patch)，当前以实际源码为准，不再附带可重复 apply 的补丁。

| 面板 | 实际接线 | 保留边界 |
|---|---|---|
| TestCenter | 委托原七个 onclick；保留 act/认证/AP/电话/租约/代次/原 ID 防重 | 隐藏保留原处理节点，只有一套可见入口；不另造提交器或登录流程 |
| OperationReceipt | 本机原 call/SMS 的有限投影；query 复用 act/query，核对对象、ID、viewEpoch 和实际完成 | 不清记录，不开放新归档；原业务归档保留；未知 state 不生成 saved 证明 |
| DiagnosticPanel | 复制有限 origin/pin 存在性/原操作投影 | 不输出完整地址/pin/凭据/正文/lease，不推断认证或证书；原 ID 查询在回执区 |
| ConfigurationState | 登录后手动调用现有 forwarding_status，接受有本次请求证明的运行/关闭状态 | 只覆盖转发进程；不确认保存，不启停服务/发送通知；缺证明、超时、陈旧保持未知 |
| UsageStatsPanel | 已挂载，read=null，窗口/未知语义保留，读取禁止 | 等 A 的授权读取 provider；不编计数、启引擎/订阅或把节点数当人数 |

构造、渲染、重新显示面板不请求业务；首页原初始化/登录读取与原轮询保留。结果复制、草稿恢复、原操作提示及手动电话/短信/设备管理保留。页面明确列出原 10010/约30秒、10010/CXYE 确认发送和 Bark 合成通知范围。

AP/电话 MutationObserver 只同步显示，不观察自身渲染区，不触发请求。离页断开并重置，返回只投影当前记录；退出仍走原清理。显示重置不证明设备已挂断或回滚。

## 转发状态的实际来源与年龄

没有新增 Core 字段或把 HTTP 到达当作新鲜证据。沿用上述 main 的现有路径：

1. authenticated.rs 将 ForwardingStatus 交给 forwarding::Controller::request；启用 service management 时选固定 Status 动作，未启用的被动路径不产生验证证明。
2. forwarding/service.rs 在 Owner/持久状态核对后记录 CLOCK_BOOTTIME 的 start，执行既有固定 status 单元并读取私有报告。完成后重新核对授权与前后 Snapshot，只有相同 boot/Core/generation/binding、成功任务、有效 PID/无 PID 和 process_state_verified 的报告可通过。
3. verified 明确要求 sample_uptime_ms >= start 且 <= now，并有10秒设备端年龄上限。两个时钟都向下取整到毫秒，因此浏览器另加1毫秒余量；不假定远端时钟与浏览器同步。
4. App 还严格检查 available/configuration_api/service_management_api、completed、verified、非负安全整数 generation，以及 enabled/state 的运行或关闭组合。任一缺失均拒绝，不从配置意图推断。
5. 读取前后同时测 performance.now 与 Date.now，均须有限、非负且小于3秒；provider 年龄取两者最大值向上取整加1毫秒。组件再保守加入整个本地等待耗时，覆盖 provider 返回到显示之间排队的 JS 时间，允许重复计算网络时间。累计超过5秒显示陈旧；不会自动重查。

状态查询固定投影 configuration_applied=false，即使返回携带该字段也不据此确认本次保存。host 注入 Date.now 作显示年龄时钟以计入休眠，负/非有限 elapsed 均未知；隐藏页面清除该快照，返回不自动查询。

读取捕获 token/viewEpoch，检查组件 AbortSignal、可见/活动状态和原业务忙碌门禁；退出/换登录/离页/后台晚响应不更新。组件3秒超时只终止 UI 等待；原 api 自带18秒 HTTP 上限，Core 原固定任务有8秒预算，没有改这些语义。旧传输未结束时本地只允许一个 status 读取，重复点击不能产生重叠请求；不冒称 UI Abort 已取消设备动作。这里不会调用 reconcile、保存、启停、quick-test、短信或电话提交。

## 当前源码验证

```sh
node tools/test-web-test-center.mjs
node tools/test-web-configuration-state.mjs
# 全部 tools/test-web-*.mjs 逐个运行
node tools/test-ble-configurator.mjs
python3 tools/fixtures/test-center/assembly-browser.py --output /tmp/homepage-evidence
cargo test -p rustsimhub-core --all-features --locked --offline forwarding::service::tests -- --test-threads=1
cargo test -p rustsimhub-edge --all-features --locked --offline
cargo clippy -p rustsimhub-edge --all-targets --all-features --locked --offline -- -D warnings
cargo fmt --all -- --check
git diff --check
```

25个网页脚本、配置器 Node、Edge38项、Core既有 service 测试7项（167过滤、0忽略）、严格 Edge Clippy 和 fmt 均通过。Core 测试使用实际 dispatch/verified 和私有 UnixStream D-Bus 夹具，不访问主机 PID1。未修改 Core 源码/测试；通过是既有合同复核，不算本人新增后端。

实际 App 与五个模块验证原 SMS 一次提交/草稿、Bark 原共享 IDB 及回执、原 ID 查询/AP/停止/退出/离页边界；配置读取覆盖运行、关闭、未知、无能力/不完整证明、3秒边界、两种时钟异常、重叠/迟到、保存不推断、年龄累计及显示时回拨。

Chromium 1440/390/320 使用实际 HTML/App/模块与原 Edge CSP。先以本地403验证实际 IDB 保留的合成未确认记录、禁读、脱敏复制、键盘/44px、折叠/溢出与零 CSP 违规；随后在同页通过本地合成设备 Ed25519 签名挑战，实际 proof.js 核验、浏览器 key 签名、夹具验签及原登录流程。手动查询核对运行/关闭/未知，等待真实5秒变陈旧，无轮询/重试，退出禁读。只替换外部 API；没有注入 App token 或放宽认证/CSP。原登录/轮询和 configuration_indicator(false) 为既有行为，新增入口只有 forwarding_status。剪贴板是外部写端口夹具，不声称 OS 权限或真实 Owner/设备验收。

日志/截图仅保留工作区，未提交生成物。未运行全 workspace/ARM/实机/实际 HTTPS/真实声音、短信、Bark、Android 或第二端；远程 CI 按完整 PR HEAD 单独回读。

## 接续与回退

统计授权 provider、完整恢复认证/GATT/Core 后端和 Pages 路径仍依赖 A/D/主线；其他配置来源不会从转发证明外推。PR 源码成功、合并和自测不等于全32步或产品验收，不改全局台账。历史起始 main `b7a5dfa8d0d8c762c2284164e9d4371fce3c373f`；原静态路由提案 #93 的精确范围保持。

回退采用对应源码提交正常回退；先撤新 import/装配再移除路由，不删除原请求、草稿、回执或身份。沿用 AGPL-3.0-only，无新依赖/第三方源码复制；OpenAI Codex 辅助实现与验证。
