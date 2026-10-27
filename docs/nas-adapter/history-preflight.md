# C-05：历史导入前只读差异报告

基线 main：`6f964ceb53a5d1effba9bf79748b327b7375b353`。
本项复用已有私有离线 SQLite 迁移契约，增加检查入口，**不执行导入**。
开发和验收仅使用合成短信、通话、联系人及其关联元数据，不连接真实 NAS 或设备。

## 使用入口

Linux 上，使用现有 `rustsimhub-migrate` 二进制：

```sh
rustsimhub-migrate preflight "$PRIVATE_OFFLINE_SNAPSHOT" "$SOURCE_NAMESPACE"
rustsimhub-migrate preflight "$PRIVATE_OFFLINE_SNAPSHOT" "$SOURCE_NAMESPACE" "$PRIVATE_ARCHIVE_DIR"
```

来源 namespace 沿用现有 64 位小写十六进制来源标识。目录必须为调用者拥有的规范绝对私有目录
（不允许其他用户权限），文件必须为调用者拥有的私有普通文件，不能是符号链接或硬链接。
来源存在 `-wal`、`-shm` 或 `-journal` 时拒绝，必须先提供一致的离线快照。
这条命令不会创建目标目录、`.history.lock`、历史数据库或 staging 文件，也不会发起网络请求。

结果以 JSON 写到 stdout：

- 退出码 0：本次请求的检查范围未发现阻断问题。
- 退出码 1：发现阻断问题，stdout 仍给出有限、脱敏的拒绝报告；stderr 不回显路径、号码、正文或数据库错误详情。
- 非 Linux 仍明确拒绝迁移文件访问，不采用较弱的所有权/权限检查。

未给目标目录时，`archive.comparison=not_requested`，`eligible_for_history_import=null`，
有效记录记入 `uncompared`；**来源检查成功不表示已经完成冲突检查**。
给定目标后，比较结果为 `new_archive`、`compared` 或 `rejected`；
来源先被拒绝、目标尚未检查时为 `not_checked`。
`eligible_for_history_import=true` 只表示本次瞬时检查没有阻断问题，不是授权凭据、写入或导入成功回执。
之后若运行现有导入器，它仍执行自己的全部校验，不接受此报告绕过检查。

## 格式和版本

`report_format=1` 是本报告格式版本。
来源的 `format=legacy_nas_sqlite_column_contract_v1` 只表示现有十张静态表的列契约匹配，
`column_contract_revision=1` 是本地字段契约修订，**不是 NAS 软件版本**。

报告另外记录实际读取的 SQLite `application_id` 和 `user_version`。
旧列契约没有可信 NAS 产品版本字段，因此 `product_version=null`，不从时间、文件名或记录值猜造版本。
缺表/列时格式为 `unknown` 并列出静态缺失字段；已存在的目标归档必须通过原有
`application_id=1381189709 / user_version=1` 检查，错误版本为 `unsupported_archive_format`。

来源 namespace 与快照 SHA-256 用于区分来源和检查快照是否变化；不从原始 SIM/设备标识生成新当前线路。
同一旧 ID 在不同 namespace 下保持不同的历史记录，不自动合并。

## 记录类型、缺值和差异

`tables` 只使用已有契约中的静态名称：

| 主记录类型 | 表 | 关联元数据 |
|---|---|---|
| 短信 | `sms` | `sms_contacts`、`modemdeck_user_message_thread_state` |
| 通话 | `call_history` | `modemdeck_user_call_state` |
| 联系人 | `contacts` | `contact_phones` |
| 历史线路与归属 | `modemdeck_lines`、`modemdeck_legacy_endpoint_lines`、`modemdeck_user_lines` | 只作为历史来源数据，不设置当前 SIM、Owner 或权限 |

每张表给出：

- `present`、`rows`、`row_count_exact`：缺失表的数量为 null，不能用 0 冒充空表。
- `missing_columns`：缺失的静态列名；有记录但缺列时，报告记录数与拒绝数，不静默跳过。
- `unmapped_columns`：已知表中契约外列的数量；不回显未知列名，明确拒绝为 `unsupported_columns`，不能把忽略新字段声称完整迁移。原导入器的投影格式保持不变。
- `missing_values`：NULL 或空字符串的字段计数，包括未知号码/时间；不输出值，也不补造默认号码/时间。
  非键列的 NULL/空值沿用现有导入器语义，不因未知时间而伪造值或拒绝合法历史。
- `counts.rows/valid/rejected`：已知数量、通过现有记录解码的数量和被拒绝的数量。
- `new/unchanged/conflicts`：与指定来源 namespace、表及旧键对应的目标历史记录比较。
  一致性比较使用现有 canonical payload 和 SHA-256，冲突不覆盖。
- `duplicates`：输入内相同旧键、相同 canonical payload 摘要的重复行数量；不同摘要为冲突。
- `uncompared`：未指定目标或目标拒绝后尚未比较的有效记录，不把它们误报为新增。

每个有效输入行归入 new、unchanged、conflicts、duplicates、uncompared 中的一项；
`valid` 包含发现冲突/重复但结构合法的行，`rejected` 用于记录类型、键或大小不合法的行。
缺列会在解码前拒绝该表，因此其 rejected 可以大于实际已解码的行数。
预测统计不代表部分导入：只要有阻断问题，整个报告不接受，不产生任何写入。

未知表和视图明确记为不支持：`unsupported_tables/unsupported_known_rows/unsupported_counts_complete`。
未知名称可能含个人信息，因此默认仅输出数量；不回显表名，不执行未知视图或虚表的内容查询。
已知凭据表 `modemdeck_admin_credentials` 单独计入 `excluded_credential_*`，只统计行数，不读取凭据列，
不作为可导入记录。其他未知类型不会默默丢弃后宣称完整迁移。

## 拒绝原因与边界

`reasons` 是有限原因与计数，例如：

- 无效 namespace、来源私有文件/离线快照要求、忙、来源损坏/不可读/变化；
- 缺表、缺列、未映射列、不支持的记录、非法记录、无效 UTF-8、超限、记录冲突；
- 目标私有文件/离线要求、错误归档格式、不可读/变化，或来源与目标重合。

复用原 256 MiB 来源、512 MiB 归档、128 KiB 单记录和 100000 行上限；
本报告的行预算覆盖需统计的已知、未知与被排除表，来源目录对象最多 128 个，表名最多 512 字节。
计数查询最多观察 100001 行；超过上限时 `row_count_exact=false`，报告下界并拒绝，不宣称截断后仍然完整。
schema 名称严格引用，不能把名称当 SQL 执行；原始键和正文只在内部解析和比较，不进入报告。

预检复用 `Snapshot` 的私有文件/共享锁/只读 immutable SQLite、现有列契约和记录解析器，
目标复用历史归档锁/格式检查。已有归档只读打开，未初始化目标不创建锁或数据库。
结束前再次核对来源/已有目标的字节摘要及 sidecar 状态；源目标重合拒绝。
没有 Core、当前业务库、身份、Owner、当前线路或待发送/待通话操作入口。

## 验证和后续

合成夹具覆盖格式/版本、三类主记录、NULL/空值、缺表/列、未知类型、冲突/新增/相同记录、
跨来源、重复键、错误类型/UTF-8、大小、私有文件/链接/sidecar 和错误目标版本。
逐次比较预检前后的真实文件字节及目录内容，核对来源、历史和相邻身份/当前状态哨兵均保持；
用原导入器在测试的单独写入阶段核对预测的新增/已存在数量，不把预检当作导入。

Linux CLI 入口测试调用生产参数解析和同一真实 SQLite 实现，校验成功/拒绝 JSON 及脱敏。
它与已有 Go CLI 夹具共享二进制测试锁，避免启动子进程时保留其他测试的历史锁描述符。

```sh
cargo test -p rustsimhub-compat --locked
cargo clippy -p rustsimhub-compat --all-targets --locked -- -D warnings
cargo fmt --all -- --check
```

实际执行结果、完整 HEAD 与 PR 见 [成员 C 交付报告](../deliveries/member-c-nas.md)。
C-05 只完成只读预检；导入中断恢复及完整权限/输入矩阵仍分别属于 C-06、C-07，
第二批综合交付属于 C-08。真实 NAS 数据、旧页面和设备验收仍由主线安排。

来源：上述固定 main 的 `crates/compat/src/history.rs` 及既有独立合成 schema，保留 AGPL-3.0-only。
不新增依赖、表格式、凭据格式或硬件操作。
