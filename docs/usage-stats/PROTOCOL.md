# RustSimHub 近期活跃安装统计协议 v1

本目录是成员 A 第一批 A-01～A-04 的离线库契约，供主线审核。统计对象是近期活跃的安装，不是自然人数；尚未接入 Core、HTTP、IPC、设备、真实 NKN SDK 或 RPC。第一批不提交交易，也不启动后台任务。后续实现须先通过本批类型契约审核。

## 固定来源与许可

2026-10-27 通过 GitHub 只读文件接口核对上游 `Viper-Boss/nknguard@c32952d7586676fec10f71a30b22969cb24881f3`。以下是实际阅读文件及 Git blob SHA，不能用上游后续 main 替代本次参照。

| 文件 | Git blob SHA | 借鉴的具体逻辑 | RustSimHub 的处理与差异 |
| --- | --- | --- | --- |
| [LICENSE](https://github.com/Viper-Boss/nknguard/blob/c32952d7586676fec10f71a30b22969cb24881f3/LICENSE) | `0c97efd25b5974b974ed9a8a18207bc4f55bb338` | 核对完整 GNU Affero General Public License version 3 文本 | 沿用 RustSimHub 工作区 `AGPL-3.0-only`；保留本文来源记录，不把 Go 实现改名成 Rust |
| [docs/USAGE_STATS.md](https://github.com/Viper-Boss/nknguard/blob/c32952d7586676fec10f71a30b22969cb24881f3/docs/USAGE_STATS.md) | `22b359fbf4636a77c3feb9caa2aecba28a981b9d` | 三窗口订阅计数、订阅自然到期、公开主题可被任意第三方参与 | 使用 RustSimHub 专用主题；披露假名化关联性，不声称精确人数或不可关联；不采用自动强制参与 |
| [pkg/usagestats/usagestats.go](https://github.com/Viper-Boss/nknguard/blob/c32952d7586676fec10f71a30b22969cb24881f3/pkg/usagestats/usagestats.go) | `349727cc45d30e57bbba49ade7ddc3e0f2fe5cb7` | 24 小时续订、三个独立主题、区块周期参数、成功 0 与未知值区分、查询缓存 | 用有限 Rust 类型及严格有界解析；默认 OFF；独立统计种子；逐窗口持久缓存与回拨保护；损坏状态报错并保留，不静默重置 |
| [pkg/usagestats/chain_nkn.go](https://github.com/Viper-Boss/nknguard/blob/c32952d7586676fec10f71a30b22969cb24881f3/pkg/usagestats/chain_nkn.go) | `4f22b8b5c39017c964b3018d6a38ff8bc42a235c` | 提交与链上订阅确认不是同一事件；查询只需统计主题 | 本批只定义只读查询 transport；不导入 Go、SDK、节点列表或联网逻辑；后续提交/确认接口需要另批审核 |
| [pkg/usagestats/usagestats_test.go](https://github.com/Viper-Boss/nknguard/blob/c32952d7586676fec10f71a30b22969cb24881f3/pkg/usagestats/usagestats_test.go) | `465d95a84f974fc81cf569947b70810db8dbb898` | 模拟 transport 断言默认策略、持久重开、局部失败、真实 0、缓存和未知查询 | 测试调用实际 Rust 组件；补版本/字段/长度/重复页/权限/链接/时间回拨；不移植上游 `AlwaysEnabled` 测试语义 |
| [internal/app/usage.go](https://github.com/Viper-Boss/nknguard/blob/c32952d7586676fec10f71a30b22969cb24881f3/internal/app/usage.go) | `105bea519af044ad818071f7e983a152e9165ab0` | 统计能力缺失应显示不可用 | 不采用 `DefaultEnabled: true`、`AlwaysEnabled: true`，不读取通信 keystore；没有 SDK 时不假装联网成功 |

本批借鉴的是上表列明的协议思路、参数和失败场景，Rust 源码独立编写。没有复制上游服务、运行时或应用接线；来源和许可证不因语言变化而省略。

## 窗口、主题与周期

协议版本 `PROTOCOL_VERSION = 1`。`Window` 只允许三个取值。各主题是 RustSimHub 的独立产品主题，不能使用 `nknguard.usage.*`、`modemdecknet*` 或其他产品主题。

| 窗口 | 产品主题 | 窗口目标秒数 | 订阅区块数 |
| --- | --- | ---: | ---: |
| 24 小时 | `rustsimhub-usage-v1-24h` | 86,400 | 8,640 |
| 30 天 | `rustsimhub-usage-v1-30d` | 2,592,000 | 133,920 |
| 90 天 | `rustsimhub-usage-v1-90d` | 7,776,000 | 393,120 |

`MAX_SUBSCRIPTION_BLOCKS = 400000` 和 `TARGET_BLOCK_TIME_SECS = 20` 是从固定上游读取的参照参数，不是本批通过真实链核验的运行保证。区块高度是链的过期依据；20 秒是目标区块间隔，不能把区块数换算成可靠墙钟保证。因此界面应表达“24 小时 / 30 天 / 90 天目标窗口的近期活跃安装估计”，不能宣称精确的最近时段人数或保证到秒过期。

| 常量 | 秒 | 第一批含义 |
| --- | ---: | --- |
| `RENEW_INTERVAL_SECS` | 86,400 | 签到意图的续订周期契约；未实现续订调度器 |
| `CHECK_EVERY_SECS` | 3,600 | 后续调度参考参数；本批不会按此启动任务 |
| `RETRY_AFTER_SECS` | 300 | 后续重试参考参数；本批不会自动重试提交 |
| `CACHE_TTL_SECS` | 600 | 单次响应观察时间至过期时间的最大间隔；缓存沿用响应到期时间 |
| `MIN_REFRESH_SECS` | 5 | 同一窗口两次显式查询开始时间的最短间隔 |
| `QUERY_TIMEOUT_SECS` | 10 | 只读 transport 的超时预算契约；适配器须执行该预算 |

上游适配器另有 20 秒轮询、15 次确认检查和 10,000 毫秒 RPC 超时；仅记录来源，不在本批实现或承诺为 RustSimHub 确认预算。真实 NKN 的订阅上限、交易费用、过期高度和 SDK 行为在后续接入时仍需重新核对。

## 公开输入与时间

`EnabledPolicy` 缺失或默认构造均为 OFF。所有者同意与当前披露确认必须同时满足，才允许产生无副作用的 `CheckInIntent`；OFF 时意图为 `None`。`prepare_check_in` 的命名结果 `CheckInResult` 为 `Result<CheckInPreparation, ProtocolError>`，只可能返回 `Disabled` 或 `Prepared { intent }`，不提供假提交或假确认结果。意图仅携带协议版本、独立统计公钥、固定产品主题、窗口、订阅区块数和续订周期，不携带种子或设备业务身份。构造意图不是创建队列、保存 Prepared 状态、签名、提交或确认交易。

`Clock` 是显式可注入的 Unix 秒来源，生产调用方可使用 `SystemClock`；无法取得有效时间必须报错。`renewal_period` 按 Unix 秒除以 86,400 得到，仅为本地协议周期编号，不是链上确认时间。区块高度不能直接替代本地缓存时间，墙钟不能代替链上到期高度。

解析入口先检查字节数，再解析完整 JSON 并校验所有约束；未知版本、错产品主题、无效窗口、错误周期/区块数、重复/多余字段、尾随内容和超大输入均拒绝。错误只返回有限分类，不回显原始输入。真实 RPC 的响应必须由后续适配器转换成本文的有限查询契约，调用者不能从响应取得任意 URL、命令或外部密钥路径。

`EnabledPolicy::parse(&[u8])` 的输入上限为 512 字节；`{}` 为 OFF。开启配置必须完整表达为 `{"enabled":true,"owner_consent":true,"disclosure_version":1}`；缺少同意、过期披露版本或相互矛盾的字段均失败。结构化调用为 `EnabledPolicy::from_owner_consent(OwnerConsent::Granted, DisclosureAcknowledgement::Current)`，只有主线确认真实 Owner 的当前操作后才应传入这两个值。

`CheckInIntent::parse(&[u8])` 与 `to_json()` 的公开载荷上限为 1,024 字节，且只包含以下六个字段。直接使用通用 serde 时仍须由上层实施同一字节上限；对不可信字节优先调用有界 `parse` 入口。

| 字段 | v1 约束 |
| --- | --- |
| `version` | 整数 `1` |
| `public_key` | 32 字节统计公钥的小写 64 位十六进制；不含种子 |
| `topic` | 与 `window` 精确对应的固定主题，最多 64 字节 |
| `window` | 字符串 `24h`、`30d` 或 `90d` |
| `subscription_blocks` | 与窗口精确对应且不超过 400,000 的区块数 |
| `renewal_period` | `Unix 秒 / 86400` 的 `u64` 周期编号；下一周期换算不能溢出 |

`ProtocolError` 将输入超限、畸形输入、未知版本、错主题、无效周期、缺少披露确认、无效策略和时钟失败分开。`ClockError` 区分 Unix 纪元前时间与时间不可用。它们不携带未受信任原文或解析器的输入片段。任意无效窗口字符串及多余/重复字段归为畸形输入，而不是默默选择默认窗口。

## 只读查询适配契约

`QueryTransport::query_page(&QueryRequest)` 只返回一页 `Result<Vec<u8>, TransportError>`，没有签名、提交或取消交易的方法。`QueryRequest` 的只读 getter 给出 `version`、`window`、`topic`、预期 `page`、`max_bytes` 和剩余 `timeout_secs`；请求不接受 URL、程序、设备路径或密钥。`TransportError` 仅为 `Offline`、`Timeout`、`Rejected` 和 `Unavailable`。

响应是本库的有限适配协议，**不是宣称真实 NKN RPC 已原生提供的格式**。后续 SDK/RPC 适配器须明确转换来源与时间：

| 响应字段 | 约束 |
| --- | --- |
| `version` / `window` / `topic` | 与请求的 v1 窗口和产品主题精确匹配 |
| `page` / `pages` | 从 0 连续请求；页号须等于预期；总页数为 1～16 |
| `count` | 无符号 `u64`，各页安全求和，总值不超过 1,000,000,000 |
| `observed_at_secs` | 不晚于本地完成该页查询的 Unix 秒 |
| `expires_at_secs` | 晚于观察及本地完成时间，与观察时间间隔不超过 600 秒 |

单页上限 `MAX_PAGE_BYTES = 2048`，总页数 `MAX_PAGES = 16`；所有页面的总页数、观察时间和到期时间必须一致。重复/错序页号、重复字段、畸形 JSON、多余字段、页数变化、时间变化、溢出及超大计数均失败。相同计数值可以出现在不同合法页，本契约按页号判重，不从计数值推断订阅者身份。

每个窗口查询同时检查注入的 Unix 秒来源和进程内 `Instant` 的 10 秒总预算；所有页面共享 `QueryRequest::deadline()` 给出的同一单调截止时间，`timeout_secs` 是两种剩余预算较小值的秒级上取整提示。库在调用前后检查预算，墙钟冻结不会延长单调截止。库采用同步调用，返回后再次检查字节数、时间、版本及内容。**transport 适配器必须在实际 I/O 期间限制读取字节并执行共享单调截止；本库不能抢占一个不返回或无限分配内存的恶意 transport**。本批没有联网适配器，测试使用内存 transport 与合成响应，包含冻结墙钟下真实等待单调截止的测试。

## 第一批范围

查询结果区分成功查询的 0、失败未知和可用旧缓存，详见 [STATE-MACHINE.md](STATE-MACHINE.md)。公开字段与本地种子的边界见 [PRIVACY.md](PRIVACY.md)。主线必须审核类型后再增加提交、确认、取消和服务接线。
