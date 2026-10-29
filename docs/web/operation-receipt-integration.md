# B-17～B-20 回执/配置状态候选接口

本包是等待 #76 测试中心契约审核的独立候选实现与字段提案，未 import 到测试中心，未改业务/存储/页面/静态路由。不同来源保持自己的实际状态和授权，组件没有第二套操作状态机。接口不被本报告自行宣布批准。

## 实际来源矩阵

| 实际模块 | 标识/提交与确认 | Unknown/归档和适配边界 |
|---|---|---|
| phone.js / call-test.js | call operation UUID、prepared/submitting/submitted/cancelled/unknown；submit_started 在唯一提交前持久化；call_id/lease/line 绑定另验 | submitted 只证明操作提交；archiveEndedOperation 还查询精确 call、空活动目录和 ended/历史 terminal 证据，组件不代替这些检查 |
| sms-management.js / app.js | SMS UUID；prepare 后 durable marker 再唯一 submit；发送回执与运营商结果分开 | Unknown 只查询原 ID，不再准备或提交；删除本地显示不等于撤回短信 |
| forwarding.js | request_id、expected_generation、实际 generation、receipt state、configuration_applied；process_state_verified 与 enabled/state 分开 | not_found 不是未执行证明；回执/进程清理和 CAS 由既有模块核验。configuration_applied 不代表进程运行 |
| ap-wifi.js | AP operation UUID、request_key_sha256、trials/commits；trial ready 尚未保存；commit saved 需 authoritative_result/settings_saved | Unknown 保留同一 AP 记录；restore_ap/close 另有证明，不能凭普通 Promise 成功清记录 |
| 直接 Wi-Fi 引擎 / T5 RecoveryClientV1 | trial 为 64 hex，恢复写请求为 UUID；不同于 AP 的 UUID 事务 | 当前回执组件仅接 UUID 原操作，不能把 64 hex 截断/重建成 UUID。T5 端口由独立控制器消费，不复用 AP 状态机 |
| session.js | 认证会话续接/过期和撤销，仍由原模块决定 | 登录过期、权限拒绝、设备结果未确认分别投影；不可把认证失败当设备未执行或重试许可 |

## OperationReceipt 展示提案

`update({id,state,submit_started,failure,canQuery,canArchive})`，id 必须规范 UUID；state 仅显示固定状态文本，不决定归档。failure 为 expired/denied/unconfirmed 或缺省。额外 payload/token/lease 不进入显示或内部 view。canQuery/canArchive 只能由原业务模块按精确 ID 的真实能力产生；没有回调或未明确授权时禁用。

显式 query/archive 回调收到原 id 和 `{isCurrent,update}` 显示作用域。渲染、更新、登录和刷新不执行回调。Unknown 不自动归档；若旧通话的 Unknown 经原 archiveEndedOperation 证明已结束，宿主可以显式开放人工归档，但回调仍须再次核验，组件不自行清记录或制造成功回执。回调 resolve 后仍保留显示，等待业务模块明确更新。reset 隔离旧回复，不能替代业务认证/租约/IndexedDB fence。

## ConfigurationState 展示提案

仅 read({signal}) 回调，手动 refresh 一次，无网络轮询；3 秒截止。回调返回 `{source,data,age_ms}`，source 暂支持 forwarding、ap-trial、ap-commit，各自使用上表真实证据，不强行统一 DTO。age_ms 必须由宿主提供可信的原始证据年龄；缺失时未知，不把 HTTP 刚返回冒充服务刚观测。

5 秒为保守的候选 UI 新鲜度（待负责人确认），叠加本机单调经过时间；超窗或时钟反向时显示陈旧/未知，定时器只令旧显示过期，不读取网络。forwarding 保存意图与 verified running/disabled 分列；AP saved/restored_ap 只在 authoritative/settings 证明完整时显示，trial ready 明确尚未保存。只保留所需字段，不复制秘密；旧异步读取不能覆盖 reset/新读取。

## 接线与验收

负责人先审 #76 和本字段提案；确认来源/新鲜度/权限投影后再把组件接入现有 handler 和原操作记录，并添加静态路由。app.js、HTML、session、IndexedDB、call-test/SMS/forwarding/AP/wire 未改。主线保留归档证据、真实服务和设备验收。生产接线不由此候选包冒充完成。

命令：

```sh
node tools/test-web-operation-receipt.mjs
node tools/test-web-configuration-state.mjs
python3 tools/fixtures/operation-receipts/browser.py --output /workspace/setup-logs/operation-receipts-browser
```

实际 Node 模块验证纯展示/精确 ID/提交标记/明确能力/旧回复与单调陈旧，既有生产业务集成测试保持并复跑。真实 Chromium 本地夹具检查 1440/390/320、长中文帮助无溢出、禁用归档、Tab/Enter、保存与进程未确认分离。夹具没有真实登录/设备操作，所有非本地资源拒绝。
