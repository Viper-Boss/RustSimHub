# U6 · 维护准入暂存（UPD-01 阶段六）

2026-10-28。用户确认最小方案后实施：新增一个单独的准入暂存命令，现有 `stage`/`choose` 一行不改。**只在电脑上写和测，没有上设备。UPD-01 仍未完成**（见文末）。

## 命令

```
rustsimhub-update admit-stage ROOT KEY PACKAGE
```

参数与 `stage` 相同。退出码：`0` 比较兼容且已暂存；`3` 需要维护、`4` 被拒绝，这两种情况都**不暂存**，只输出报告；`1` 是其他错误（同样不写入）。

## 做了什么

1. **信任从已安装存储中解析**：`Store::open` 持有 `.update.lock`，并要求公钥与 `state.json` 里记录的 `key_sha256` 一致。之后验签和程序检查都使用这把已绑定的公钥，不另外接受调用方提供的信任。
2. **角色从已安装存储中解析**：没有待定候选时，已确认组（confirmed）同时是当前运行组和候选失败后的回退组，新包是候选。已有候选（`Busy`）或没有已确认组（`Denied`）时，在检查任何程序之前就拒绝。已确认组的字节会重新读取，并重新核对哈希、签名和全部载荷。
3. **核对 systemd 实际生效的配置**（`admission/systemd.rs`）：固定执行 `/usr/bin/systemctl show --all --no-pager -p LoadState -p FragmentPath -p DropInPaths -p NeedDaemonReload rustsimhub-core.service`，清空环境，限时 5 秒，输出上限 16 KiB。要求 `LoadState=loaded`，`FragmentPath` 必须是 `/etc/systemd/system/rustsimhub-core.service`，`NeedDaemonReload=no`，并且 drop-in 必须**正好**等于磁盘观察得到的 Root 模式所对应的集合（device 没有 drop-in；forwarding 只有 `30-forwarding.conf`；setup-ap 有 `30-` 和 `50-`）。这样，`service.d/`、`rustsimhub-.service.d/` 这类前缀目录，以及 `/run` 下的 drop-in，磁盘快照不覆盖的部分也能被发现。WSL 中真实 systemd 259 的输出格式与此一致，已核对。
4. **只有比较兼容才暂存**：U5 签名比较的三个角色依次为 已确认组 / 新包 / 已确认组。比较结果兼容时，在暂存之前**再核对一次**生效单元和全部数据锁、文件观察，然后才调用原有的 `Store::stage`。整个过程中，维护快照持有的数据写锁一直保持（因此 Core 必须已停止）。

## 测试（失败路径为主）

- systemd 解析（3 项）：三种模式各自只接受自己的 drop-in；未覆盖目录的 drop-in、`/run` 下的 drop-in、前缀目录、待 reload、非预期的 fragment、未加载均拒绝；缺少、重复、未知的属性，缺少 `=`、空输出、非 UTF-8 也都拒绝。
- 准入（3 项）：兼容时暂存，floor 升到 2，报告中 `authorization_verified=false`；比较被拒绝（数据库 schema 不符，退出码 4）时不暂存；生效单元核对失败时，一个程序都不检查；比较通过后数据快照发生变化（`Busy`）时，`state.json` 字节不变、候选目录不存在；已有候选时为 `Busy`；没有已确认组时为 `Denied`，且不生成 `state.json`。

## 验证（本机 WSL2，CI 因账单未启动）

HEAD `468078b`，经 `git archive` 导出，SHA-256 为 `47af15c0a9ca569683314015d9b0b93420302811a91e580ff8d2f98c7f2425c5`，共 795 个文件；rustc 1.98.0，使用全新目录和独立 target。

| 检查 | 结果 |
|---|---|
| `cargo fmt --all -- --check` | 0 |
| `cargo clippy --workspace --all-targets --all-features --locked --offline -- -D warnings` | 0（第一次运行发现本次新增代码有 1 个 `type_complexity`，已在 `468078b` 修复后重跑） |
| `cargo test -p rustsimhub-update --all-features --locked --offline` | 42 通过（U5 时为 36 项，本次新增 6 项），2 个需要 Root 的用例按设计忽略 |
| `cargo test --workspace --all-features --locked --offline --no-fail-fast` | 639 通过 / 7 失败，与 U5 基线的 7 个已知 WSL 环境失败完全相同（5 个 forwarder `egress`、1 个需要 Go 的 compat 用例，以及 1 个被它 panic 连带的 compat 用例） |

Root 降权夹具没有重跑：`capture` 本身没有改动，只是把 `Reap` 的可见性改为 `pub(super)`，以便 systemd 模块复用。另有一次运行因为旧目录中有 Root 所有的缓存文件而清理失败，那次结果作废，换用全新目录重跑。

## ARM64 交叉构建与 QEMU（同日补做）

用 `b14ad4f` 经 `git archive` 导出（SHA-256 `25802771e46439c2e39c6453439affc56c5473bd1bab4b745a981a4d53a5dd75`），在 WSL 用现有交叉工具链（GNU aarch64-linux-gnu-gcc 15.2，`-mcpu=cortex-a53`）和 rustc 1.98.0，在全新目录中编译。

| 检查 | 结果 |
|---|---|
| `cargo build --release --workspace --no-default-features --locked --target aarch64-unknown-linux-gnu` | 0，用时 106 秒 |
| 9 个程序的 ELF 头 | 都是 AArch64；所需 glibc 最高 2.39，未超过设备的 2.41 |
| QEMU：`rustsimhub-update --build-info` | 0；`source_archive_sha256` 与导出包一致，`database_schema` 6，`maintenance_observation_protocol` 1 |
| QEMU：非 Root 运行 `admit-stage` | 退出码 1，`Denied`，没有读写任何路径 |
| QEMU：`cargo test -p rustsimhub-update --all-features --target aarch64-unknown-linux-gnu` | 42 通过，2 个需要 Root 的用例按设计忽略 |

这只是本机构建和用户态模拟，不是签名发布包，也没有在设备上运行。本机 `.local/wsl-cross/gnu-root.txt` 记录的路径已经过时，指向一个源码目录；实际的 sysroot 是该路径下的 `usr/aarch64-linux-gnu`。这一项记为 P3，只记录。

## 限制与尚缺

- **没有上设备，也没有在完整的设备 Root 上做端到端运行。** `show()` 与真实 systemd 的交互只核对了同一条命令行在 WSL 中的输出格式。
- **存储路径**：开机选择单元通过 `BindPaths` 把 `/opt/rustsimhub/versions` 挂到存储目录的 `versions` 下。在宿主机上直接运行（不论 `stage` 还是 `admit-stage`），写入的是底层目录。真正的维护入口要在相同的挂载视图里运行，这属于 UPD-02（签名维护入口、固定写入目标）。
- `authorization_verified` 保持 `false`：谁有权发起这次维护，由 UPD-02 处理。
- 需要在服务停止的维护窗口里运行；Core 运行时，数据写锁会让命令返回 `Busy`。
- 仍缺：签名发布包、主线审核合并、设备交付，以及 UPD-04 中实机注入候选失败的验证。

原 P0–P7 约 80%，台账 14/169（加权 7.3%）不变。
