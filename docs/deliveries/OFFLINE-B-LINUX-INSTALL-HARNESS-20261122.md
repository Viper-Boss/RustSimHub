# B：独立安装 harness 编译修复与 Linux 复验

起始 main：`de663743bd726e77f7c560139d0f91ea78e8fe57`，本轮经 GitHub 回读仍为该 SHA。
分支：`codex/offline-b-linux-install-harness-20261118`。
完整受测/提交 HEAD 另绑定在 PR 正文，避免报告自身的哈希循环。

## 已复现的问题与最小改动

原安装 harness 直接编译真实 `crates/update/src/product_materials/mod.rs`。
新增的 `rootio/owned.rs` 两项单元测试使用 `tempfile`，但独立 harness 没声明该测试依赖。
原命令 `cargo test --manifest-path tools/product-install/harness/Cargo.toml --locked --offline`
因此以 E0433 编译失败，不能执行原有回归。原失败日志保留在任务自己的测试目录。

仅给此独立 harness 增加 `[dev-dependencies] tempfile = "3"`，相应独立锁文件
新增 tempfile 3.27.0、fastrand 2.5.0、once_cell 1.21.4，并给 harness 依赖表加 tempfile。
逐项比对确认原有全部依赖记录不变；新包版本/校验值也与原主仓锁中的记录一致。
没有改根 Cargo.toml/Cargo.lock、生产模块、接口、角色、权限或默认启动。

新 CI 专门运行此 harness 的原有两个非特权库测试，触发路径包含真实源模块。
不以主 workspace 的测试替代独立 manifest，也不把 CI/job 标题当成 Root 或安装验收。

## 实际验证

用户已明确允许安装必要工具。测试在本任务新建的 Debian 13 x86_64 虚拟机中执行，
PID1 为 systemd，内核 6.12.117+deb13-cloud-amd64，Rust 1.98.0。
固定 Node22.15.0、Go1.25.7 及 Rust 均核对官方 SHA256，Debian 镜像和 QEMU 核 SHA512。
虚拟机仅使用自己的新建 COW 虚拟盘、合成钥匙和回环管理端口，没有挂宿主目录或设备。
512MiB 客体/2CPU、2GiB 客体交换文件，宿主独立 Job 有 3GiB/32进程/2小时上限。
一般公网出口关闭；安装阶段的 Debian 包只经过限定官方域的回环代理，APT 原验签保留。
测试阶段 Cargo/Go 都关闭网络，包归档逐项校验原锁 SHA256 后在客体建独立缓存。

修复后的开发字节实际命令：

```sh
umask 022
cargo test --manifest-path tools/product-install/harness/Cargo.toml --locked --offline
```

退出0，原有两项测试通过，0失败/0忽略；另有0项 doctest，不累计成测试。
它们实际核验 inode 所有权证据、同字节替换冲突，以及拒绝意图后只清理原暂存文件。
没有新增镜像实现或“只返回成功”的测试端点，也没有删除/放宽原断言。
最终干净提交上的 `--lib --locked --offline` 再验证结果在 PR 正文绑定。

另有独立的历史证据，不能冒充本 PR 的测试：PR270/271 的精确组合
`5a62506ae377b2fc092e29798387adc0f4c851b1` 已在同一客体通过12组原有 Linux
材料测试及完整来源验证。最初缺旧基线 tree/blob、umask0002 导致安全拒绝的三组失败
均保留；补真实对象并以 umask022 建新 checkout 后，原断言全部通过，没有改安全策略。

## 未完成与主线接入

- 当前修复仅恢复独立 harness 的两个非特权回归，不是完整 Root/PID1/cgroup/nft/安装链。
- 升级 harness 原锁另外需要 smallvec1.16.4、tokio1.53.3，已核原锁哈希补缓存；其执行结果
  另报，不将这项环境准备写成本 PR 通过。
- ARM/sysroot/SDK stage、完整未签名制品、物理掉电/恢复、设备/公网与正式签名安装仍分层验收。
- 云端 CI 按提交后的具体 HEAD 回读；计费零步骤不算通过，不改计费/保护或重复刷 CI。

主线可独立审核本修复，不依赖合并 PR270/271。回退仅撤此独立 dev-dependency、锁新增项和
专用 CI，不涉及业务数据。未触碰 A、BLE/AirSIM/native 音频、已有身份、设备/NAS或私有维护目录。
所有工具安装都在自己的任务目录/虚拟机，未改 Windows 开机或生产服务。
新代码沿用项目 AGPL-3.0-only；依赖及校验以各原锁为准，Codex 辅助实现/实际检查，未编造工时。
整体完成度保持81.5%，全部 B 任务尚未完成；本报告不更改全局看板或验收权重。
