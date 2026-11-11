# 成员 A · 第一批离线统计组件交付

日期：2026-10-27。成员 `member-a` / `fungi1220`，包名 `rustsimhub-usage-stats`。

- 起始最新 `origin/main`：`6f964ceb53a5d1effba9bf79748b327b7375b353`。
- 实现及本地验证的完整代码 HEAD：`4346954f050b93c35331a2c617206753a7b24886`。
- 本地初始代码提交 `1c45bdbbb996cfc4f9de093e71969488d89be4a6` 与上述 GitHub 发布代码的完整 tree 均为 `bc2ad89ce7eb2a3f1b176025371023d72f2b7a37`，逐字节一致。Git 凭据存储器不可用，改用本人已连接 GitHub 发布；本地原提交保留为 checkpoint，没有改写其他作者或丢弃历史。
- 分支：本仓库 `codex/usage-stats-offline`，目标 `main`；本报告作为后续仅文档提交，PR 正文记录最终完整 HEAD，避免伪造包含自身的提交 SHA。
- 范围：A-01～A-04 / STAT-01、02、04、05 的离线子范围，待负责人逐 HEAD 审核。未将任何 STAT 动作或全局进度标为完成。
- 已读：AGENTS、HANDOFF 顶部、PROGRESS、development-plan、团队 README/ACCESS、本人任务书及正式追加、交付模板。追加任务按顺序保留，未开始 A-05～16。

## 实际行为及文件范围

新增 `crates/usage-stats/` 库与三个集成测试文件，仅在 workspace 增加该成员；Cargo.lock 只增加本包的 15 行记录，没有升级或新增注册表包。

1. `protocol` 固定三个有限窗口及 RustSimHub 专用主题、版本、块订阅/续订参数、有界严格解析、有限错误和可注入时间。`CheckInResult` 只表达关闭或本地意图准备，不签名、提交或伪造确认。
2. `identity` 用操作系统随机数或显式的统计专用种子生成独立身份，标准 SHA-256 明确域分离后由 ed25519-dalek 生成公钥。没有读取或复用 Owner、SIM、短信或通信主密钥；公有 API 不导出种子。
3. `store` 是真实版本化文件后端，保存独立私有种子、身份/产品绑定、固定窗口/周期、预留待确认摘要、最近成功、禁用代次及查询缓存。损坏或身份不符报错保留；没有静默换身份。Linux 以目录句柄、NOFOLLOW、严格权限/UID/单硬链接、生命周期独占锁及同目录同步/原子发布实现；Windows 有真实文件后端但安全保证不同，见下面限制。
4. `query` 显式注入只读 transport；每页 2 KiB、最多 16 页、总计数不超过十亿，拒绝重复字段/页、错主题/版本/窗口、过期、超限和溢出。Unix 秒与共享单调截止双预算，不从响应执行 URL、命令或路径。三窗口成功、成功的 0、未知、带错误的历史缓存明确区分。
5. 缓存保留原观察/成功/到期时间和来源；高水位发现回拨后不可逆撤销旧条目新鲜资格。集成测试实际保存、关闭、重开并核对撤销状态，而非用 map 代替重启。
6. 默认 OFF、明确 Owner 同意加当前披露。OFF 的实际调用计数为 transport **0**、clock **0**，不创建身份、队列、目录、线程或定时任务。独立库没有修改转发、远程访问或蜂窝数据开关的接口。

三篇契约文档为 [协议与来源](../usage-stats/PROTOCOL.md)、[状态与持久边界](../usage-stats/STATE-MACHINE.md)、[隐私](../usage-stats/PRIVACY.md)。未修改 Core、Edge、Root/systemd、默认开机策略、业务身份、其他成员文件或全局进度。

## 真实验证

宿主 Windows x86_64，无已安装 Rust/WSL。仅在工作区外层任务工具目录解包官方 Rust 1.98.0（`rustc 88d9e12ae`、`cargo 797e8a9bc`），每组件核对官方版本清单 SHA-256；没有系统安装或改全局 PATH。Windows GNU 自带 dlltool 需要缺失的汇编工具，改用官方 llvm-mingw `20261020` 的 dlltool（发布资产 SHA-256 `fd5462b7f5d93b2ff981fcd3f9c7e427e47f3203c64ea91c27bcc444eebd74cd`），链接器仍为 Rust 自带 GNU driver。

系统 TLS 通道最初报 `SEC_E_NO_CREDENTIALS`，没有关闭证书验证。用 Python 的验证 HTTPS 通道预取 Cargo.lock 中 311 个公开固定包和对应索引记录，逐包核对 lock/索引/归档 SHA-256，配置任务专用本地 registry 后全部 Cargo 检查离线执行。工具链、索引、缓存与二进制均未加入仓库。

| 命令 | 真实结果 |
| --- | --- |
| `cargo fmt --all -- --check` | 退出 0，整个工作区格式通过 |
| `cargo test -p rustsimhub-usage-stats --locked --offline` | 退出 0，Windows 实际运行 47 项：协议 13、真实文件持久化 16、查询/缓存 18；无忽略，无失败 |
| `cargo clippy -p rustsimhub-usage-stats --all-targets --locked --offline -- -D warnings` | 退出 0，Windows 全目标严格检查 |
| `cargo check -p rustsimhub-usage-stats --lib --no-default-features --locked --offline` | 退出 0，无默认特性的生产库检查；本包未定义后台联网特性 |
| `cargo check -p rustsimhub-usage-stats --target x86_64-unknown-linux-gnu --all-targets --locked --offline` | 退出 0，Linux 条件生产代码及全部测试编译检查 |
| `cargo clippy -p rustsimhub-usage-stats --target x86_64-unknown-linux-gnu --all-targets --locked --offline -- -D warnings` | 退出 0，Linux 条件全目标严格交叉检查 |
| `git diff --cached --check` | 退出 0，所提交文件检查通过 |

真实测试包括文件关闭重开、写入受文件系统阻挡、短/坏/大文件、版本/产品/身份/种子错配、锁竞争、孤立临时文件、原文件损坏后拒绝覆盖；三窗口离线、超时、部分失败、0、过期、重复页、元数据变化、计数溢出和超量；实际等待单调截止以证明墙钟冻结不延长预算；OFF 零调用与日志/序列化脱敏。

保留修正前失败：第一次查询测试 17/18，通过类型检查但一个“仅改观察时间”的夹具意外把 TTL 变成 601 秒，正确返回 `InvalidTime`，而测试期望页元数据不一致。将夹具 TTL 保持有效后再单独验证 `InconsistentPages`，未放宽生产校验。严格 Clippy 首次提示 Rust 1.98 的 `chunks_exact_to_as_chunks`，改为等价 `as_chunks::<2>()`，没有屏蔽警告。最终上述检查全部通过。

## 未运行与未验收

- 本机没有 Linux 执行环境；Linux 条件的 **6 项** 权限、symlink/祖先链接、hardlink、权限变化及锁 inode 测试只完成编译/Clippy，尚未在本机运行。现有 Linux CI 的实际状态由 PR 单列，不将交叉编译当成运行。
- 本机未运行完整 Linux workspace、systemd/Root/native audio 回归；它们需要 Linux 平台及相应系统依赖，未声称全绿。
- Windows 后端只继承调用方 ACL，不具备 Linux 的 owner/mode、等价路径替换防护或目录 fsync；Windows 生产私有存储安全未验收。其他平台明确返回 UnsupportedPlatform。
- 文件系统阻挡写入测试不等同于实际磁盘写满或断电；ENOSPC/保存中断的完整故障矩阵留在后批。
- 无真实 NKN/RPC/链上确认、Owner/Core 鉴权/IPC、签到确认/取消状态机、服务进程或实际共享引擎。同步 transport 必须遵守共享单调截止与读取上限；库不能抢占一个不返回的自定义实现。
- 订阅区块参数仅是固定上游参考，不保证 20 秒块间隔或墙钟窗口。ARM 构建/设备运行、网页及真实所有者开启均未验。
- 没有连接设备、NAS、真实 NKN/RPC、Bark、运营商或生产服务；没有交易、外发消息或私有维护目录读取。

## 来源、依赖与 AI 辅助

固定参考 `Viper-Boss/nknguard@c32952d7586676fec10f71a30b22969cb24881f3` 的 LICENSE、USAGE_STATS 文档、usagestats.go、chain_nkn.go、usagestats_test.go、internal/app/usage.go。逐文件完整 blob SHA、实际借鉴逻辑与差异见协议来源表。上游 GNU AGPL v3 与本工作区 `AGPL-3.0-only` 保持来源；Rust 独立实现，没有复制/改名 Go 引擎或导入其私有配置。特别没有沿用 AlwaysEnabled、通信种子派生或损坏后重建的策略。

以下为本包新增的直接引用，全部复用现有 lock 版本；没有引入 TLS、VPN 或 Go 运行时。许可取自校验后的包清单，不表示第三批 SEC-05 的全项目许可/通告审计已完成。

| 依赖/固定版本 | 用途 | 包声明许可 |
| --- | --- | --- |
| serde 1.0.229 / serde_json 1.0.154 | 严格 DTO、私有持久格式 | MIT OR Apache-2.0 |
| ed25519-dalek 2.2.0 | 标准 Ed25519 公钥生成，开启既有 zeroize | BSD-3-Clause |
| getrandom 0.4.3 | 独立随机身份和临时文件名 | MIT OR Apache-2.0 |
| sha2 0.10.9 | 固定产品/目的域的 SHA-256 | MIT OR Apache-2.0 |
| zeroize 1.9.1 | 种子及私有读写缓冲清理 | Apache-2.0 OR MIT |
| fs2 0.4.3 | 真实文件独占锁 | MIT/Apache-2.0 |
| rustix 1.1.5（仅 Linux） | 安全目录句柄/受限文件 API、有效 UID | Apache-2.0 WITH LLVM-exception OR Apache-2.0 OR MIT |
| tempfile 3.27.0（仅测试） | 真实临时目录回归 | MIT OR Apache-2.0 |

由 Codex 辅助实现，含协议、持久化与来源文档的并行子代理；主代理完成跨模块集成、静态审查、真实 Cargo 运行及失败修正。没有将 AI 生成或测试数量当作主线验收。实际有效工时未自报，不估造。

## 直接给主线 Codex 的接入说明

先审核这些有限类型及周期参数，再安排 A-05 等后批。主线需要自行设计经 Owner 当前授权的统计配置命令/查询命令，独立 `enabled` 开关、当前披露版本确认、三个窗口的状态/计数/观察时间/成功时间/到期时间和失败原因；不能把 OFF 或 Unknown 格式化成 0。不要将本批库直接注册成开机服务。

数据目录应为专用非 Root 用户控制的独立 0700 子目录，父目录先由主线准入，不使用身份/SIM/短信目录。只在 Owner 授权后调用 `StatisticsIdentity::generate` 和 `FileStore::create`；另行在受控统计配置中绑定预期统计公钥，重开用 `FileStore::open(path, expected)`。公钥绑定来源不可从未校验 envelope 自行采纳。OFF 只读入口无需创建或打开存储。

查询注入有界只读适配器；本批没有选定真实 Rust NKN SDK。适配器不得把任意 RPC URL/命令透传入库。每次查询后，包括失败和回拨，用 `client.cache().clone()` 更新 `StoredState.cache` 并 `save`；保存失败单独报告，重开核验可能已原子发布的结果。当前库不会自动落盘或重试。

后批提交、查询确认与取消的状态机必须另行审核；本批 `CheckInPreparation::Prepared` 只是本地协议提案，`pending` 字段也只预留持久格式。任何真实签到均需新批准的 transport、持久化顺序、防重及禁用代次约束。回退可移除本库调用而保留独立数据；不得删除文件伪造撤回链上订阅，也不把新格式交给旧实现强行修复。

2026-10-28 交付复核：发布报告时的控制台编码损坏已从保留的原始 UTF-8 提交恢复；修正仅涉及本报告，Rust 实现及测试内容未变。后续使用 ASCII 转义 JSON 传递文本并核对文件摘要，避免再次损坏。

审核结论：**待负责人对 PR 完整 HEAD 审核**。全局原 P0–P7 仍 80%，扩展台账仍 14/169、加权 7.3%；本报告不更新这些台账。


## 2026-11-04 · PR #41 审查 P2 的后续修复

- 用户明确要求修复审查记录中的缓存新鲜度问题，本节只记录该修复，不接管已转交成员B的A-05～30或其签到/NKN实现。
- 旧审查提交为 `295c47655af1d69d2eb0568eda09023c4599c16b`。PR #41 已于2026-10-31合并为 `4459a39259da0220f2cc5856e616656cce519147`；最新main仍存在原问题，因此另建小PR，不重开或修改已合并历史。
- 本次起始main：`5304a96d28f2c76c14ab651938a217dbeae67fa3`；修复代码完整HEAD：`c4a6853245a22ad7a7c7626a3c39c1460dab1943`；分支 `codex/usage-stats-clock-fallback`，最终含本报告的完整HEAD在新PR正文记录。
- 主线已同时保留mdns、ble、usage-stats及其他成员。本次不修改Cargo.toml或Cargo.lock，原workspace冲突不再存在；主线已加入的check_in模块、测试和文档全部保留。

### 原因与行为

transport返回后的时钟读取失败时，fetch报告ClockUnavailable，但query_window把旧high_water_secs作为当前时间传给fallback，仍可能把已经到期的旧值标为WithinTtl。修复仅对ClockUnavailable传None，复用与查询开始即失去时钟相同的保守路径；保留原缓存、原成功/到期时间和ClockUnavailable错误，不变成0或成功。沿用现有Freshness::ClockRollback分类表达无法证明新鲜度，不新增公共枚举或改变持久格式。

新增3项实际QueryClient回归，覆盖三个窗口与transport成功/失败：已有count=42、成功1000、到期1015，查询从1010开始，I/O后时钟不可用；无缓存时保持Unknown；第二页时失去时钟不发布部分计数、不继续请求下一页。合计24种组合；原条目完整保留、计数/时间不重写。负对照在未修复代码上为1通过/2失败，两个失败均实际输出错误的WithinTtl；修复后三项全部通过。未读取审核者私有探针目录。

### 本次实际验证与边界

为避开本机Git CLI的网络/凭据限制，通过已连接GitHub读取固定main的**完整实际统计包**（包括其他成员新增check_in源码与测试），在工作区专用临时目录做单包隔离验证，只覆盖本次两个Rust文件。生产模块不是测试专用替代实现。隔离workspace的包元数据和共享依赖与main一致；裁剪后的56条锁记录逐项核对版本/校验和，全部存在于主线Cargo.lock，未改变仓库依赖。已核对发布文件的Git blob SHA与实际测试文件字节相同。

| 命令（Rust 1.98.0，隔离统计包工作区，离线） | 实际结果 |
| --- | --- |
| cargo test -p rustsimhub-usage-stats --locked --offline | 退出0，Windows实际50项：持久化16、协议13、查询21；无失败/忽略 |
| cargo fmt --all -- --check | 退出0，只验证该隔离包工作区，不冒充全仓格式回归 |
| cargo clippy -p rustsimhub-usage-stats --all-targets --locked --offline -- -D warnings | 退出0，Windows目标 |
| cargo clippy -p rustsimhub-usage-stats --target x86_64-unknown-linux-gnu --all-targets --locked --offline -- -D warnings | 退出0，Linux条件代码及全部测试的严格交叉检查 |
| cargo check -p rustsimhub-usage-stats --target x86_64-unknown-linux-gnu --lib --no-default-features --locked --offline | 退出0，Linux生产库检查 |
| git diff --check | 退出0 |

首次宿主全测曾在默认临时路径的创建/权限检查处遇到PermissionDenied；将TEMP/TMP明确设为工作区内临时目录后通过，没有放宽FileStore检查。此前审查者提供的53项Linux通过属于旧HEAD，不能沿用为本次新HEAD通过。

本机没有可用WSL；6项Linux权限/链接运行测试、Linux限定check_in测试、完整workspace/native audio/systemd/ARM及设备/真实NKN检查**未运行**。交叉检查不等于运行。云端CI以新PR对应HEAD的实际回读为准，未预先标为通过。

无新增依赖或外部实现，无设备、NAS、真实NKN/RPC、交易或消息外发，未读取私有维护目录。未改Core/Edge/Root、身份、开机策略、其他成员实现或全局进度。由Codex完成定点修复和真实检查，等待负责人对新HEAD复审；整体验收沿用主线81.5%、20/169和加权10.6%，本修复不自行计入新验收。
