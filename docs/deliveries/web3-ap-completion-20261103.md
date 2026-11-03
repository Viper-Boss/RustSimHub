# WEB3-AP 产品收尾交付

本任务来自 `codex/web-product-delegation-20261103` 的 WEB3-AP 与父任务书。执行账号 wnfhsj88；无合并、部署或设备动作。最新 main 起点 `8f9cfeff1a4b5a804b8b138e890f7386caf5a006`。本报告只记录本包，不更新全局台账；整体仍 81.5%。

## 阶段一：Core 权威回执

受测产品提交 `49d50051a2e2976bb09c2223e22950ff9ccb4a89`；负向测试提交 `f7d8310`（同分支历史，可核验完整提交）。网页流程及浏览器验证在下一独立阶段继续，不能据此称整包验收完成。

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
