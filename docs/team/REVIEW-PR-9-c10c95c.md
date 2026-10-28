# PR #9 补审 · C-07/C-08 NAS 历史权限与输入矩阵

状态：2026-10-28 补审。结论为**离线范围通过**，已记入 `reviews.json`。审核由 Claude 代负责人执行。#9 已于 2026-10-27 合并，但当时没有逐 HEAD 的审核记录；本记录补上，不改变合并本身。

| 项 | 值 |
|---|---|
| PR | [Viper-Boss/RustSimHub#9](https://github.com/Viper-Boss/RustSimHub/pull/9) |
| 作者/分支 | yh1030 / `codex/nas-history-input-matrix`（原仓库分支） |
| 被审 HEAD | `c10c95ce35fb029678457069cc6354337e4121f3` |
| 起点 | `2f1595aa3b71fbfdefdbc8801ff6ecd844144b85` |
| 合并提交 | `dc8b21b061ebc1552c624d92b0ec41ed67405386` |
| CI | 账单原因未启动，无结果 |

## 范围

3 个提交，共 7 个文件，全部是测试或文档：

- 新增 `crates/compat/src/history/tests/input_matrix.rs` 和 `crates/compat/src/session/tests/input_matrix.rs`；
- 两个既有 `tests.rs` 各加一行 `mod input_matrix;`；
- 新增两份 `docs/nas-adapter/` 说明，追加本人交付报告。

**没有改动任何生产代码、依赖、schema 或 Go 代码**，路径都在成员 C 第二批的授权范围内。

## 内容审查

- 11 个用例都直接调用真实的 `HistoryService`、`Authority`、`validate`/`import`/`preflight`，没有测试专用的替代实现。
- 覆盖面：
  - 越权读取、撤销、错误 NAS 身份/令牌/权限范围/签名时间；
  - 会话过期后不能延长有效期；会话存储版本错误时不会重建；
  - 非法游标、ID 和页面大小不会变成 SQL 或权限；
  - 路径/链接非规范、来源 ID 无效；
  - 无效来源行、缺 schema、归档版本错误、超大或稀疏文件；
  - 合法 UTF-8、NUL 和未知时间原样保留；
  - 重复导入幂等，冲突重复会整体回滚。
- 每个用例都在失败前后比较相关文件的字节，证明失败不会改动原历史库和当前库。
- 交付文档明确写了：真实旧 NAS 页面和版本、合法授权的离线副本，以及 Linux/Go 全量复验，都属于主线补验项，没有夸大验收范围。

## 执行证据（本机 WSL2，非隔离服务）

被审 HEAD 经 `git archive` 导出，tar 的 SHA-256 为 `86af76238413080b5288b9754d5a8e21ec92ad4c1f3f4a286a4c6ee3ebe68791`，共 755 个文件；rustc 1.98.0，独立 target。

| 命令 | 结果 |
|---|---|
| `cargo fmt --all -- --check` | 0 |
| `cargo test -p rustsimhub-compat --locked --offline input_matrix` | 11/11 通过 |
| `cargo test -p rustsimhub-compat --locked --offline --no-fail-fast` | 50 通过 / 2 失败 / 1 忽略。2 个失败是原有的 Go 互操作用例（`nas_adapter_tests::go_nas_readonly_contract_and_synthetic_cli`、`preflight_cli_tests::history_preflight_cli_uses_real_sqlite_and_outputs_redacted_rejections`），原因是 WSL 没有安装 Go，main 上同样失败，与本 PR 无关 |
| `cargo clippy -p rustsimhub-compat --all-targets --locked --offline -- -D warnings` | 0 |

本 PR 没有改动 Go 代码，所以没有重跑 Go test/vet；Go 侧的覆盖见 #6/#7 的合并复核。

## 结论

**C-07、C-08 离线范围通过。** 成员 C 的已验收步骤从 7 步增加到 9 步。这不代表全局 NAS-05/06 动作已完成：真实旧 NAS 页面、版本和授权数据的补验仍由主线负责。
