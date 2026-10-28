# U5 · 签名程序元数据检查（UPD-01 阶段五）

2026-10-28。这一阶段的代码由 Codex 在额度用尽前完成，留在主工作区 `codex/update-maintenance-20261027`（HEAD `529a541`），一直没有提交。用户决定由 Claude 接手 UPD-01，并选择原样复制到新分支提交为 WIP（`579d7ae`）、主工作区保持不动。本记录补上 Codex 没来得及做的整仓回归。**UPD-01 仍未完成。**

## 内容

- `SignedPrograms`（`crates/update/src/admission/programs.rs`）：先验证完整的签名应用组，再把**已验签的可执行文件字节**写进密封的 memfd，以 UID 986、清空环境和附加组、`no_new_privs` 运行一次 `--build-info`，限时 3 秒、输出上限 16 KiB。元数据只从这些字节产生，不接受调用方提供的 JSON；如果缺少回收降权子进程所需的能力，在启动子进程之前就拒绝。
- `check-maintenance-versions KEY SELECTED CANDIDATE FALLBACK`：在持锁观察的基础上，用三个组的签名元数据做 U3 比较，输出报告并返回分类退出码。**这仍然只是诊断，不授予任何写入权限。** 报告中 `installed_trust_verified`、`slot_identity_verified`、`effective_systemd_verified`、`authorization_verified` 都是 `false`。

## 验证（本机 WSL2，CI 因账单未启动）

`579d7ae` 经 `git archive` 导出，SHA-256 为 `f94831b32ec32b930a56aed6e83cf57ba90e4630c21b4bf874766315ce503a21`，共 793 个文件；rustc 1.98.0，独立 target。

| 检查 | 结果 |
|---|---|
| `cargo fmt --all -- --check` | 0 |
| `cargo clippy --workspace --all-targets --all-features --locked --offline -- -D warnings` | 0 |
| `cargo test -p rustsimhub-update --all-features --locked --offline` | 36 通过，2 个需要 Root 的用例按设计忽略 |
| Root：`unshare --net setpriv --no-new-privs tools/test-metadata-process.py` | 通过：实际降到 UID 986，附加组清空，环境清空，执行环境封闭 |
| Root：同上加 `--bounding-set=-kill --missing-kill` | 通过：缺少 kill 能力时，在启动子进程前拒绝 |
| Root：`unshare --net tools/test-maintenance-observation.py`（U4） | 18/18 |
| `cargo test --workspace --all-features --locked --offline --no-fail-fast` | 633 通过 / 7 失败，均为已知 WSL 环境失败：5 个 forwarder `egress`、1 个需要 Go 的 compat 用例，以及 1 个被前者 panic 连带的 compat 用例（单独运行时 7/7 通过） |

第一次跑 Root 夹具时，命令参数在跨 shell 传递时被提前展开，夹具实际没有执行，那次的结果作废；改用脚本文件后完整重跑，上表是重跑结果。

## 尚缺（UPD-01）

1. **角色绑定**：selected、candidate、fallback 和信任公钥应当从已安装的更新存储（`state.json` 的 confirmed/candidate 和已安装的信任）中解析，而不是由调用方传入目录。
2. **systemd 实际生效配置的核对**。
3. **有限维护准入**：stage/choose 需要以这次比较通过为前提，并且在持锁期间完成。这会改变设备升级路径的行为，实施前需要负责人确认方案。
4. ARM 生产构建、主线审核合并，以及设备交付。

原 P0–P7 约 80%，台账 14/169（加权 7.3%）不变。
