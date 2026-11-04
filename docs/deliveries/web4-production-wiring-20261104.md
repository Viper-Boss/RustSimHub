# WEB4 任务8：生产接线交付（持续验证中）

本分支从 main `a2044edfef6aeeced3d2c2eb8743791b34e4ccdc` 独立建立，复用已合并 PR208，正常整合 PR206/211、209/213、210/212 及任务依赖历史。没有合并 PR、设备操作、生产密钥、生产签名、epoch 分配或部署。全局进度保持 **81.5%**；扩展 **20/169、加权10.6%**。

## 阶段8-B：正式入口与连续守卫

正式 Core 在所有激活前缀之前处理固定 `--product-runtime-compatibility MODE`、恢复 child、恢复只读检查。updater 固定 `product-prepare/apply/inspect/recover/rollback/activate` 不接受路径、hash、UID、schema 或测试 root。原 Store/安装信任、真实签名应用、原业务/outbox writer、独立 NSS/角色策略、固定停机、原数据库一致备份与300秒期限由同一生产事务持有。

安装器借出实际 fd 支持的 inode/摘要/模式、暂存/删除操作与父目录代次。DataFence 只认可本事务固定写集，保持同一原 MaintenanceSnapshot，写集外变化永久失效。最后日志落盘后增加最终校验。原 email 接口新增返回同一快照的变体，旧接口语义保留；原恢复 child 的 R/G/D 交接只释放业务 writer，Root 在 R 点重新核对完整源，D 后核对实际日志及数据，再取得业务 writer。没有通过丢弃整个快照再观察伪装连续持锁。

## 阶段8-C：显式组合与生产构建（验证中）

新增有限 `product-recovery-health-v1`：14个固定 Root 目标，launcher 保留健康/产品/恢复前缀。正式模式元数据报告实际 schema9，运行确认使用原 socket、SQL/Auth lease、flock 与最终 Recheck/state.next 协议。旧 schema6/forwarding 合同保留。v1 单 Core profile 要求 baseline/candidate 的 Core 字节相同且二者实际支持新模式；不符合的旧 baseline 拒绝，需单独升级基线。

原 PR205 builder/packager 已扩展此显式 profile，实际固定 MODE 探测进入构建清单。仍未声称这些材料是可启动固件。

## 当前验证与待办

- `cargo check -p rustsimhub-core -p rustsimhub-update --all-features --locked --offline`：通过。
- updater/launcher 全 feature 测试编译：通过。
- 原业务/outbox 锁跨真实签名安装连续持有、写集外变化永久拒绝：2通过。
- Root 操作 inode 替换与拒绝后清理：2通过（后续源变更需最终重跑）。
- 一次 updater 全 feature 库测试：94通过、1失败、6 ignored。失败为既有子进程错误/250毫秒超时的时序断言，预期 Integrity、实际 Busy；保留失败日志，尚未以重跑替代失败记录。
- 本环境已实际安装 Rust AArch64 标准库，下载并隔离解包交叉编译器与 QEMU；C 交叉编译/QEMU 冒烟通过。
- **尚未完成**：冻结提交的正式 ARM/native 发行构建、实际 production dispatch→迁移→launcher/Core/Auth/Store→健康确认端到端、完整故障矩阵、最终 fmt/严格 clippy/web/历史迁移/来源复验。不能据当前库测试宣布任务8完成。

原始日志、临时数据库和测试密钥均仅留在忽略目录 `.local/`。最终表格将在实际完成每项后记录准确 HEAD、归档摘要、命令、计数和未运行项；不累计有重叠的历史测试总数。
