# 成员 B 第二批：网络显示与交接（B-05～B-08）

执行账号：`wnfhsj88`；旧账号 `wnfhsj`，稳定成员 ID `member-b`。用户要求继续工作，本批单独交付审核；第一批 PR #74 仍待审核，未计验收。本分支从 main 建立，不包含或依赖 #74 的计量修改。负责人仍需按任务书串行核对、审核后决定合并；本报告不更新全局进度或名册。

- 起始 main：`7257cfa8d82bac8972158ef79ea081d9be12af75`
- 实现和测试完整 HEAD：`9724a620d62961ad3ce7ca6e0f3edc3ac05d49f9`
- 分支：`codex/wnfhsj88-network-display`，仓库 `Viper-Boss/RustSimHub`。
- PR 最终完整 HEAD 见 PR 正文（随后只有本交付报告提交）。
- 对应任务：NET-03 的显示离线子范围，B-05/06 回归、B-07 验证、B-08 交接。未宣称 NET-03 全范围或所有 B 任务验收。

## 触发输入与改动

只修改实际 `crates/edge/src/web/network.js`、既有 `tools/test-web-network.mjs` 和本报告。

| 触发输入 | 修改前 | 修改后 |
|---|---|---|
| cellular=null，频率/信号是字符串或对象 | 读取 null 或调用非数字 toFixed，整个 render 失败 | 单项暂不可用；有效零值、负 dBm/dB 保持 |
| 非数组 DNS、devices 中有 null | 扩展对象报错、字符串拆成字符，或访问空设备报错 | 只消费数组中的字符串/对象设备，有效 IPv4/IPv6/DNS/APN 保持 |
| usage/policy 缺失或 uncertain 不明 | 计量“正常”、限制“已关闭”，可能显示剩余全部额度 | 未知状态显示暂不可用；uncertain=true 时不展示可用额度 |
| 缺失/非法速率 | 曲线将缺测转为 0；Infinity 污染坐标 | 曲线以 null 留缺口，真实 0 仍显示零速率；坐标保持有限 |
| rate(Number.MAX_VALUE) | 中间乘 8 溢出，显示 Infinity Mbps | 调整除乘顺序，有限输入不因中间计算溢出 |
| 一次有效刷新后下一次请求报错/过期被拒 | 文本清空但旧曲线和接口仍残留 | 同时清除曲线、旧接口和 sample；保存按钮保持禁用 |

频率仍注明“按信道计算”；允许频段不冒充驻留频段；带宽缺失不编造。字节为十进制 B/KB/MB/GB，速率输入 B/s，显示 Mbps（一次乘 8），不改后端计量单位。所有设备文本通过 textContent 渲染，合成 HTML/APN 不执行。没有新增请求、轮询、自动网络探测或数据启用行为，没有修改权限、会话、提交或幂等逻辑。

## 真实验证

Debian 13 x86_64，Node 24.19.0、Rust 1.98.0。

- 新矩阵先在原 network.js 上执行 `node tools/test-web-network.mjs`，退出 1：cellular=null 导致 `Cannot read properties of null (reading 'serving_cell')`。修复后同一矩阵退出 0。
- `for test in tools/test-web-*.mjs; do node "$test" || exit; done`：19 个脚本全部通过，退出 0。既有退出迟到响应隔离、pending 不重复提交、隐藏页面不请求和不自动主动测试断言保留。
- 新增矩阵直接导入真实 network.js，以合成 DTO、最小 DOM/Canvas 端口检查 null/类型/非法数值、有效零值、未知枚举、文本注入、DNS/IPv4/IPv6、单位换算、缺测曲线、有限坐标及过期清理；不复制实现作为判定。
- `cargo test -p rustsimhub-network-agent -p rustsimhub-edge --locked`：退出 0，Edge 37、network-agent 12，通过共 49 项；doc-test 0 项另计。这里基于 main 原 network-agent，未包含 #74 新增 16 项。
- `cargo clippy -p rustsimhub-network-agent -p rustsimhub-edge --all-targets --locked -- -D warnings`：退出 0。
- `cargo fmt --all -- --check`、`git diff --check`：退出 0。
- Linux 持久路径未修改，没有执行第一批提到的零匹配 Python 测试，也不计其通过。

## 保留边界和主线补验

- 快照时效由现有 Core 的 schema/boot/设备时钟与 30 秒窗口验证；收到拒绝后页面清旧数据。不另用浏览器时钟判断设备时钟。速率由设备提供，网页不对计数器另算差分；同时间戳、倒退/长暂停/计数器重建的生产者计算正确性仍应由主线在设备核对，不能由本显示矩阵宣称全部通过。
- 没有访问 NAS/棒子、真实 NetworkManager/nft/MM 或生产通信数据，没有发送短信、拨号、主动测速或启用普通 APN。
- 未运行真实浏览器、Android、多端或实机切网；Node DOM/Canvas 端口不等同真实浏览器验收。未运行 ARM64 和整仓 Rust 测试；只验证受影响包。
- 主线补验：真实缺信号/断线/小区未知、APN/IPv6/DNS、按信道推导/带宽未知、运营商文本安全；限额开关与不确定状态、收发速率及运营商计费误差；页面隐藏/退出/恢复和换网后陈旧曲线/接口是否清除。不得自动回退蜂窝或在未授权时主动产生测试流量。
- PR #74 的 CI 已确认因 Billing & plans 未启动，本 PR 的远程 CI 状态以 GitHub 回读为准，不拿本地通过替代 CI。

## 来源、集成、回退

本次 OpenAI Codex 辅助实现和测试，复用仓库模块与依赖，无新增依赖或第三方代码复制；沿用 AGPL-3.0-only SPDX。未估造有效工时，未改写旧提交作者。

负责人可独立审核本 HEAD；建议按第一批再第二批的顺序合并。静态 network.js 由现有 Edge include_str 路由提供，主线接受后按已有应用包流程重建/交付，设备部署和验收由主线负责。无需 schema/配置迁移，回退实现提交即可恢复原显示。任务状态和整体百分比由负责人审核后维护。
