# WEB3-UPD 第一阶段：Core 健康来源与固定协议

日期：2026-11-04（Asia/Shanghai）。待审核，未合并、部署、生产签名或验收。整体 81.5%、20/169、加权 10.6% 保持。

## 固定来源与范围

起始 main：`8f9cfeff1a4b5a804b8b138e890f7386caf5a006`。正常合入 PR200 `a690213cec950b273632f595b96a0bea8f8e5e64`（包含 PR197 `ecfb9fe7d8c4b7f940db82cd9ea52b1ebb71b810`）及 PR198 `32d340f9701cb73d36fbdc443bfcf6ef7c08ff6a`，共同父基线 `7f6377c1798ae445cc8718349e90d9c494e02d2f`。依赖原作者及历史保留；继承的全局文档来自 PR198，本包未编写全局进度。超限 ELF 反例保留。

**完整受测产品 SHA：`5988d416dd05a4590559d087b971f67acc8cefd2`**。本报告的后续提交只含文档，不将文档 HEAD 冒称已重新编译。Git 作者沿用 GTW，GitHub 当前登录经回读为 wnfhsj88，未改写历史作者。

这是任务动作 1–3、5 的 Core 子范围。Root 组合准入、Store 限定确认、触发单元和完整确认正例在第二个堆叠 PR 交付；本 PR 不声称自动确认完成。

## 调用图与可信边界

显式首前缀 `--enable-update-health-candidate` → 原设备服务解析 → `authenticated::serve_inner_media` → `update_health::serve`。未指定前缀的服务没有该 listener；公共 Owner/NAS/BLE wire、恢复 actor、AP 模块均不承载此协议。

实际 Core 初始化持有的同一个 `Arc<Mutex<Auth>>` → forwarding management mutex → Auth 只读 lease → 原 Storage writer actor 内的只读事务。整个有界连接保留这些原对象及锁；没有另开业务 Store，也不迁移、创建转发队列或启动 worker。业务就绪来自已初始化设备模式、真实订阅/调和/存储错误计数及服务存活标志，不能由请求设置。

Core 实例为当前 Auth 的实际随机 instance；数据库 realm 为实际 meta.db_epoch UUID，schema 为实际 PRAGMA user_version（6/7/9），line/Owner generation 来自同一事务。实际 device key/Owner/realm 的摘要只用于连接内等值绑定，不是对外授权令牌。Linux boot UUID 仅绑定内核进程域，不转换为 Auth boot。

转发策略读取已有队列的 enabled/generation 元数据，配合同一 controller 的真实 Session generation/deadline；不读取 policy/jobs/credential 正文。未配置且目录不存在可明确 OFF；存在目录但 controller 未持有队列为未知，拒绝。OFF 必须无已准入 worker；ON 必须匹配有效 worker 代次。Root 对实际 worker/cgroup 的独立比较属于第二阶段，不能用此 JSON 自报替代。

Core 只接收内核 SO_PEERCRED UID0，重查 Root 当前 UID 和进程开始时间、comm、Linux boot；Core 不依赖无权读取的 Root /proc/exe。Root 必须在其权限域核对 Core peer PID/exe 与 PR197 的已验签运行实例一致，且不能只相信 body 中 source/instance。

## 给窗口4的固定接口

- Core 首前缀：`--enable-update-health-candidate`，只能接原显式设备服务参数；默认关闭。
- socket：`/run/rustsimhub/update-health/core.sock`。Root 创建安全祖先，专用 tmpfiles 模板在 `firmware/system/update-health/`；Core UID986 拥有叶目录 0700、socket 0600、生命周期锁 0600。仅在持锁、原 socket 同 inode 且连接拒绝时回收自身旧 socket，不删除存活/外来节点。
- wire v1：大端 u32 长度 + 严格 JSON，1–8192 字节；完整帧读取 250ms，连接观察绝对最多 5000ms，空闲轮询不续期。Observe → 一次 Recheck → Finish；同一随机 challenge、服务端随机 connection 和 snapshot 必须一致。Finish 在原 actor 事务释放后才回复 released。
- 新 wire 字段在 `crates/wire/src/update_health.rs`。拒绝未知/重复字段、请求指定 PID/path/time/health、跨连接或顺序重放。该 JSON 不能持久化后变成确认能力。
- 没有编译来源时 source_sha256 为 None，诚实表明未知；Root 必须拒绝。schema7/9 的报告不等于 schema6 Root 路径兼容。
- Root 固定入口约定 `rustsimhub-update confirm-running-candidate`，无参数，在第二阶段实现；不得在发行工具提前声明自动确认可用。旧 `inspect-runtime` 保持只读及 may_confirm=false。
- 模板没有安装到默认 overlay；无隐式启用。第二阶段的明确候选 profile 才可装配。

## 检查与真实结果

工具链 Rust/Cargo 1.98.0；按仓库锁文件，`CARGO_BUILD_JOBS=2 CARGO_INCREMENTAL=0 CARGO_PROFILE_DEV_DEBUG=0 CARGO_PROFILE_TEST_DEBUG=0`。原始日志和隔离材料只保存在忽略的 `.local/`，不提交主机路径、密钥或原始设备信息。

| 命令/场景 | 结果 |
|---|---|
| `cargo test -p rustsimhub-core -p rustsimhub-auth -p rustsimhub-storage -p rustsimhub-wire --locked` | 退出0；45 Auth、21 Core lib、195 Core bin、67 Storage、25 wire，共353通过，2项真实UID专项忽略不计通过 |
| 同四包 `--all-features update_health` | 退出0；新增6 Core、4 Storage、1 wire通过；2项UID专项忽略；与默认重叠，不相加 |
| 同四包完整 `--all-features` | 退出101；Core lib 95通过/1失败/3忽略，失败为既有 `network_trial::provision::recovery::tests::t5_tests::t5_reopened_saved_history_and_abandoned_claim_preserve_original_ids` 的 Storage；未修改窗口2模块，也不称完整套件全绿 |
| `cargo clippy -p rustsimhub-core -p rustsimhub-auth -p rustsimhub-storage -p rustsimhub-wire --all-targets --all-features --locked -- -D warnings` | 退出0 |
| 受测默认 Core lib 二进制，独立无网络 Docker、只读根、有限内存/PID/tmpfs，执行 ignored `update_health::tests::kernel_root_core_fixed_endpoint` | 退出0；UID0 → UID986，生产固定 socket 0600、真实 Auth/Store、实际数据 inode、Recheck/Finish 及 socket 清理通过；父子各一项输出属于同一场景，不加成两个独立端到端 |

内核场景只把外部业务就绪输入合成，协议生产者、授权、单 writer、Unix peer 与 UID 是真实实现。它不是完整设备 Core 守护进程、MM/NM、真实 systemd PID1、ARM 或棒子验收；也不是第二阶段确认正例。Docker 从 scratch 加已编译测试和运行库构造，不拉取镜像、不用宿主挂载/网络/NSS/系统总线。

新增失败路径覆盖无 Owner/未就绪、错误 UID、未知字段、截断/超时、连接重放、停服、数据库 inode/代次变动、断连/绝对截止释放、实际 schema9 不伪装6、原 writer 互斥与未知 outbox。早期编译的访问器参数不一致及隔离镜像文件模式、/run tmpfs 模式准备失败均保留；修正夹具后不放松生产权限检查。最终 UID 容器按冻结产品重建并重跑通过。

## 剩余、回退与审核

第二阶段必须将 PR197 的槽位/签名/实际进程与此连接绑定，核对 Root/data/前后代次，完成原账本锁升级与受限提交。旧 Core/缺来源/不支持 schema/profile 一律显式拒绝。生产来源构建、ARM/PID1、异常崩溃矩阵、实际设备最终签认仍不能由这里代替。

回退：停用显式候选 profile，并正常 revert 本包独立代码提交；不改账本、身份、epoch、attempted 或数据库 schema。依赖 PR197/198/200 仍由主线独立审核。本包代码新增沿用 AGPL-3.0-only，未新增外部源码。由 OpenAI Codex 辅助实现及检查；不估造人工有效工时。审核结论：待负责人对完整 PR HEAD 审核，不自行批准或合并。
