# WEB3-AP 产品收尾交付

本任务来自 `codex/web-product-delegation-20261103` 的 WEB3-AP 与父任务书。执行账号 wnfhsj88；无合并、部署或设备动作。最新 main 起点 `8f9cfeff1a4b5a804b8b138e890f7386caf5a006`。本报告只记录本包，不更新全局台账；整体仍 81.5%。

## 阶段一：Core 权威回执

受测产品提交 `49d50051a2e2976bb09c2223e22950ff9ccb4a89`；负向测试提交 `f7d8310e3b56dd3a51fa2558b68d192ae2d2269f`。阶段一 PR [#202](https://github.com/Viper-Boss/RustSimHub/pull/202)，固定 HEAD `ff848793f79981b5bdd796002634f12db65e7d55`，已请求 Viper-Boss 审核。阶段二见下文；两阶段均不能代替物理热点验收。

实际链路：首配 `setup.html` → `setup.js` 的扫描绑定 → `ApConfigurator.scanBegin` → 原有 `setup_ap_scan_begin/status/cancel` → Core `setup_ap_control/rescan.rs` → 受保护有限 Root 槽 → setup-ap 原扫描实现 → NM；Root 结果经严格父绑定、Owner 代次、boot/时间校验回投。Owner 近期认证、全局网络协调、持久化前置、丢回执只查原 ID、30 秒冷却与每父 8 次限制均复用。未修改 Auth/wire 或 Root 协议、默认开关和租约。

修复两个实际缺口：同一父窗口开始新轮后，旧轮权威回执必须标为 historical，不能证明当前 AP 已返回或替换当前列表；子扫描状态投影当前父窗口的已核验 Root 能力、冷却、剩余次数与父剩余时间。能力不存在/陈旧时不编造预算。

验证命令（Rust 1.98.0，locked/offline，独立 target，jobs=2，incremental/debug=0）：

- `node tools/test-web-ap-wifi.mjs`、`node tools/test-web-setup.mjs`：main 基线均通过。
- `cargo test -p rustsimhub-core --lib setup_ap_control --locked --offline`：最初选择错误，0 项执行、15 filtered，不能计为 AP 通过。
- `cargo test -p rustsimhub-core --bin rustsimhub-core --all-features setup_ap_control --locked --offline`：main 基线 71 通过；修复后 73 通过、0 失败、144 filtered。
- `cargo test -p rustsimhub-core --bin rustsimhub-core --all-features setup_ap_control::rescan --locked --offline`：修复前新增断言实际失败，15 通过、2 失败、200 filtered（旧轮 historical 为 false；预算字段为 null）。原始失败保留，未改写为全绿。
- `cargo clippy -p rustsimhub-core --bin rustsimhub-core --all-features --locked --offline -- -D warnings`：通过。
- `rustfmt --edition 2021 --check crates/core/src/setup_ap_control/rescan.rs crates/core/src/setup_ap_control/rescan/tests.rs`、`git diff --check`：通过。仅格式化本包修改文件。

测试使用实际 Core Runtime/Auth/私有日志与受保护槽，Root/NM 回执为明确合成输入；不是实机热点证明。原始日志保留在本包私有忽略目录。Root 真实特权/PID1、RF/手机、签名部署均未运行。

固定旧 PR195–200 已逐项回读，HEAD 与父任务书一致、open/unmerged，不重复实现或合入无关分支。草稿69仍 draft/open，HEAD `803501e2dca59dc388446426010ddaed3201dcd1`，未合并，其旧正文不代替当前代码事实。


## 阶段二：首配页面与恢复

收尾验证日期为 2026-11-04（Asia/Shanghai）；任务 ID 与文件名保持 20261103。独立分支 `codex/web3-ap-ui-20261103` 从重新 fetch 的最新 main 建立；通过正常 merge `483eb878068548723099bed636dcf4e1dc957272` 消费阶段一固定 HEAD，没有重写或复制依赖实现。网页 PR [#204](https://github.com/Viper-Boss/RustSimHub/pull/204) 依赖 #202，均为非草稿并请求 Viper-Boss 审核，目标同为 main，建议审核顺序为 Core → 网页。最新 main 回读仍为上述起点。

最终受测产品/测试 SHA：`283f48da4339d1b6a3b89813d313e2397ff20cd9`。后续报告提交不改变产品或测试源码。代码提交均使用实际执行账号 wnfhsj88。

| 任务动作 | 实际交付与复用 |
|---|---|
| 1 调用链/基线 | 上述真实链路和 main 的 71 项 AP 基线；复用既有 Core/Root/NM 事务，无新网络 actor |
| 2 单一刷新 | 首配仅一个日常“刷新附近 Wi-Fi”；移除隐藏缓存按钮。登录自动读取明确标作旧快照，不主动扫描；未启用或缺少新鲜能力明确说明，可手输 SSID |
| 3 原命令接线 | 复用原 Begin/Status，补页面对原 Cancel 的调用；先持久化 device/parent/scan ID 和取消标记，丢回复或重开只查原 ID，不重放 Begin/Cancel |
| 4 权威事实 | Core 当前轮隔离及能力投影；页面展示上次核验的剩余次数、冷却和父窗口，缺字段不编造。Ready 空结果替换旧列表；历史回执、未核验恢复不发布新列表 |
| 5 试连→保存 | 保留现有明确阶段、同 ID 查询和显式确认保存；重开要求重输 Wi-Fi 凭据，密码/seed 不进入存储。本地进度与 Root 已保存事实分开 |
| 6 互斥/退出 | 原 single-flight 加取消扫描；待扫描时禁试连/保存，隐藏重复父查询。冷却定时器只恢复按钮，不排队执行扫描；旁边显示原因。epoch/device/存储 CAS 在发布列表前核对，pagehide 清凭据并禁控件，BFCache 重开重新加载 |
| 7 失败矩阵 | 实际 ApConfigurator 67 组夹具与生产 Core/Root crate 测试，覆盖错误密码、能力不支持/陈旧、空结果、丢回执、离线查询、取消/撤权、两页更新、服务退出及恢复失败 |
| 8 真实浏览器 | 实际 HTML/setup.js/ap-wifi.js/password.js，loopback 外部合成设备回执；Chromium 11 场景，含真实两标签页、关闭/重开和 320/390/1280 布局。检查真实页面截图，未上传原始材料 |
| 9 验证 | 下列命令通过；旧六个 VM 脚本属于窗口5，未改动，未用本包局部通过背书其旧失败 |
| 10 产品 PR | Core #202 与网页独立 PR，均供 Viper-Boss 审核；未批准、合并、部署或修改看板/名册 |

本地扫描 metadata 升到 format 2，仅新增 `cancel_sent`；format 1 可读并规范化，未知字段拒绝。它不是公共 wire 协议，包含设备/父/扫描绑定而不含 SSID、密码或 token。旧版本页面遇到新格式会拒绝继续，不能绕过保护。

扫描终态发布顺序为：核对原存储与 epoch → 验证权威回执/快照 → 再次核对并 CAS 清理原记录 → 更新列表。终态存储写失败、其他标签页清理原记录、设备变化或关闭均不得先回填列表。扫描 Ready 之后若父进度查询掉线，继续显示已核验的扫描结果，单独提示父进度暂不可读，不把成功扫描误报为未确认。

### 最终针对性验证

环境复用已有 Rust 1.98.0、Node 24.19.0、Chromium 和 Python Playwright；无新依赖、锁文件或 workflow 修改。Rust 使用 `CARGO_INCREMENTAL=0 CARGO_PROFILE_DEV_DEBUG=0 CARGO_PROFILE_TEST_DEBUG=0 CARGO_BUILD_JOBS=2` 与独占 target。浏览器解释器由 `WEB3_AP_BROWSER_PYTHON` 选择已安装 Playwright 的 Python；`WEB3_AP_CHROMIUM` 可选择已安装 Chromium，默认系统 Chromium。未公开本机路径。

| 命令 | 结果 |
|---|---|
| `cargo test -p rustsimhub-core --bin rustsimhub-core --all-features setup_ap_control --locked --offline` | 最终产品 SHA：73 pass / 0 fail / 144 filtered |
| `cargo test -p rustsimhub-core --bin rustsimhub-core setup_ap_control --locked --offline` | 同一 Core 源码（网页提交 f140702 时）：73 pass / 0 fail / 124 filtered；与全特性重叠，不累加 |
| `cargo test -p rustsimhub-edge -p rustsimhub-setup-ap --all-features --locked --offline` | 最终产品 SHA：Edge 41 pass；setup-ap 180 pass；bin/doc 0 项，不另计 |
| `cargo clippy -p rustsimhub-core -p rustsimhub-edge -p rustsimhub-setup-ap --all-features --all-targets --locked --offline -- -D warnings` | 通过，含测试 target |
| `rustfmt --edition 2021 --check crates/core/src/setup_ap_control/rescan.rs crates/core/src/setup_ap_control/rescan/tests.rs` | 通过，只核对修改的 Rust 文件 |
| `node tools/test-web-ap-wifi.mjs` | 67 组实际控制器夹具通过；不是 67 项新增功能或验收点 |
| `node tools/test-web-ap-scan-fence.mjs` | 实际控制器跨页发布栅栏通过 |
| `node tools/test-web-setup.mjs` | 实际首配账号/签名、缓存、试连保存及存储失败回归通过 |
| `node tools/test-web-setup-browser.mjs` | 最终产品 SHA：Chromium 11 场景通过，真实模块与 HTML，不复制网页状态机 |
| `node --check crates/edge/src/web/ap-wifi.js`、`node --check crates/edge/src/web/setup.js`、`node --check tools/test-web-setup-browser.mjs`、`git diff --check` | 通过 |

浏览器验证的外部后端仅生成合成挑战并用 Node WebCrypto 验证浏览器真实 Ed25519 签名，按场景提供 Core 形状回执。Core 测试走真实 Runtime/Auth/持久化与受保护槽，setup-ap 测试走生产扫描/试连/保存执行器及 NM/Root 的显式替身。没有把它们写成“浏览器已联调真实 Core→特权 Root→无线设备”。

### 修复前失败及夹具调试记录

- Core 两项新增负向断言在 `f7d8310e3b56dd3a51fa2558b68d192ae2d2269f` 实际失败：旧轮 historical=false，预算=null，原始日志保留。
- 网页旧源码 `483eb878068548723099bed636dcf4e1dc957272` 的真实跨页缺陷：断言 `snapshots.length === 0` 实际为 1。可复现命令：`git show 483eb878068548723099bed636dcf4e1dc957272:crates/edge/src/web/ap-wifi.js | node tools/test-web-ap-scan-fence.mjs /dev/stdin`，exit 1；最终源码同测试 exit 0。它不运行复制算法。
- 初写该夹具时，在生成回执前清空存储触发了夹具自身前置断言，不能算复现；改为生成回执后再模拟跨页清理，才得到上述实际失败。
- 扩展 Node 测试时，取消后 reopen 已自动只读恢复，夹具多点一次刷新意外开始了新轮；移除多余点击。Unknown 有界轮询结束文案也补为明确的“扫描结果及热点恢复尚未确认”。这些调试失败不计作产品通过。
- 首次浏览器运行失败：favicon 未分流、直接断开无响应 socket 触发 Chromium 传输层同 ID POST 重试，夹具的“ID 必须从未见过”也不符合真实 Core 幂等规则。保留该失败日志；改为返回完整 HTTP 但不可解析的 Begin 回执模拟可用回执丢失，状态查询用 503 模拟掉线，同时保留真实 same-ID 幂等。此后两轮扩展到最终 11 场景通过；没有拿传输层自动重试冒充应用只发了一次 Begin。
- 原始负向、基线、最终输出和合成浏览器截图均留在本包私有忽略目录，未提交原始日志/凭据或公开本机路径。

### 云端、未运行和负责人验收

Core PR202 的 GitHub Actions run `41672128147` 为 failure，test/native-audio 均无执行步骤。实际回读 annotation：因最近账号付款失败或 spending limit 需提高，job 未启动。不能称云端 CI 通过；未改计费、额度或 workflow。网页 PR204 在报告提交 `76c33d6896de089877ad9b518c2529d0c61839c3` 触发 run `41674818697`，首次回读 queued、无完成结论；最终 head/检查在 PR 正文回读补充，不用 Core PR 的结论代替。

未运行：真实 Root 特权/PID1/system bus/NSS/firewall，物理 AP/RF/NM、手机离线重连、真实设备服务退出/恢复、签名安装与部署。当前普通 UID 环境不是 Root 实机验收。未触达 NAS/棒子、蜂窝数据、电话短信、真实通知、NKN/TURN 或 PMIC/关机。

负责人审核、部署后手机最终验收：

1. 在受支持设备的既有首配热点连接后打开首配入口（当前有限契约为 `http://192.168.100.1/`）。核对只有一个刷新入口，首次缓存读取不会断网或声称扫描完成；关闭能力时可手输。
2. 明确确认一次刷新，观察真实 AP 暂停并返回，重连原热点后查询原 scan ID。验证真实空列表能清空选择，冷却显示、每父最多8次及父到期仍由原 Root 拒绝；不通过刷新页面扩大预算。
3. 扫描中断线/关页、两手机或两标签页恢复时核对原 ID、不会再 Begin；取消后只读核对，恢复未确认保留记录并禁止试连保存；旧轮回执不得覆盖新列表。
4. 正确/错误 Wi-Fi 密码分别试连；成功仍显示尚未保存。重开页面重新输入凭据，明确确认一次保存，只有真实 saved 才显示配置完成；实际服务退出和恢复失败保留 Unknown。
5. 保存成功后手机连回目标 Wi-Fi，通过设备实际配置的 HTTPS/.local 管理入口登录；这一步不由 loopback 浏览器替代，不自动启用蜂窝回退。

代码开发和隔离验证已交付，审核/合并及以上物理验收仍由 Viper-Boss 负责。固定整体完成率仍 81.5%，20/169、加权10.6%；不把本包10个动作、夹具数或 PR 数加入既有完成率分母。
