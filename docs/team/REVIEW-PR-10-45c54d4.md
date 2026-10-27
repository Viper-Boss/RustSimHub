# PR #10 审核 · C-09～C-12 备份恢复离线矩阵

状态：负责人 2026-10-28 确认，结论为**需修改**，已记入 `reviews.json`（`needs_changes`）。审核摘要已在 PR 上评论。本审核不合并 PR，也不计入已验收步骤。审核由 Claude 代负责人执行。

| 项 | 值 |
|---|---|
| PR | [Viper-Boss/RustSimHub#10](https://github.com/Viper-Boss/RustSimHub/pull/10)，Draft |
| 作者/分支 | yh1030 / `codex/member-c-recovery-matrix`（原仓库分支，非 fork） |
| 被审 HEAD | `45c54d4b32bc00a1093cf54d4faead9795cdbdc7` |
| 起始 main | `6f228fd3dcb9ce6da54d7910f4b044833e71a91a`（即 merge-base） |
| 提交 | befa030、bfaa8dc（测试代码），a4145a4、45c54d4（仅报告） |
| CI | 账单原因未启动，无结果；不当作失败或通过 |

**结论：需修改。** 实际 Linux 执行 15 个场景中 11 个通过、4 个失败。其中 3 个失败暴露了一个真实的生产行为（见 F1），这是本 PR 的有效发现；另 1 个是夹具自身缺陷（F2）。

## 范围与安全

- 11 个文件全部是新增，路径都在 T3 追加边界内：`crates/recovery/tests/`、`tools/fixtures/backup-recovery/`、`tools/test-business-recovery-matrix.py`、`docs/recovery/`、本人报告。没有改生产代码、schema、workspace/Cargo.lock、设备脚本或他人文件。
- 只调用真实公开 API（Store/Identity/recovery/Auth/Queue），没有替代实现。转发目标是 `example.invalid`，没有构造 transport。只用私有临时目录和合成数据，不碰设备、网络或 Root。
- 驱动在 `cargo test` 里嵌套运行一次 `cargo build --locked --offline`，产物放在私有临时 target，再用 clippy-driver `-D warnings` 编译夹具，并逐个核对 15 个用例名。设计严谨，代价是 CI 多出约 30 秒构建。

## 执行证据（本机 WSL2，非隔离服务）

环境：WSL2 Ubuntu，内核 6.18.33.2，rustc/clippy 1.98.0（官方 rustup），用 `git archive` 导出被审 HEAD，tar 的 SHA-256 为 `9a18d08a758a8c116b28b98da818f3fc8895fbf355ba6efa76909cab13514ac6`，766 个文件，放在 Linux 原生目录。这次执行不是 UID 65534、无外网的隔离服务；`cargo fetch` 联网，测试本身 `--offline`。

| 命令 | 结果 |
|---|---|
| `cargo fetch --locked` | 0 |
| `cargo fmt --all -- --check` | 0 |
| `python3 -m unittest discover -s tools/fixtures/backup-recovery -p test_runner.py` | 0（4 项选择器单测） |
| `cargo test -p rustsimhub-recovery --locked --offline` | **101**：库 4/4 通过；`offline_business_matrix` 失败（内部 15 场景 11 通过、4 失败） |
| `python3 tools/test-business-recovery-matrix.py --run` | **1**：同样 11/4 |
| `cargo clippy -p rustsimhub-recovery --all-targets --locked --offline -- -D warnings` | 0 |
| Windows：fmt、夹具 rustfmt、`git diff --check`、计划模式、控制器单测 | 均为 0 |
| Windows `cargo test` | 未运行成功：本机没有 MSVC `link.exe`，属环境问题，与 PR 无关 |

失败用例：

1. `business::wal_backup_…`，business.rs:29：备份目录多出 `backup.sqlite3-shm`、`backup.sqlite3-wal`
2. `business::repeated_restore_…`，business.rs:328：`manifest(&backup) == before`
3. `business::corrupt_missing_and_newer_…`，business.rs:395：`manifest(&bad) == before`
4. `outbox::same_device_mixed_batches_…`，outbox.rs:263：`manifest(&mixed) == before`

诊断：只在审核用的副本里让 `manifest()` 忽略 `-wal`/`-shm`，其余不变，重跑后 14/15 通过，只剩 1 号失败。由此可知 `backup.sqlite3` 本身的字节没有变，变化全是新增的 sidecar 文件。

## 发现

### F1（生产行为，由本 PR 测出）`Store::restore` 会在源备份目录留下 `-wal`/`-shm`

- `Database::backup`（storage/src/lib.rs:745）用 Backup API 从 WAL 库复制，产出的 `backup.sqlite3` 文件头仍是 WAL 模式。
- `Store::restore` 用 `SQLITE_OPEN_READ_ONLY` 打开源文件（lib.rs:424）。只读连接打开 WAL 库时会创建 `-shm`/`-wal`，关闭时不清理。已用最小 Python/SQLite 3.46.1 实验独立复现。
- 影响：恢复后源备份目录多出文件，与文档中“来源保持”的说法不符。另外推断从只读介质恢复可能失败，这一点**未验证**。
- 成员 C 不能改 storage 生产代码。负责人决定由主线修复：[Viper-Boss/RustSimHub#14](https://github.com/Viper-Boss/RustSimHub/pull/14)（提交 `f8bd02dd24c3db9587a0b52622ecfb77e77b00bc`，未合并）让 `Store::backup` 在发布前把目标库切到 `journal_mode=DELETE`，恢复语义不变，并附回归测试。
- 本机 WSL2 上，#10 HEAD 叠加 #14 后，15 个场景全部通过，退出码 0。成员 C **不需要**为 F1 修改这 3 处断言，也不要改成忽略 sidecar；等 #14 合并后 rebase 即可。
- #14 不改写修复前生成的旧 WAL 格式备份，这些备份恢复时仍会多出 sidecar。C 的 `docs/recovery/` 里关于“来源保持”的表述，应注明这一点以 #14 为前提。

### F2（夹具缺陷）`wal_backup…` 自己制造了 sidecar

business.rs:24 的 `logical(&backup…)` 先用只读方式打开了备份，第 25～29 行才列目录。修正方法：在任何打开之前列目录，或者 `readonly()` 改用 `immutable=1`。注意：叠加 #14 后这个用例在本机通过，只是因为备份已变成回滚日志格式，只读打开不再产生 sidecar；测试本身先打开、后列目录的顺序问题仍应修正。

### F3（建议）`newer` 场景硬编码 `PRAGMA user_version=7`

当前 `SCHEMA = 6`（lib.rs:19）。主线一升到 7，这个负例就变成“声称是当前版本但缺表”，失去原来的含义。建议沿用 storage 测试的 99，或者用明显超出范围的值。

### F4（建议）

- business.rs:392 对 `.restore-pending` 是条件断言，这里的行为是确定的，建议直接断言实际状态。
- 交付报告里记录了成员本机安装 WSL 的过程，与本批无关但无害；文档“来源保持”的表述需按 F1 修正。

## 步骤判定

| 步骤 | 判定 |
|---|---|
| C-09 盘点 | 内容基本准确，补上 F1 的说明后可按离线范围通过 |
| C-10 / C-11 矩阵 | 需修改：被审 HEAD 本身 11/15；叠加 #14 后 15/15，F2/F3 仍待改 |
| C-12 交付 | 需修改：补 F1 说明和新 HEAD 的实际 Linux 结果 |

给成员 C 的最小修改清单：

1. 修 F2（先列目录再打开，或只读改用 `immutable`）。
2. 修 F3（不要硬编码 7）。可选再处理 F4。
3. `docs/recovery/` 和报告中补上：F1 由主线 #14 修复，旧 WAL 格式备份仍有限制。
4. 等 #14 合并后 rebase 到 main，再提交新 HEAD 的实际 Linux 结果。

新增提交会让本结论失效，需按新 HEAD 重跑。
