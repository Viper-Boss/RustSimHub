# B-29：统计展示、页面回归与剩余接线

本包直接对照成员 A PR #41 的完整 HEAD `295c47655af1d69d2eb0568eda09023c4599c16b`：`crates/usage-stats/src/query.rs::QueryOutcome/CacheEntry/Freshness` 和 `protocol.rs::Window`。只读检查了这些公开文件，未修改 A 实现、接口或分工。该来源仍未成为可用的 Core/Edge 统计 API，因此候选组件默认没有 read 端口。

usage-stats.js 的 usageRows/UsageStatsPanel 接受实际 QueryOutcome：disabled、success{entry}、cached{entry,freshness,reason}、unknown{reason}。entry 的 window 为 24h/30d/90d，source 为 read_only_transport，原始观察/上次成功/到期时间和 freshness_revoked 分开处理。只显示有限白名单，不渲染独立身份公钥、订阅地址或 peer 详情。

- 已关闭只说明停止新统计意图，已有公开订阅可能保留到到期；不推断转发/远程/数据开关。
- 缓存明确标记原值及过期/回拨/期限状态；缺失、来源不匹配、窗口不符或时间证据无效保持未知。
- count 是近期活跃安装估计，不是人数、在线用户数或 NKN 连接节点数。原 A 的 u64 在 JSON 中是 number，超过 JS 精确整数范围时拒绝显示计数，不能先舍入再当精确值。若主线希望支持全部 u64，需与 A 审核无损传输适配，不能由本包自行改 DTO。
- 时间统一标明 UTC+8，保留实际统计窗口与原更新时间。本页面不启动引擎、订阅、续订或测速。
- 只有点击只读刷新时调用 read({window,signal})，10 秒截止、至少5秒间隔，不自动轮询；reset 隔离旧回复。提供端口仍由主线执行真实授权/统计开关/来源和 TTL 检查。

## 最小生产接线提案（未实施）

负责人接受 A DTO/授权读取、静态路由和本候选后，在日常页面增加独立统计 section，然后：

```js
import { UsageStatsPanel } from './usage-stats.js';
// readApprovedUsageStats 是待主线接纳的授权读取适配，不是本包新增 Core 操作。
const statsPanel = new UsageStatsPanel({document, root:document.getElementById('usage-stats'), read:readApprovedUsageStats});
// 在既有 logout/pagehide 清理处 statsPanel.reset()，不新增身份/认证流程。
```

route allowlist 必须显式增加 `/usage-stats.js`；现有 Edge 静态路由目前没有此模块，也没有 test-center/operation-receipt/configuration-state/diagnostics 的新候选路由。不能只往 app.js 插入这些 import，造成主页加载失败。T6 开放 app/HTML/CSS 装配，但没有开放修改 Rust 路由/统计鉴权；本包把这个必要接线点交负责人，不越界修改。#76/#79 的合并不自动证明组件契约/接口已验收。

其余组件应沿各自 integration 文档复用原业务控制器，并在同一主线接线窗口进行，避免两套提交入口、复制/草稿/Unknown 状态冲突。原电话、短信、回收站、配置、登录、会话不重写，不新增权限或自动提交。生产全部组件装配尚未完成，不能用本包独立组件截图冒充。

## 实际页面修复与测试

Chromium 拒绝本地合成 identity 请求时，主页面仍显示“正在检查设备…”。本包只改 initialize 的失败提示，让测试区明确“初始化未确认”；不改任何认证/业务请求或按钮提交逻辑。

Node 实际模块覆盖窗口/时间/关闭/陈旧缓存/未知/超安全整数、额外秘密字段排除、仅手动读取/间隔和旧回复隔离。真实 Chromium 1440/390/320 检查独立组件键盘/缓存显示，以及当前实际日常页面折叠/展开电话、短信和 AP 的布局无溢出；API 由本地合成拒绝，记录仅允许 identity/challenge 只读准备，不点击业务测试。

```sh
node tools/test-web-usage-stats.mjs
python3 tools/fixtures/usage-display/browser.py --output /workspace/setup-logs/usage-display-browser
```

未验收：真实统计传输/同意开关、A 主线集成、路由与所有组件完整装配、登录态实际页面/Android/第二端、真实运营商或设备。没有将原 P0–P7 或 WEB-03/07/STAT-09 标完成。
