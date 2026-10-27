# 2026-10-27 手动合并后的代码复核

**结论：本轮未发现需要回滚的阻塞缺陷。** 结论绑定合并后的主线 `2f1595aa3b71fbfdefdbc8801ff6ecd844144b85`，包括逐项代码审查、独立 Linux 验证及主线 CI 日志核对；不代表没有潜在缺陷，也不以 Codex 作者、合并或绿勾代替验收。

## 实际合并关系

| PR | 核对结果 | 本轮接受边界 |
|---|---|---|
| [#4](https://github.com/Viper-Boss/RustSimHub/pull/4) | 用户合并至 main，合并提交 `b1a926d6e5ab8b2c3fcc0c835280f8677b838531` | DNS 测试夹具先预留成对 UDP/TCP 端口；不改变产品出口、发送重试或截止时间 |
| [#5](https://github.com/Viper-Boss/RustSimHub/pull/5) | 先合入 #4 的功能分支，提交 `8a3e520c111771c13b6eb265d7920e2a22c00037`，再随 #4 进入 main；PR HEAD `7ecf8c93c1b21dca6874d1d9b8e7de1e4f5aeccb` | U1～U3 版本能力、只读 outbox 检查及候选/回选比较；UPD-01 尚未完成 |
| [#6](https://github.com/Viper-Boss/RustSimHub/pull/6) | yh1030，HEAD `9c0802f30607f1badfb2666c3a46ba12a866a408`；合并提交 `c9763c17407a382b51ec80c329618c290e73d0d1` | C-05 只读历史预检，offline_accepted |
| [#7](https://github.com/Viper-Boss/RustSimHub/pull/7) | yh1030，HEAD `b13bae7ea80a4c8f45396609b22bfe1f8cc42923`；合并提交即本次主线 | C-06 幂等、失败与恢复的离线范围，offline_accepted |

#3 的既有审核及原作者保留，见 [PR #3 审核](REVIEW-PR-3-751e4a9.md)。本轮没有执行管理员合并、推送 main、重写作者或部署设备。

## 代码核对

- **#6 生产代码：** `crates/compat/src/history/preflight.rs`、`history.rs` 和 `migrate.rs`。新入口复用原记录解码/哈希与私有文件检查。原导入流程的契约校验、事务与写入逻辑保持；解码抽取与原逻辑逐项对照一致。
- **预检语义：** 无目的档案时明确没有比较、导入资格为未知；不把未知数当零或当全部新增。缺表/字段、未知列/类型、重复键冲突、超限和非法文本有有限拒绝原因。未知表名、正文、号码与凭据不回显。
- **文件边界：** 仅私有离线文件，拒绝符号链接、硬链接和 SQLite 辅助文件；现有档案使用共享锁及只读打开，检查前后哈希。不初始化目的档案、不创建锁文件或迁移数据。报告仅为当时观察，不能作为后续导入授权；导入仍须重新验证。
- **#7 范围：** 只有测试模块、测试入口和交付说明，没有修改生产导入器。真实 SQL 触发器在插入发生后拒绝、真实子进程 SIGKILL 和文件大小上限覆盖事务回滚/重开/重跑；丢失成功回执后重复导入保持 ID 与批次。
- **测试真实性：** 被标为 ignored 的 `import_worker` 是父测试显式用 `--exact ... --ignored` 启动的子进程入口；并非遗漏恢复测试。父进程有超时、退出码/信号断言及 Drop 清理。未把 RLIMIT_FSIZE 说成 ENOSPC，也未把 SIGKILL 说成真实断电。
- **历史隔离：** 真实 HistoryService 的 realm、principal generation、cursor audience 和 archive binding 均覆盖。当前 SIM/身份哨兵是合成旁路文件，没有连接真实 Core/棒子；因此不据此宣称实机换卡或历史集成已验收。历史 submitting/dialing 仅存档，不重新发送或拨号。

## 独立执行结果

从 Git 精确主线归档到独立目录，归档 SHA-256：`c36d9a625d823099860f36071835b989472ede5b3de019a97767a31c3e47895b`。原 U4 工作区与未提交文件保持。测试使用全新 target、只读源码、UID 65534、无外网/设备的 Linux 临时服务；Rust 1.98.0，Go 1.27.3，离线依赖与工具链校验。

| 命令 | 结果 |
|---|---|
| `cargo test -p rustsimhub-compat --locked --offline -j 2` | 32 项库测试 + 9 项迁移/互操作测试通过；1 个专用 worker 由父测试实际启动 |
| `cargo clippy -p rustsimhub-compat --all-targets --locked --offline -j 2 -- -D warnings` | 通过 |
| `cargo fmt --all -- --check` | 通过 |
| `go vet ./tools/nas-adapter` | 通过 |

9 项中包含真实 Go/CLI 契约入口、7 个恢复父测试及预检入口。结果保存时间为 2026-10-27 08:41 UTC；完整日志与私有证明不公开。

同一主线的 [CI 40472423183](https://github.com/Viper-Boss/RustSimHub/actions/runs/40472423183) 的 `test` / `native-audio` 均 success。已读取实际 test job `122082462342` 日志，确认新预检/恢复、U3 比较及实际 CLI 步骤执行；本机独立复核仅覆盖本表，不能冒充独立重跑了完整固件 CI。

## 仍保留的问题与下一步

1. #5 先前两轮检查 `40451102823`、`40451107201` 在既有 Owner 转发协调夹具的 `probe_closed` 等待失败；当前主线通过不证明间歇故障根因已解决。保留为后续稳定性调查，不放宽鉴权或自动重发。
2. UPD-01 的实际 Root 清单、签名字节绑定和数据锁下维护准入仍未完整交付。当前 U4 未提交现场保留；本轮不将 #5 的合并算作安全升级流程完成。
3. C-05/C-06 接受的是离线范围，成员 C 现为 **6/16**；C-07～16 及真实 NAS/设备补验继续保留，不接管成员后续任务。
4. 全局 NAS-05/NAS-06 不因两笔离线 PR 完成而直接置完成。原 P0～P7 **80%**；扩展行动台账仍 **14/169，剩余155，加权7.3%**。

本轮只做代码与合成测试；没有新电话、短信、推送、普通蜂窝数据、PMIC/关机或分区操作。
