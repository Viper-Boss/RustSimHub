# A-25 独立监督库实际交付

开发账号 wnfhsj88（原 wnfhsj，已接手 fungi1220 的 A 组任务）。
基线 main `80db24e9b101dd3fb4b8c951aa7f1e0e1858e65d`；实际实现及受测代码完整 HEAD
`2f19c28329f2e3fb3b4916d1fd4eaff71e0912d2`。报告补交提交只增加此文件，
PR 最终 HEAD 以 GitHub 回读为准，不将报告提交伪写成已先验测试的实现 SHA。

## 已实现

- 新增无第三方依赖的 `rustsimhub-nkn-agent`，真实有限监督状态机；默认不启动。
- 私有且不可复制的三类引用、单次可提前取消上下文、代次隔离、原证明/进程所有权。
- Root 与引擎启动共享 10 秒且受证明有效期限制；重复 Starting 查询不延长。
- 明确 Offline/已退出才允许有限重试：最多 3 次、至少间隔 5 秒、最多 30 秒，
  不续期或再次启动 Root；永久/未知/部分启动只清理。
- 原子进程退出与 Root 释放共用 5 秒总停止预算；迟到或丢失回执只查原范围。
  超时后的普通 drive 只读核验；显式继续清理不能重发已发 stop 或启动新资源。
- 不采纳另一所有者资源，不凭 PID/stdio 成功推导 SDK Ready，两个资源均核验后才 Disabled。

实现说明、完整提供者契约及生命周期见 `docs/nkn/member-a-supervisor.md`。
Cargo.lock 仅增加本地包 4 行；无依赖下载、版本升级或许可变更，AGPL-3.0-only。
OpenAI Codex 辅助编写实现、失败测试与交付说明。

## 实际验证

环境 Linux x86_64 / Rust 1.98.0；先启用 `/workspace/toolchains/activate-rustsimhub.sh`。

| 命令 | 实际结果 |
| --- | --- |
| `cargo test -p rustsimhub-nkn-agent --locked` | 31 passed / 0 failed / 0 ignored，另 0 doctest |
| `cargo clippy -p rustsimhub-nkn-agent --all-targets --locked -- -D warnings` | 通过 |
| `cargo check -p rustsimhub-nkn-agent --no-default-features --locked` | 通过 |
| `cargo test --workspace --locked` | 51 个测试段，811 passed / 0 failed / 6 ignored |
| `cargo fmt --all -- --check`、`git diff --check` | 通过 |

本次 workspace 基于 main 加本库，不含 PR123/124/128 的实现；没有借用它们的
测试数，也没有运行 whole-workspace all-features。6 项忽略为既有有界子进程 worker、
真实 tmpfs ENOSPC、隔离 Root/UID 能力等特定环境项，不记为通过。
31 项本包测试执行真实 Supervisor，只有外部 Root/引擎/时钟用合成端口；
未运行实际 SDK、Root/cgroup/低权 UID 或设备测试。

完整日志保留在工作区 `setup-logs/member-a-nkn-supervisor-`
`{tests,clippy,production-check,workspace}.log`，不提交私有环境日志。
CI 是否实际执行以当前精确 HEAD 的 GitHub jobs/annotations 为准，账单阻止不得记绿。

## 未验收与下一步

PR129 的固定 Root 策略/权限/序列化、真实 SDK 及身份/固定出口仍待负责人审核。
本库提供不含 wire 的进程内关联类型，没有批准新 Root 协议、分配 UID、创建服务、
开放路由、连接公网或接入生产 Core。缺真实适配器默认 unavailable。
真实提供者必须实施原证明验证、deadline/cancel、精确进程回收及 Root 独立租约过期清理；
同步库不能抢占不遵守契约的阻塞提供者，Drop 也不是无残留进程证明。

A-25 的实际监督策略/失败矩阵已交供审核，整项仍有真实引擎/Root 接线和验收缺口。
全项目公开进度沿用最后实际核验的 81.5%，不由成员离线测试改写。
