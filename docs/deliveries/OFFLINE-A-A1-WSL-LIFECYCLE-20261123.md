# A1 跨进程重启与授权到期复验（2026-11-23）

本人草稿 PR #268 的离线续测覆盖 STAT-03、STAT-06、STAT-07。项目 P0–P7 验收进度为 **81.5%**。

- 父提交为 `4e8ccc781f713fbb6f0a4a8dfe1e073061cd13d0`，起始 main 为 `de663743bd726e77f7c560139d0f91ea78e8fe57`。
- 受测源码树为 `cdac532d75de9da68eef1da2f5c43b33b861f24d`。固定源码归档和 ELF 的 SHA256 已记录，交付提交的完整 SHA 可在 PR 的 Commits 页面核对。[脱敏证据](../offline-a/a1-wsl-lifecycle-evidence-20261123.json)
- 提交内容为 Core 测试、隔离夹具和交付证据，沿用 AGPL-3.0-only。SDK／Root 二进制复用原 `18cbd7c405d844d24de15eb115880217cf8f5fad` 构建，并核对其对应 Go 源码、构建入口和产物哈希。

## 实测结果

| 场景 | WSL2 实测 |
|---|---|
| 跨进程重启 | 第一进程完成一次提交，合成本地节点丢弃回执。第一进程退出且原 SDK 进程已回收后，驱动程序启动不同 PID 的第二进程。第二进程重新打开原 SQLite、Core 身份和统计 FileStore，只查询原摘要一次。节点总计一次 nonce 查询、一次提交、一次原摘要查询。Pending 保留，查询结果为 `Unknown`。仅统计查询冷却使用夹具逻辑时钟，Auth 和 Root 使用真实时钟。 |
| 300 秒 Root 授权到期 | 从原 Owner 认证起实际经过 **300.102 秒**。原注册存续期间使用 Core 的 250ms 轮询。节点在原 BOOTTIME 截止前 **723ms** 收到唯一一次提交，等到过期后才尝试回复。Core 拒绝原授权的后续请求，在途结果为 `Unknown`，原 Pending 摘要保留。退出上报只释放自己的引用，Manual／Remote 两个引用仍在账上。其共用的旧 Root 授权同时到期，实际 SDK 进程由 Root 回收。 |
| 共享引用与丢失回执回归 | `shared` 与 `lost_ack` 均通过。授权有效时，退出上报后其他引用继续复用同一个 SDK。两个场景均有一次提交和一次原摘要查询。 |

## 命令与结果

Ubuntu 24.04.5 LTS WSL2，Rust 1.98.0，使用先前固定的本地 Podman 镜像。容器使用 `--pull=never --network=none --cgroupns=private`、只读输入和专用输出。夹具核对 mnt／net／pid／cgroup 隔离，在私有命名空间安装合成测试布局。四个容器结束后，专用状态里无运行容器。公网链和设备验收仍待安排。

| 命令或入口 | 结果 |
|---|---|
| `cargo test -p rustsimhub-core --lib --features usage-root-transport-candidate --locked --offline --no-run --message-format=json` | 退出 0，复制并核对测试 ELF |
| 私有容器 `/core-test --ignored --exact nkn_root::check_in::tests::statistics_transport_kernel_sdk --nocapture` | `process_restart`、`shared`、`lost_ack`、`root_expiry_300` 各退出 0。同一入口运行四个场景。 |
| `cargo test -p rustsimhub-core --lib --features usage-root-transport-candidate --locked --offline nkn_root::` | 25 通过，0 失败，2 忽略 |
| 同一特性组合筛选 `nkn_statistics::tests::` | 5 通过，0 失败 |
| `cargo fmt --all -- --check` | 退出 0 |
| `cargo clippy -p rustsimhub-core --all-targets --features usage-root-transport-candidate,email-production-candidate --locked --offline -- -D warnings` | 生产特性组合退出 0 |

前两轮到期测试分别在约 298 秒与 151 秒丢失 Ready，保留原失败记录。最终台架每秒刷新合成网络快照，原子交换文件并保留旧 inode，使在途 Root 读取仍能看到单链接文件。跨进程首轮的两个 Core 进程均退出 0，夹具汇总被临时文件权限拦住，修正文件打开模式后复跑通过。只启用 `usage-root-transport-candidate` 的首轮严格 Clippy 报告了四处既有转发测试的同类型转换，本轮按上一份报告的生产特性组合单独记录结果。

## 交接

可信链包含证明、确认深度、重组以及长期授权的更新和撤销策略待负责人定案。生产上报保持 OFF。当前证据支持离线重启恢复和原授权到期，生产链确认、真实设备与其他跨阶段组合仍待验收。负责人可按源码树和产物 SHA256 复跑。原始日志和本机绑定入口保存在 `.local/`。
