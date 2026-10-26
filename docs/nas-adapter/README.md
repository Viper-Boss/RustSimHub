# NAS 只读适配契约 · 成员 C 第一批

范围：C-01～C-04 / NAS-01 的离线部分。基线为主线
`ffbb72d05f4f865b501d7ab021fe62996dbe8c6c`。本目录说明的是实际
`adapter_api=1 / data_format=2` 读取契约，不是旧 NAS protocol 1.5 的完整响应实现。
对应实现为 `tools/nas-adapter/` 的 Go 客户端；运行不会开放监听或启用设备兼容服务。

## 路径和动作支持矩阵

“源码支持”“旧页面可用”“实机验收”分别记录；本次仅使用本地合成数据。

| 路径 / 动作 | 实际支持与授权 | 旧 NAS 适配 / 未验收边界 |
|---|---|---|
| `POST /v1/session` | 已有：独立 NAS principal、Owner 预先批准的映射、旧签名及设备绑定证明，返回专用短期会话 | 不是旧配对或 Owner 登录；本批不变更签名/期限，实际设备撤销/重启由主线补验 |
| `POST /compat/v1/read`，`entity=status` | 已有正式读取；本批类型校验和投影 `LineView`；需 `status.read` | 旧状态页需适配；不提供运营商注册结论、网络健康或 SIM 原始标识 |
| 同路径，`entity=sms` | 已有正式读取；本批校验 `SmsPage`；需 `sms.read` | 保留稳定 ID、线路代次、预览与游标；旧收件箱页面尚未对接 |
| 同路径，`entity=sms_detail` | 已有正式读取；本批校验 `SmsDetail` 及请求 ID 对应；需 `sms.read` | 保留正文/字节数组；旧详情页面尚未对接 |
| 同路径，`entity=calls` | 已有正式读取；本批校验 `CallPage`；需 `calls.read` | 当前观察视图无通话时间，通常无号码；不能构造旧通话历史、控制或媒体 |
| `GET /v1/health`、`/v1/status`、`/v1/modem`、`/v1/capabilities` | compat 的 `endpoint_scope` 只有 scope 分类；Edge 没有这些旧响应实现 | 不能因 scope 函数支持而声称 HTTP 路径已实现；应适配正式读取 |
| `GET /v1/sms`、`GET /v1/calls` | 同上：scope 分类不等于 Edge 实现 | 旧页面需改用正式读取及明确的数据格式 |
| `POST /v1/pair`、旧配对/信任接管 | 不支持；不得把 Owner 或普通客户端当作 NAS | 由现有 v2 授权流程预先批准 NAS，本工具不提供配对入口 |
| 发送/删除短信、拨号/接听/挂断/DTMF | 本桥不支持；客户端拒绝四类读取以外的 entity | 不增加写入、重试或硬件控制路径 |
| 旧 PCM、PQ、KCP、旧更新、其他 `/v1/*` | 未实现；Edge 的旧路径兜底为 501 | 不把 HTTP 可达或签名向量通过当作旧页面、双向声音兼容 |
| 历史读取/导入 | 既有独立 archive、授权和迁移实现；本桥会话不自动获得历史权限 | C-05～C-08 属第二批，须第一批契约审核后开展 |

准确的设备启用和绑定说明见 [NAS 在线接入桥](../nas-online-bridge.md)。
本次不运行其中的服务启用命令，不改变默认关闭配置。

## 字段映射

响应仍是 `{adapter_api:1,data_format:2,data:...}`。模型由正式 Rust 结构投影，
不引入旧 NAS 字段别名。所有 JSON 对象严格区分字段大小写，拒绝未知字段、重复键、
类型错误、无效 UTF-8、多个根值、超过 16 层嵌套和超过 65536 字节的响应。
新增上游字段须审核本投影后接纳，不能盲目透传潜在凭据/硬件信息。

| 字段 | 权威来源 / 类型 | 缺值、保留和边界 |
|---|---|---|
| `adapter_api` / `data_format` | `core::compat_api::dispatch`，整数 1 / 2 | 必填；其他整数版本为 `version_unsupported`，缺失/错误类型为 `invalid_reply` |
| 查询 `entity` | `domain::NasEntity`，四个有限字符串 | 必填；不接受任意 URL、控制命令或 history entity |
| 查询 `limit` | Edge/Wire 的兼容读取，整数 1～16 | 必填；不自动调整超限值 |
| 查询 `record_id` | 短信详情稳定 UUID 字符串 | 仅详情必填；其他读取须为空/省略，不使用原 ModemManager 路径 |
| 查询 `after` | `domain::InboxCursor` 对象或 null | 只用于短信/通话列表；初次省略/null；不在桥内持久化 |
| `data.line` / 记录 `line` | `storage::LineView`、`SmsSummary`、`CallSummary` 的 `LineEpoch(u64)` | 必填整数，允许 0，限制为存储可表示的 i64 范围；Go 用 uint64，保留超过 JavaScript 安全整数范围的代次，不转浮点 |
| 状态 `data.status` | `storage::LineStatus` 的 `ready/absent/locked/observing/unavailable` | 必填，未知状态拒绝；含义见下表，不能推断注册或连通性 |
| `messages[]` / `calls[]` | `storage::InboxPage` / `CallPage`，数组 | 必填，可为空；null、缺失或多于请求 limit 拒绝；单页重复 ID 拒绝 |
| 短信/通话 `id` | Core 存储的稳定 UUID | 必填，原样保留；详情 `summary.id` 必须与请求相同；不是电话号码或 SIM 标识 |
| `binary` | `SmsSummary`，布尔值 | 必填；缺失/null 不可默认为 false；详情须与字节数组是否非空一致 |
| `duplicate_candidate` | `SmsSummary`，可选 UUID/null | 保留候选关系，不将相同正文短信合并；省略时输出 null，不造候选 ID |
| `display` | `SmsSummary` 的可选 `SmsDisplay` | 无值时不补造预览；对象存在时三个字符串均必填 |
| `display.sender/timestamp/preview` | `storage::list_summary`，字符串 | 上游分别截取 64/48/96 个字符；原样保留空串/文本，不解析成 HTML 或猜时间 |
| 详情 `summary` | `SmsDetail.summary`，同一 `SmsSummary` 结构 | 必填；保持 ID 和线路代次；不与当前线路强制相等，避免把旧线路记录改为当前记录 |
| 详情 `sender/timestamp/text` | `SmsDetail`，字符串 | 必填，可为空表示上游未知；缺失/null 拒绝，不生成号码/时间或空正文替代错误 |
| 详情 `data` | Rust `Vec<u8>` 的 JSON 整数数组 | 必填，可为空；只允许 0～255，输出仍是数组，不变为 Go `[]byte` 的 Base64 字符串 |
| `direction/modem_state/reason` | `CallSummary`，i32 整数 | 必填，保留未知底层数值；不猜拨入拨出或失败语义 |
| `state` | `domain::CallState` | `outgoing_pending/dialing/ringing_out/ringing_in/active/held/waiting/ended/reconciling`；未知字符串拒绝 |
| `availability` | `storage::calls::read_summary`，`observed/reconciling` | 必填；reconciling 不是已结束或设备离线 |
| `media_state` | 当前 `CallSummary` 的 `none` | 必填；不能据此宣称媒体连接或声音可用；其他值须重新审契约 |
| `number` | `CallSummary.number`，可选字符串/null | 当前正式只读视图通常不返回；缺失保持未知，不填本机号码或空号码 |
| 列表 `cursor.db_epoch` | `domain::InboxCursor`，数据库 UUID | 必填；响应和请求 epoch 不同为 `resync_required`，禁止跨库推进 |
| 列表 `cursor.seq` | 同上，十进制字符串 | 必填，0～i64::MAX；不转浮点；非空页必须前进，空页保持原位置 |

不存在的旧字段（原始 IMSI/ICCID、硬件路径、注册详情、通话起止时间等）不合成。
授权的短信正文/发件人属于业务返回，不能写入公开交付日志。

## 状态与错误

| 类型 | 表示 | 不能推断 |
|---|---|---|
| `ready` | Core 线路状态就绪 | 不保证互联网、运营商注册、电话或 NAS 页面可用 |
| `absent` / `locked` | SIM 缺失 / 锁定 | 不等于设备不可达 |
| `observing` | 正在观察线路 | 不等于已完成通信验收；也不用于推断 Core 启动模式 |
| `unavailable` | 线路观察不可用 | 不等于 HTTP、NAS 网络或 SIM 注册必然失败 |
| `transport_failed` | DNS/TCP/TLS/超时或被禁止的重定向 | 不转换为 SIM 离线或空列表，不自动重试 |
| `unauthorized` / `forbidden` | HTTP 401 / 403 | 保留 HTTP 数值，不自动登录/换 Owner 凭据 |
| `compat_disabled` | HTTP 501 且正式错误 `legacy_disabled` | 不自动开启兼容服务 |
| `not_implemented` | 其他 HTTP 501 | 不当作临时掉线；不得静默降级协议 |
| `backend_unavailable` | HTTP 502/503/504 | 业务后端暂不可用，数据未知 |
| `busy` | HTTP 429 | 不自动重试 |
| `http_error` | 其他非 200 状态，包括不可重放的 307/308 | 保留 HTTP 数值；不输出远端任意错误字符串 |
| `version_unsupported` | 正式响应版本非 1/2 | 不用旧格式/错误类型继续投影 |
| `invalid_query` / `invalid_reply` | 输入/响应类型、范围或结构非法 | 不发布部分数据或新游标 |
| `resync_required` | 响应数据库 epoch 与输入游标不一致 | 必须由调用方重新开始读取，不继承旧检查点 |

CLI 失败返回非零退出码，stderr 只含本地有限标签（以及 HTTP 数值）。
不回显任意错误正文、seed、token、证书路径或响应业务内容。
`session` 是唯一会输出短期 token 的显式命令；`read` 输出没有凭据字段。

## 分页、重放与调用方集成

本组件是无存储的单次读取器：输入游标只读；成功前没有输出数据；
失败时不返回部分页或新游标。相同请求和相同页得到相同稳定 ID/代次，不创建任何业务记录。
分页失败后调用方继续使用上次已确认游标；只有整页消费成功才提交新游标。
NAS 接收方如要把结果保存到自己的数据库，应以 `(db_epoch, entity, id)` 幂等更新，
在同一事务内保存页与游标；不能把列表查看当作短信发送、删除或通话动作。
这项接收方持久化尚不在本桥中实现，也没有冒充旧 NAS 页面已经完成幂等集成。

TLS 保留配置 CA/主机名校验、最低 TLS 1.3、固定设备公钥/绑定 epoch 的会话证明；
不使用代理、Cookie jar、重定向或自动请求重试。不改变 NAS principal/scope/期限，
不拿只读 token 请求 v2 或 Owner 接口。新模块只用 Go 标准库，无新增 Go module 或 Cargo 依赖。

## 验证与交付

从仓库根目录执行（Go 模块模式关闭是因为原桥本就没有 go.mod）：

```sh
export GO111MODULE=off
export RSH_GOLDEN_FILE="$PWD/crates/compat/src/legacy-golden.json"
go test -count=1 -v ./tools/nas-adapter
go vet ./tools/nas-adapter
CGO_ENABLED=0 go build -o /tmp/rustsimhub-nas-adapter ./tools/nas-adapter
cargo test -p rustsimhub-compat --locked
cargo clippy -p rustsimhub-compat --all-targets --locked -- -D warnings
cargo fmt --all -- --check
```

Linux 的 `TestCLISyntheticConfigAndRead` 使用真实 `runCLI`、0700/0600 临时配置和
种子、合成 TLS 回环响应，验证会话绑定及四类读取，不连接设备或启动网关。
同一夹具还构建并执行真实 CLI 程序，验证 stdin/输出和进程退出行为。
Linux 的 compat 测试增加一个 Go 互操作入口（因此要求 PATH 上有 Go），
禁用 Go 模块下载并检查 Linux CLI 夹具确实执行；未修改仓库 CI 工作流。
其他平台运行纯模型、黄金向量及 TLS 测试，CLI 明确拒绝读取凭据，不降低权限检查。
交叉编译通过只证明可编译，不能代替执行此 Linux 测试。

真实执行结果和未运行项见 [成员 C 交付报告](../deliveries/member-c-nas.md)。
第一批审核后才继续 C-05～C-08。主线仍需提供固定版本的旧 NAS 源码/页面契约，
补验原页面列表/详情/错误显示及真实设备授权、撤销/重启、控制和媒体；
本地合成 HTTP 通过不表示这些项目完成。

## 来源与许可

基于本仓库上述固定 main 的 Rust 权威结构与原有 Go 适配器独立补充。
原 Go 客户端仍明确标为 Go，签名与 device proof 算法未更改。
`crates/compat/src/legacy-golden.json` 的原始 Go 向量来源修订为
`ea019485e5d890a96acae357c52b94c746de837f`，沿用原说明和测试。
未复制缺失的旧 NAS 应用源码或前端；新增代码保留 `AGPL-3.0-only` SPDX 声明。
