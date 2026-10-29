# 测试中心组件接线提案（成员 B / B-09～B-12）

本包交付独立 `TestCenter` 展示适配器及隔离验证。主线现有 app.js 已有真实五项测试、回执、停止和恢复逻辑，不能再造一套业务流程。本包未替换首页、未新增设备路由、未改变 CallTest/SMS/转发/会话/权限/幂等规则。当前组件只由本地夹具使用，生产接线等待负责人审核。

## B-09：实际入口与状态

| 现有入口 | 现有行为 | 保留的边界 |
|---|---|---|
| 首页 `#test-center` | 位于关闭的 account-panel 之前；检查、拨出、来电、短信、Bark 五项主要任务可见 | 不能因渲染、登录、刷新或显示重试提示自动提交 |
| `runTestCenter('check')` | 现有 testLogin 后读取 line_status/call_status/network_status | 只读状态不等同互联网或媒体可用 |
| `runTestCenter('outgoing'/'incoming')` | 现有 CallTest 的麦克风、原操作核对、租约和通话流程 | 音频未就绪时按钮禁用；不由组件拨号、接听或续租 |
| `runTestCenter('sms')` | 草稿保存、确认、一次发送、回执与收件箱 | 不新增短信 prepare/submit，Unknown 保留原 ID |
| 原 `test-forwarding` 的绑定回调 | 有界 forwardingQuick.run、原 IndexedDB 保留与恢复 | 保留 AP/lease 门禁，不能替换成裸 forwarding API |
| 原停止回调 | 增加 testGeneration，停止 CallTest，并明确“已请求停止” | Promise 返回不证明呼叫/进程已停止 |
| `runTestCenter('recover')` | 现有 reconcileTestPending 查询/核对与归档 | 不由组件发新幂等键、清原记录或自动重发 |
| 原手动电话/短信/账户详情 | 外层 details 默认收起，短信内部 composer 打开不等同主面板展开 | 宿主页保留原功能、复制结果、草稿恢复与原记录提示 |

已存在的入口可见性和草稿/恢复/多标签行为由 `tools/test-web-test-center.mjs` 覆盖；本包复跑，不把这些已有功能记为新增。

## B-10：冻结提案接口

```js
const center = new TestCenter({document, root, actions, debugElement});
center.update({
  message: '已脱敏的现有控制器提示',
  busy: false,
  unresolved: true,
  actions: {
    check: {enabled: true},
    outgoing: {enabled: false, reason: '原操作结果未确认'},
    recover: {enabled: true}
  }
});
center.reset(); // 只清本组件显示/隔离迟到 UI；宿主仍负责停止原控制器
```

- root 必须是带唯一 ID 的独立 DOM 挂载点；每个 reason 以其 ID 为前缀。debugElement 是宿主提供的已有调试节点，放入默认关闭的 details。
- 回调收到 `{isCurrent, update}` 显示作用域；异步通知应使用这个 update，reset 后会忽略旧通知。宿主主动刷新可调用 center.update，但必须保留自己的认证/业务代次检查。这个显示作用域不替代既有业务 fence。
- actions 固定为 check/outgoing/incoming/sms/forwarding/stop/recover 的 Promise 回调。没有回调或 enabled=true 时默认禁用，原因始终可见且由 aria-describedby 关联。
- update/render 不调用回调；只有用户点击/键盘激活（或宿主显式调用 run）执行对应回调一次。busy 阻止并发开始；停止回调单独防重，允许终止尚未返回的开始回调。
- unresolved 或本组件捕获到未确认错误时，四个提交入口禁用，查询原操作仍由宿主回调控制。发生错误后不自行启用提交，需宿主显式 update 新快照。新快照必须来自现有控制器/存储的真实状态，不可在 finally 盲目写 enabled=true。
- 此接口只有 UI 可用性/提示字段，不接收或持久化原操作 DTO；原操作 ID、会话、秘密、短信内容、lease、回执和 IndexedDB 全留在现有控制器中。回调闭包可引用原操作，不得复制秘密到 DOM。
- 回调 resolve 不自动写“已成功”“已停止”或清原操作；未收到控制器 update 时只提示返回后需核对。错误消息固定脱敏，不输出原始异常。reset 后迟到回调不恢复按钮或状态。
- 原控制器通过 notify/testMessage 提供“准备/拨号/提交/声音连接/挂断未确认”的真实文本。展示组件不从普通 Promise 或 PID 推断这些业务阶段。

## 宿主页最小接线步骤（尚未实施）

1. 负责人确认接口及所需显示状态。保留旧 DOM/业务处理器，选定一次性的挂载点和唯一 ID，避免两套按钮同时可操作。
2. 在 app.js 中注入上述既有 runTestCenter 入口；转发复用现有绑定回调（包括 AP/lease 门禁、act 和 finally），不要直接调用裸发送方法。stop 必须沿用 testGeneration 和现有停止/核对逻辑。
3. 把 showTestControls、testMessage/CallTest.notify 的现有状态投影到 update。AP 未结束、其他业务 busy、音频未就绪、未确认 pending、服务/权限不足的原因来自现有控制器，不能通过 UI enabled 绕开服务端授权。登录可由既有 testLogin 完成，不重复实现认证。
4. 将同一份原 pending 的查询交给 recover 回调，Unknown/失去确认只查询原 ID。保持现有 `test-copy`、草稿恢复、回执展示功能；新组件未提供这些替代实现。
5. 在现有 Rust 静态 allowlist 中显式提供 `/test-center.js` 的 include_str 资源；不开放目录浏览/任意路径，不上传 fixtures/mock。将 `node tools/test-web-test-center-component.mjs` 加入既有 CI 的明确命令清单；本包不修改路由或发布流程。
6. logout/pagehide 先调用现有 stop/audio/session 清理，再 reset；组件 reset 本身不停止设备。保持 viewEpoch/testGeneration/current 检查，不能把显示代次代替业务 fence。
7. 主线接线后必须复跑 app/SMS/CallTest/Forwarding 的实际集成回归，以及真实浏览器键盘/布局，最后再安排设备/双端验收。当前夹具结果不代表完成上述生产接线。

## B-11/B-12：可重复验证

```sh
cd /workspace/RustSimHub
node tools/test-web-test-center-component.mjs
node tools/test-web-test-center.mjs
python3 tools/fixtures/test-center/browser.py --output /workspace/setup-logs/test-center-browser
```

浏览器脚本需系统 Chromium（当前 /usr/bin/chromium）及 Python playwright。它仅在临时端口绑定 127.0.0.1，仅提供本夹具 HTML 和实际组件模块，拦截所有非该源请求；结束后关闭浏览器和服务器。样例只执行本地回调，不接设备/公网业务、不发短信/拨号/转发。截图输出目录由参数指定，不提交生成物。

本地 Chromium 实测 1440/390/320：长中文消息与禁用原因无横向溢出，debug 默认收起，Tab 跳过禁用按钮，Enter 只触发一次 check，继续 Tab 到 recover/summary，Enter 展开调试。所有页面无 JS 异常。

Node 检查组件真实模块，外部 DOM/音频/IPC 使用合成端口；直接调用真实 CallTest 验证麦克风拒绝、权限不足、SIM 未注册、HTTP409 不产生 prepare/submit；用真实 archiveEndedOperation 验证 Unknown 精确原 ID 查询且不清记录。另测并发/丢回复/停止中/停止失败/退出迟到/文本注入。既有 SMS/转发 app 集成脚本保持并复跑，未用新状态机替代。

真实 SIM、声音、停止进程、第二端、生产装配仍未验收。本包为 B-09～B-12 的独立组件/契约与离线验证交付，等待负责人审核后接线；不自动增加产品进度。
