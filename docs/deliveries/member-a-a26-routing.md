# A-26：有限远程路由策略实际交付

账号 wnfhsj88，2026-11-01。起始最新 main
`ce9ccb8debb61db49860fe795c1f3e471b749f4c`；实现及最终受测源码
`ac7039fc67e26ab55040670ae19cdf3b5d65ade9`，最终整仓测试开始时 tracked_dirty=false。
本报告提交仅新增交付说明；最终完整 PR HEAD 在 PR 正文/GitHub 回读记录。
分支 `codex/wnfhsj88-a26-routing-policy-20261101`，同仓库 Viper-Boss/RustSimHub，非 fork。
实际 GitHub 连接和 gh CLI 均只读核实为本人账号；本次提交作者使用本人 GitHub noreply。

## 具体变化

承接已合并 PR123/124/128/129/133/134，不重做旧 PR、不修改历史作者。
优先为原剩余9步中的 A26 提供实际 `nkn-agent::routing::Router`：精确双服务分派、
普通 NKN/BLE purpose 分离、不透明完整绑定比较、逐段撤销/父连接检查、每方向
顺序/128请求配额、同一5秒且受更短授权约束的绝对截止，以及业务可能执行后的
`OutcomeUnknown`。只返回已封装结果，不把路由回复当业务成功或自动重试。

默认生产端口是 `UnavailableTransport`/`UnavailableBusiness`，测试合成端口仅
`cfg(test)`。未实现成员密码验证器、安全会话、wire 编码或普通 Core 桥；
`Binding` 的可信来源、完整性与真实 Core 执行点鉴权仍需提供者履约与审查。
本库不解析原 AuthRequest、不重建 Principal/Scope/队列，也不接 BLE recovery 能力
到正常 Core socket。具体接口、有限预算、并发/同步提供者责任、发送前最后检查和
不宣称内存擦除的限制见 `docs/nkn/member-a-routing-policy.md`。

文件：本包 `crates/nkn-agent/src/lib.rs` 最小导出、新 `routing.rs` 与
`routing/tests.rs`、上述接口说明、本报告及原剩余步骤索引。没有 Cargo/lock 改动，
无第三方新增/升级、服务安装、固件权限、CSP、生产配置/迁移或其他成员代码变化。
原进程监督31项测试保持，新增34项路由测试直接执行实际生产 Router，只有外部
密码/成员/Core端口与时钟为合成，不用这些结果宣称密码/真实Core互操作已通过。

## 真正失败及修正

1. 首轮包测试实际 63通过/1失败：业务后时钟回拨正确产生 Unknown 并关闭，但
   下一次调用在检查 Closed 前再次读时钟，返回 ClockRollback。改为先识别已关闭
   会话，同一断言最终返回 Closed，不重试业务。旧日志保留，不称主线生产漏洞。
2. 代码复核补加一项测试，先运行实际失败：仅剩1秒的初始授权，首次只读 inspect
   却拿到5秒默认预算。修正为在首次 inspect 前也取原授权较短期限；后续所有阶段
   只可收紧。保留相同断言，未拉长期限或放宽拒绝。

## 最终实际验证

Linux x86_64，启用 `/workspace/toolchains/activate-rustsimhub.sh`，Rust 1.98.0。

| 命令 | 退出/实际结果 |
| --- | --- |
| `cargo test -p rustsimhub-nkn-agent --all-features --locked` | 0；65通过/0失败/0忽略，含34新增+31既有，另0 doctest |
| `CARGO_INCREMENTAL=0 cargo test --workspace --all-features --locked --no-fail-fast` | 0；59段，1196通过/0失败/6继承忽略 |
| `CARGO_INCREMENTAL=0 cargo clippy --workspace --all-targets --all-features --locked -- -D warnings` | 0；严格整仓通过，含本包最终源码 |
| `cargo check -p rustsimhub-nkn-agent --no-default-features --locked` | 0；通过 |
| `cargo fmt --all -- --check`、`git diff --check` | 0；通过 |
| 当前源码 `python3 -m unittest discover -s tools/rootfs/scripts -p test_rootfs.py` | 1；35通过/1失败，见下 |
| origin/main 隔离 `git archive` 同命令 | 1；同一35通过/1失败，见下 |

首次完整整仓运行在收紧首次inspect预算前已1195通过/0失败/6忽略，未拿来冒充最终
代码的整仓结果；最终结果以上表同HEAD重新运行的数据为准。忽略项是继承的特定
Root/tmpfs/子worker环境测试，不计通过；未新增ignore。
保留已存在的 proc-macro-error2 2.0.1 future-incompatibility 提示，不宣称无通告问题。

Rootfs 的同一基线失败是 `test_rust_core_rule_requires_exact_reviewed_bytes_and_safe_file`
（test_rootfs.py:434）：`rust-core-policy-not-exact: etc/polkit-1/rules.d/45-rustsimhub.rules`。
实际策略 SHA256 `1712437963bf11875e63ffce7aa74c669841e53bd25d4422f4adec87d6875f04`；
imagepolicy.py要求 `94a7c6b199117ba94d70aa2bb43b37a9ac77fdc89017eec4f18020e265768bbf`。
隔离快照只导出当前 main 的 tools/rootfs 和该策略文件；实际执行全部36项测试。
本 PR 不改这些文件、权限/哈希或断言；远端整体 CI 不能据本地 Rust 通过写成绿。
远端按最终 PR HEAD 单独回读，不移用其他 PR/run 的结果。

日志仅留工作区 `work/a26/`；不提交原始日志。SHA256摘要：

| 工作区日志 | SHA256 |
| --- | --- |
| package-test.log | `9965adab3c7bee8d8f576c29c49d039b0e132747c300ab68fa1368be5964fa06` |
| workspace-test.log | `02e8b05369567613842500bcfc566121daffc7ca5ce28f8277fdcc7452dad6a2` |
| workspace-clippy.log | `a64827d9d41f26afc8d2917d32ee3f5330b5672c93fe2966dfd88ab137656ffc` |
| production-check.log | `443cf03ea927816bcdc935dd30b7480997910ece31301bc8c7ddc6e6d77de156` |
| fmt.log | `e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855` |
| first-failure.log | `45b7ac2715634da505a5693e4c0793c4177ebc664febfd0e97c2881d1ef30ef7` |
| short-authority-failure.log | `a269746cd0b48b308f683713b119649f08bead119f131636a8d0c5c1c2b2948c` |
| rootfs-current.log | `e49b6a5d1911baf8e926832d5910ec56e76af31adaf724bc86750505120ca205` |
| rootfs-main-baseline.log | `d9a47c7e67fffb585ae32eb08e2fd7d41d71246c871b92b33644af001d588558` |
| tested-source.json | `7de284b9ea18ea24c6ab79c81bd94948e1e452d7420b85b72fc6bea3c8340bba` |

## 剩余9步与主线接线

`member-a-remaining-implementation.md` 已按本轮最新 main/合并状态核对。
A26 的本地策略缺口已推进，但其协议/签名域批准、真实成员/安全会话和 D 的普通
Core/媒体桥仍缺，整步不标端到端完成。A27 依赖套件/依赖/绑定转录/WASM审批，
crypto_wire_frozen=false；A13～16 依赖真实 SDK/身份/固定出口，再重做实际进程和
双架构构建；A22 依赖真实状态采集；A25 已合并监督库保持，Root/cgroup/低权与
固定出口实际提供者待审；A29 的独立 SMTP/TLS 已交付，生产兼容/queue/worker/Core
Fence接线和获准邮箱实测仍缺。A30 只做真实组合收口，不能重复扣除这些步骤。

主线先按本候选完整接口审核；真实提供者可逐个替换 unavailable，但默认启动和
生产接线必须另交精确审查。若回退本包，仅不导出/不接入路由库，不变更原业务
数据库、Root材料或现有监督器。同步库不能强占不守约阻塞端口，不保证设备无PID，
不把策略前后检查当实际Core副作用点的授权/撤销证明。

没有生产设备/NAS/Root/系统总线/真实NM/NKN/RPC/链上交易/邮箱操作，没有发送
短信、拨号、蜂窝流量、签名、部署或合并。C组装与负责人最终环境验收保持原分工。
固定 P0–P7 按当前 main `docs/PROGRESS-SCALE.md` 仍约80%，不改全局台账/看板。

来源与许可：沿用 `docs/nkn/fixed-upstream.json` 的固定 nknguard 来源和已有作者；
本包未复制其Go成员密码/转发源码，无新增第三方实现。新Rust AGPL-3.0-only。
OpenAI Codex辅助实现、复核和验证；实耗未知，不估造工时。
