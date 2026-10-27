# 成员 C · NAS 兼容与历史离线交付

本报告按日期追加交付；当前 C-07/C-08 见文末及 [PR #9](https://github.com/Viper-Boss/RustSimHub/pull/9)。第一批、C-05 和 C-06 各节保留提交时的历史快照；PR #3、#6、#7 后续已由负责人合并。当前批次的实现、测试和审核状态以本批节及对应完整 HEAD 为准。

当前追加 C-09～C-12 备份恢复离线包见文末；C-07/C-08 的 PR #9 已由负责人合并，各前批节的待验描述保留为当时提交截面。

日期：2026-10-26（Asia/Shanghai）。成员：yh1030 / member-c。
任务：C-01～C-04，对应 NAS-01 的契约盘点和只读桥离线范围。

## 提交截面与当前状态

- 起始 main：`ffbb72d05f4f865b501d7ab021fe62996dbe8c6c`。
- Linux 全部检查通过的完整代码 HEAD：`6cee2ed4e5ee2e599bacaee8f52f63c62b26358d`。
- 对应代码/文档树：`848b745a51e7efc041a061073c847909a1bcb860`。本次最终证据补记只修改本报告，当前最终 PR HEAD 另在 PR 正文记录。
- 首版原仓库代码 HEAD：`46ee6b20e0c34482c3a89e00731e496e1bb58a24`；与首次本地验证提交 `be44659b953a4e31b663dd76a3618e5936d0fe06` 的树 `61894dbb02dac8dcae54d205e3a10ab3fe9b707d` 相同，后续仅调整测试进程隔离及文档。
- 分支：`codex/nas-readonly-history`。
- 提交方式：用户传达负责人最新安排，直接使用原仓库 `Viper-Boss/RustSimHub` 的独立开发分支向 main 提交第一批 PR，不使用 fork，不直接推送 main。
- 已创建原仓库 [PR #3](https://github.com/Viper-Boss/RustSimHub/pull/3)。第一批实现、离线测试和交付材料完成，最终证据提交后转为可审核；主线逐 HEAD 的契约审核仍未完成，不能称已验收或已合并。
- 本报告另作提交，当前最终 PR HEAD 应在 PR 正文逐次更新，不能把旧代码 HEAD 当作新增提交后的审核截面。
- 第二批 C-05～C-08 未开始，等待第一批契约获得主线审核。

## 问题与修改后行为

原 Go 桥只检查成功响应是合法 JSON，然后整体输出；读取查询也没有有限类型校验，
任意远端错误字符串可能直接进入 stderr。现在查询只允许四类读取，响应按
`adapter_api=1 / data_format=2` 的权威结构解码和重新序列化；错误区分线路状态、
传输、HTTP、鉴权、兼容关闭、未实现、后端暂不可用和不支持版本。
缺失的号码/时间不合成，未知字段、潜在凭据字段、重复键、错误类型和超限数据拒绝，
错误不回显任意远端文字。列表/详情保留稳定 ID 与线路代次，字节数组不变为 Base64。

| 任务 | 交付证据 | 验收边界 |
|---|---|---|
| C-01 实际支持盘点 | [支持矩阵和字段映射](../nas-adapter/README.md)：实际 Edge 路径与 scope 分类分开，字段标注来源、类型、缺值、ID、线路和游标 | 没有固定旧 NAS 页面源码；不猜旧字段，不宣称页面集成 |
| C-02 状态和错误模型 | `readonly.go` 的有限状态、类型/版本/大小检查；`http.go` 的本地有限错误类别 | 不从线路 ready 推断互联网或运营商注册，不输出远端错误原文 |
| C-03 四类只读组件 | `readInput` → `readOnce` → `decodeRead` → 真实 CLI；本地模型/回环 HTTPS 测试 | 不创建记录、不持久化或推进游标；接收方须原子保存页和游标、按稳定 ID 幂等保存；旧页面尚未接线 |
| C-04 测试和 PR 交付 | 下方真实命令/退出码、Linux 实际 CLI 及完整 CI，原仓库 PR #3 | 实现和离线交付完成；真实 NAS/设备、旧页面及主线审核另验 |

## 修改文件与依赖

- `crates/compat/src/migrate.rs`：仅在独立二进制测试进程注册 Linux Go 互操作模块，不改迁移、签名、授权或版本协商。
- `crates/compat/src/nas_adapter_tests.rs`：Linux compat 测试调用真实 Go 夹具，检查 CLI 夹具确实执行。
- `tools/nas-adapter/main.go`：真实 CLI 使用只读类型和有限错误，保留会话签名字节和设备绑定证明。
- `tools/nas-adapter/readonly.go`：输入/响应类型、版本、结构、分页和 ID 对应校验。
- `tools/nas-adapter/http.go`：保留严格 TLS 1.3、无代理/重定向/自动重试的有界单次请求。
- `tools/nas-adapter/private_linux.go`：原 Linux 所有权/权限/链接检查移出 main；读取后再核对大小，防止读期间增长越界。
- `tools/nas-adapter/private_other.go`：非 Linux 明确拒绝凭据文件；仅允许运行纯模型/向量/回环 TLS 测试。
- `tools/nas-adapter/{readonly_test,http_test,cli_linux_test}.go`：合成契约、错误、分页、TLS 及 Linux 真实命令行测试。
- `tools/nas-adapter/README.md`、`docs/nas-adapter/README.md`：构建方法、支持矩阵、逐字段契约、集成和补验边界。
- 本报告。

新增 Go 代码只使用标准库，没有 go.mod、第三方 Go 依赖、Cargo 依赖或 Cargo.lock 变更。
Linux 的 compat 测试新增 Go 工具链这一测试前提（PATH 上需有 Go）；原因是原桥就是 Go，
必须运行真实桥及其 CLI，不能用 Rust 内的替代实现声称互操作通过。
测试设置 `GO111MODULE=off / GOTOOLCHAIN=local / GOPROXY=off / GOSUMDB=off / CGO_ENABLED=0`，
不下载模块；Go 夹具执行有 60 秒截止，CLI 只访问临时回环 HTTPS 服务。
未修改 CI 工作流、Core/wire/storage、默认配置、服务单元、全局进度或其他成员任务。

## 实际本地验证

环境：Windows amd64；Rust/Cargo 1.98.0；官方便携 Go 1.27.3 位于忽略的 `.local/toolchains/`。
下表的 `go`/`gofmt` 实际使用该目录的 `.exe`，不是系统安装。
所有检查从仓库根目录运行，日志和产物仅保存在忽略的 `.local/`。
Go 环境同上，另设 `RSH_GOLDEN_FILE` 为本仓库黄金向量的绝对路径，缓存也位于 `.local/`。

| 实际命令 / 环境 | 退出码 | 结果 |
|---|---:|---|
| `go test -count=1 -v ./tools/nas-adapter`，Windows | 0 | 14 个顶层测试通过，含原有两项签名测试；覆盖状态、未知/缺值、401/403/501、错误 JSON、分页/详情、超限、TLS 信任/最低版本、302/307 禁止跳转、无代理和无自动重试 |
| `go vet ./tools/nas-adapter`，Windows | 0 | 通过 |
| `GOOS=linux GOARCH=amd64 go test -c -o .local/nas-adapter-linux.test ./tools/nas-adapter` | 0 | 包含 Linux CLI 夹具的测试程序编译通过；没有在 Windows 执行它 |
| `GOOS=linux GOARCH=amd64 go vet ./tools/nas-adapter` | 0 | Linux 专用文件也完成静态检查 |
| `GOOS=linux GOARCH=arm64 go build -o .local/rustsimhub-nas-adapter-linux-arm64 ./tools/nas-adapter` | 0 | ARM64 客户端编译通过；未上设备执行 |
| `go build -o .local/nas-adapter.exe ./tools/nas-adapter`，Windows | 0 | 通过 |
| Windows 实际程序 `nas-adapter.exe read`，参数为临时合成配置，stdin 为 `{}` | 1（预期） | stdout 为空，stderr 为 `linux_private_files_required`；未降级凭据检查、未发送请求 |
| `cargo test -p rustsimhub-compat --locked --offline` | 0 | Windows 的 5 项规则/签名/黄金向量测试通过；Linux history/session 和新增 Go 互操作入口未在 Windows 执行 |
| `cargo clippy -p rustsimhub-compat --all-targets --locked --offline -- -D warnings` | 0 | Windows 严格检查通过 |
| `cargo fmt --all -- --check` | 0 | 通过 |
| `gofmt -l tools/nas-adapter` | 0 | 无未格式化文件输出 |
| `git diff --check` | 0 | 通过 |

首轮本地结果来自 `be44659b953a4e31b663dd76a3618e5936d0fe06`（Go 源码与最终代码 HEAD 一致）；测试隔离后的 Rust 复验及 Linux 对应 HEAD 的证据另列如下，不将交叉编译写成运行通过。

## Linux 实际运行证据

完整代码 HEAD `6cee2ed4e5ee2e599bacaee8f52f63c62b26358d` 的
[GitHub Actions 40336473537](https://github.com/Viper-Boss/RustSimHub/actions/runs/40336473537)
两个任务 `test` 与 `native-audio` 均为 success。

- `cargo test --workspace --all-features --locked`：退出码 0。compat 库 19 项全部通过；独立 migrate 测试进程的 `nas_adapter_tests::go_nas_readonly_contract_and_synthetic_cli` 实际通过，内部运行全部 Go 夹具并强制核实 Linux CLI 夹具未被跳过。
- Linux Go 夹具包含正式契约的临时 TLS 服务、0700/0600 合成配置/seed、会话签名/设备绑定、四类读取，并真正构建和启动 CLI，验证 stdin、stdout、拒绝公开配置的进程退出码；没有替代凭据检查实现或访问设备。
- `cargo clippy --workspace --all-targets --all-features --locked -- -D warnings`：退出码 0；覆盖 Linux 专用源码及新增测试入口。
- `cargo fmt --all -- --check`：退出码 0；后续服务、材料、恢复和生产特性步骤也全部通过。
- 原生音频测试与严格检查任务 success，是现有 CI 的回归证据，不扩大本次 NAS/媒体支持声明。
- 测试隔离提交同步到本地后，Windows `cargo test -p rustsimhub-compat --locked --offline`（5 项）和该包严格 Clippy 再次退出码 0。本地 HEAD 与发布 HEAD 按原始 Git 对象 SHA 和文件树核对一致。

最终仅文档的证据提交及其 CI 状态在 PR 正文维护，不能把本段代码 HEAD 的结果冒充其他 HEAD 的结果。

## 保留的失败与未运行项

- 初次 Cargo 离线执行因未缓存依赖失败；下载仓库锁定依赖后实际测试通过，未改锁文件。
- 首轮回环 TLS 测试被受限执行环境阻止；允许本机回环后重跑。未通过关闭证书校验解决。
- 首轮 307 测试预期为传输错误，实际因请求体无 GetBody 而直接返回 HTTP 307。
  修正为保留 HTTP 错误并断言未访问目标，另补 302 的 CheckRedirect 拒绝测试；最终均通过。
- 新增 Rust 测试文件首轮 fmt 检查有差异，格式化后最终检查通过。
- 本机没有 Linux/WSL 发行版，本地没有执行 Linux 私有文件检查、CLI 正向夹具、history/session 或该平台 Clippy。交叉编译和 Linux vet 已完成；后续 Linux 证据来自实际 GitHub Actions。
- 首轮 CI [40335570789](https://github.com/Viper-Boss/RustSimHub/actions/runs/40335570789)，完整 HEAD `9585946cd7ffbb6fc00406b1925306c82c907721`：Linux `nas_adapter_tests::go_nas_readonly_contract_and_synthetic_cli` 已实际通过（内部强制检查真实 CLI 夹具运行），会话和签名测试通过；compat 18/20，通过项不冒充全包通过。两项原历史测试 `grants_survive_normal_restart_but_never_changed_identity_realm_or_archive` 和 `two_nas_namespaces_never_merge_identical_legacy_ids` 失败，其中后者明确为 Busy。
- 新增 Go 子进程原先与库内并行的历史/SQLite 锁夹具同处一个测试进程，现改放到独立 migrate 二进制测试进程，避免潜在描述符继承干扰；不放宽历史锁、权限或原断言。隔离后的对应 HEAD CI 实际全部通过；未把一次通过夸大为已完成更广泛的锁/故障稳定性验收。
- 此前按默认 fork 流程尝试发布，GitHub 返回 403 `Resource not accessible by integration`。
  之后用户明确传达负责人要求直接在原仓库提 PR；原仓库独立分支创建已成功，不再要求个人 fork 授权。
- 没有真实 NAS/设备、旧页面、实际撤销/重启、控制或媒体测试；这些留给主线，不冒充完成。

## 主线集成、风险与回退

1. 针对 PR 正文的当前完整 HEAD 审查范围、支持矩阵、错误与字段契约；新增提交后重新核对。
2. Linux 准备现有 Rust 工具链与 Go，运行 compat 测试、严格 Clippy、fmt 和上面的 Go 测试/vet。
   Linux CLI 夹具必须实际执行；若缺 Go 或夹具失败，互操作测试失败，不默默跳过。
3. 本桥无新增监听、迁移、数据库写入或默认配置变更。集成后仍由主线决定是否启用任何真实设备兼容服务。
4. 严格字段投影会拒绝上游新增字段；需要扩展时先审契约，不自动透传。构建已拆文件，须构建目录，不能只传 main.go。
5. NAS 接收方持久分页仍须使用 `(db_epoch, entity, id)` 幂等写入及页/游标原子事务；本批无旧 NAS 数据库写入，不能宣称该页面集成已经实现。
6. 需要回退时，回退本第一批 PR 的提交即可；没有数据迁移或硬件状态需要恢复。
7. 主线提供固定旧 NAS 版本后，再验证旧状态/收件箱/详情/通话页面的字段与错误显示。
   真实 NAS 身份撤销/重启、权限变化和实际设备行为另验；PCM、电话控制及第二批历史工作不在本次验收内。

## 来源、许可与 AI 辅助

复用本仓库固定基线的权威模型和原有 Go 桥，保留 Go 的真实语言与来源。
原签名黄金向量来源为 `ea019485e5d890a96acae357c52b94c746de837f`，未改向量和签名算法。
未复制缺失的旧 NAS 前端或服务源码；新增源文件使用 `AGPL-3.0-only` SPDX 声明。
本轮由 Codex 辅助阅读、实现、测试和整理；用户个人检查和主线审核尚未完成，不代为声称已经人工验收。
未估造实际有效工时，也未修改作者来增加贡献数字。

## 声明与审核门槛

本批没有访问生产设备、使用真实通信数据或发起外部消息/链上交易；
只使用合成夹具、本地回环服务与临时文件。GitHub 仓库读取/分支发布属于交付操作，
不等于向通信设备发送业务操作。

整体进度沿用主线固定量表约 **80%**，本批未经过主线逐 HEAD 验收，不调整全局台账。
审核结论：待审核。第一批通过前不开展第二批。

## 2026-10-27 · C-05 独立交付

本轮仅执行用户指定的 C-05 / NAS-05“导入前差异报告”，不计入 C-06～08 或追加任务的完成。
第一批门槛已由 PR #3 对完整 HEAD `751e4a964e14db0142b34b3dc4afa245971bb54c` 的负责人批准和合并满足。

- 起始最新 main：`6f964ceb53a5d1effba9bf79748b327b7375b353`。
- 功能分支：原仓库 `Viper-Boss/RustSimHub:codex/nas-history-preflight`，提交 PR 到 main，不 fork、不直接推 main。
- 当前完整 HEAD、PR 链接和对应 CI 状态在 PR 正文记录；没有借用第一批 HEAD 的测试结论。
- C-05 实现与离线验证完成，原仓库 [PR #6](https://github.com/Viper-Boss/RustSimHub/pull/6) 提交审核；负责人验收、真实 NAS 与后续任务不计完成。
- Linux 完整 CI 通过的代码 HEAD：`ac272e7d5addfdf6d2f36e532dab55ccb4548db5`，代码树 `6248f4307d2f3075f040cad6cb1715477259d98e`。本次最终证据补记只修改本报告，最终提交 HEAD 及其 CI 状态在 PR 正文单独维护。

### 实现与要求映射

| C-05 要求 | 本次实现与证据 |
|---|---|
| 复用已有迁移，报告格式/版本、记录类型/数量、缺字段、冲突和拒绝原因 | `history::preflight::preflight` 复用既有私有文件/只读 SQLite、静态 CONTRACTS、记录 canonical 解码、hash 和归档检查；新增 `rustsimhub-migrate preflight`。报告含实际 SQLite app/version、未知产品版本、静态表计数、缺列/缺值、新增/已存在/重复/冲突/拒绝/未比较统计及有限原因 |
| 合成短信/通话/联系人，未实现类型明确不支持 | 使用原独立合成 test-schema 和临时真实 SQLite；未知表/视图按数量明确为 unsupported，不回显名称或执行未知视图，凭据表只计排除数量 |
| 历史与当前状态隔离，保留来源/未知值，不覆盖身份、Owner、当前线路/发送 | namespace + snapshot 摘要保持来源；NULL/空值仅计数，不造号码/时间。预检只读，源目标重合拒绝；测试逐字节/目录比较来源、已有历史和相邻合成身份/当前状态哨兵不变 |
| 默认不包含正文/号码，逐条调试只在合成夹具展开 | 报告不含旧键、记录 ID、内容、联系人/号码/凭据或未知表名；合成秘密标记的脱敏断言。没有新增对外 debug 明细模式 |

### 文件、兼容与依赖

- `crates/compat/src/history.rs`：抽取既有离线 sidecar 检查、有界 hash 和记录解码为共享函数；原 validate/import 仍执行相同严格契约，未改归档 schema 或导入格式。
- `crates/compat/src/history/preflight.rs`、`history/preflight/tests.rs`：只读报告实现和真实临时数据库测试。
- `crates/compat/src/migrate.rs`、`preflight_cli_tests.rs`：新增命令及生产参数解析/输出路径测试；保留旧命令和非 Linux 拒绝行为。
- `crates/compat/src/nas_adapter_tests.rs`：已有 Go 夹具和新增 CLI 文件夹具共享仅测试用互斥锁，防止并行启动子进程影响文件锁；不改 Go 桥产品逻辑。
- `docs/nas-adapter/history-preflight.md`：报告字段、格式/版本含义、只读边界、限制和使用方法。
- 本报告。没有 workspace、Cargo.lock、Core/wire/storage、授权、默认配置、系统单元、其他成员文件或全局台账变更；没有新增依赖。

### 已执行与未运行

- Windows `cargo test -p rustsimhub-compat --locked`：退出码 0，5 项适用的规则/签名回归通过；新 SQLite 预检仅 Linux，未借此称其通过。
- Windows `cargo clippy -p rustsimhub-compat --all-targets --locked --offline -- -D warnings`：退出码 0；有系统增量缓存硬链接失败后复制的环境提示，不作为 Linux 结论。
- `cargo fmt -p rustsimhub-compat` 和 `git diff --check`：退出码 0。
- 初次离线 Cargo 因最新主线的 regex-lite 索引未缓存而退出 101；按锁定依赖联网刷新后成功，未更改锁文件。
- 首轮 Linux CI [40451542534](https://github.com/Viper-Boss/RustSimHub/actions/runs/40451542534)，完整 HEAD `048255bc882d8c1f672776e370cb4ce6c04c9a86`：compat 29/30，通过的新预检包括格式/数量、缺值、预测与原导入器对照、冲突/来源隔离、未知类型、非法记录、上限和不写文件。符号链接已拒绝但原因归为不可读，与夹具预期的私有普通文件要求不符；补充预检文件类型诊断，保留底层 NOFOLLOW、权限和原迁移行为。旧会话/历史测试通过；没有据首轮结果标整包成功。
- 同轮审查补充已知表的未映射列拒绝，以及源未读取/扫描中断时不谎报 unknown 统计完整，增加对应真实 SQLite 夹具；最终以新 HEAD CI 核对。
- 修正后的 Linux CI [40452099907](https://github.com/Viper-Boss/RustSimHub/actions/runs/40452099907)，完整代码 HEAD `ac272e7d5addfdf6d2f36e532dab55ccb4548db5`：`test` 与 `native-audio` 两任务均 success。实际日志确认 `cargo test --workspace --all-features --locked` 退出码 0，compat 库 32 项通过（包含 13 项预检夹具及原 history/session 回归），migrate 二进制测试 2 项通过（生产预检 CLI 入口和既有真实 Go/CLI 互操作）。不将入口函数测试写成新预检独立可执行进程测试。
- 同一 HEAD 的 `cargo clippy --workspace --all-targets --all-features --locked -- -D warnings` 与 `cargo fmt --all -- --check` 均退出码 0；生产特性及现有系统材料/转发/服务回归步骤通过。日志中的既有 proc-macro-error2 未来兼容提示保留，没有据此修改依赖。
- 本机仍无 Linux，Linux 运行证据来自上述 GitHub Actions。本轮未运行真实 NAS、旧页面或设备验证；最终仅报告的提交不冒用前一 HEAD 的 CI 结论。

### 主线集成、风险、来源与下一步

API/CLI 均只给出瞬时诊断，不作为授权或后续写入许可。未给目标时 `eligible_for_history_import=null`，
不假装已经比较冲突；有阻断时输出原因且非零退出。未知类型不宣称完整迁移。
128 KiB/100000 行等上限保留并用于诊断扫描，未知表预算和 128 个 schema 对象限制明确记在说明中。
没有创建目标目录、锁文件、归档或业务记录，没有调用硬件、发送、Owner 或当前 SIM 接口。
真实导入仍由原导入器重新检查；回退本 PR 即可，无迁移和硬件状态需要恢复。

来源为上述固定 main 的既有 history 实现、独立合成 schema 和第一批测试隔离方式，保留 AGPL-3.0-only。
Codex 辅助实现、测试和报告；不代用户声称人工审核，未估造实耗。没有真实 NAS、设备、用户数据或通信副作用。
后续 C-06 的导入故障/恢复、C-07 的完整权限/输入矩阵及 C-08 的综合交付仍待安排。
整体主线估算仍约 **80%**，本项不自行改变进度或真实设备验收。

## 2026-10-27 · C-06 独立交付

成员 yh1030 / member-c。本轮仅执行 C-06 / NAS-06 的重复、部分导入及恢复离线范围，
按任务书及 PR-DELIVERY-TEMPLATE 交付；不计 C-07～16，不接管设备或全局进度。
C-05 已由负责人 Viper-Boss 合并 PR #6，合并提交 `c9763c17407a382b51ec80c329618c290e73d0d1`。

- 起始最新 main：`b1a926d6e5ab8b2c3fcc0c835280f8677b838531`。
- 本人功能分支：`Viper-Boss/RustSimHub:codex/nas-history-recovery` → main；同仓库 PR，不 fork、不直接推 main。
- 原仓库 [PR #7](https://github.com/Viper-Boss/RustSimHub/pull/7) 交付 C-06；代码与离线验证完成，负责人审核及实机验收不代填。
- Linux 全部 CI 通过的完整代码 HEAD：`8b8d83b3f0466cfb7a1ce3baefd3bba712a2d95e`，对应树 `343932cb7a2757388254f1806700d9a7bdac1428`。本次证据补记仅修改本报告，最终完整 HEAD 与其 CI 状态在 PR 正文逐次维护。
- 文件仅 `crates/compat/src/migrate.rs` 的 cfg(test) 模块接入、`crates/compat/src/history_recovery_tests.rs`、`docs/nas-adapter/history-recovery.md` 和本报告。
- 不改变生产导入器、归档格式、权限、会话、Core/wire/storage、workspace/Cargo.lock、默认配置或其他成员文件；无新增依赖。

### 实现、原数据保持与要求映射

详细动作/断言见 [C-06 测试矩阵与边界](../nas-adapter/history-recovery.md)。新增 7 个父测试和 1 个专用子进程入口，
均调用真实 history::import/HistoryService、临时 SQLite 和既有独立合成 schema，子进程与现有 Go/预检文件夹具串行隔离。

| 要求 | 本次验证 |
|---|---|
| 相同包幂等、同旧 ID 不盲目覆盖 | 15 条首次 + 5 次重复，完整 ID/payload/digest/批次和文件字节；排序更早的新记录后发生同源冲突，全批回滚；不同源 16 条独立保存和 3 次重跑 |
| 真实后端中断/写入失败/恢复 | 真实 SQLite 在 3 条插入后 ABORT，2 次失败后去除故障再跑；非空 journal 下真实 SIGKILL；128 KiB 子进程 OS 文件大小限制触发真实数据库写失败，新/已有目标分别重开恢复再导入 3015 条和重复；另测提交后报告丢失的 3 次重试 |
| realm/SIM 代次/旧游标与映射隔离 | 真实 HistoryService 的 realm/归档绑定、principal generation、audience/epoch 游标与跨来源 record_id 拒绝；变化后的合成相邻 SIM 代次哨兵保持，历史仍标 archived，不冒充 Core 换卡实机验收 |
| 不恢复发送/通话和硬件副作用 | 提供历史 submitting/dialing 及相同 request_id；历史列表/详情仅 archived、归档业务表仅两张历史表（SQLite 内部统计表除外）；源文件/相邻合成当前库/身份/Owner 字节保持，报告 hardware_operations=0；没有硬件或发送接口 |

### 实际验证及未运行项

- Windows `cargo test -p rustsimhub-compat --locked --offline`：退出码 0，5 项适用签名/契约回归；新增 Linux 测试不在此运行，不据此称 C-06 通过。
- Windows `cargo clippy -p rustsimhub-compat --all-targets --locked --offline -- -D warnings`：退出码 0；保留增量缓存硬链接失败后复制的环境提示。
- `cargo fmt -p rustsimhub-compat`、`git diff --check`：退出码 0；最终格式与 Linux 运行结果以 PR 对应 HEAD 实测补记。
- 本机无 Linux/WSL，Linux 实际运行证据来自原仓库 GitHub Actions；缺工具/失败不静默跳过。
- 首轮 [CI 40468724355](https://github.com/Viper-Boss/RustSimHub/actions/runs/40468724355)，完整 HEAD `fc505032941db5ef463dca22273a835593537862`：compat 库 32 项通过；migrate 4 通过/5 失败/1 专用 worker 忽略。实际文件大小写失败（新/已有目标）、报告丢失重试、跨源冲突、journal 下 SIGKILL 恢复通过；realm/代次/游标断言已执行到最后表检查，误将 PRAGMA optimize 创建的 sqlite_stat1/stat4 当成业务表导致失败。其余 4 项被测试互斥锁 PoisonError 连带阻止，不算通过。修正为只比较非 sqlite_* 业务表，保留严格两张历史业务表及其他断言；不修改生产逻辑、SQLite 优化或权限，最终按新 HEAD 重跑。
- 修正后 [CI 40469187650](https://github.com/Viper-Boss/RustSimHub/actions/runs/40469187650)，完整代码 HEAD `8b8d83b3f0466cfb7a1ce3baefd3bba712a2d95e`：`test` 与 `native-audio` 均 success。实际 `cargo test --workspace --all-features --locked` 退出码 0，compat 库 32 项通过；migrate 9 通过/0 失败/1 专用 worker 忽略，其中 7 项 C-06 父测试全部实际通过，另两项为既有真实 Go/CLI 互操作及 C-05 生产预检入口。ignored worker 已由父场景在真实子进程调用，文件大小失败覆盖新/已有两种目标，强杀实际核实 signal=9，报告丢失核实退出 72；没有把 helper 单独 ignored 当作测试跳过。
- 同一代码 HEAD 的 `cargo clippy --workspace --all-targets --all-features --locked -- -D warnings`、`cargo fmt --all -- --check`、生产特性及现有系统回归均退出码 0。既有 proc-macro-error2 未来兼容提示保留，未改依赖。最终仅报告的提交不冒用本代码 HEAD 的 CI 结果。
- 没有真实 NAS、设备、用户数据、通信发送、Core 换卡集成或硬件掉电测试。文件大小限制不是实际 ENOSPC，进程退出不是断电；不删除来源、手工 journal 或清库来通过测试。

### 集成、风险、回退与来源

Linux 准备锁定 Rust、python3 标准库及原 Go 工具链，运行上述 compat 测试/严格 Clippy/fmt；
专用 ignored worker 由父测试实际执行，不能把普通输出的该 worker ignored 当作父场景未运行。
临时 SQLite 失败触发器仅用于测试数据库，Python 只给子进程设置资源限制并 exec 同一 Rust 测试程序；没有产品故障开关或替代迁移器。
新目标中途失败允许留下有效空 schema，禁止半批记录/成功标记；完整逻辑状态在正常 SQLite 重开后核对。
本批只改测试和说明，回退本 PR 即可，不需要数据迁移或设备恢复。

复用固定 main 的既有导入器、查询器和独立合成 schema，保留 AGPL-3.0-only；Codex 辅助实现、验证和报告。
不代用户声称人工审核，不估造有效工时，不改作者/贡献数字。负责人按最终完整 HEAD 审核，结论待填。
下一步由负责人按 PR 当前完整 HEAD 审核；C-07 及后续尚未执行。整体固定量表仍约 **80%**，不自行增加验收点。

## 2026-10-27 · C-07 / C-08 第二批收尾交付

成员 yh1030 / member-c；用户授权继续任务后，从已合并 C-06 的最新主线开展 C-07，并按 C-08 汇总第二批交付。
既有各节为当时历史记录，不覆盖原失败与验证证据；本批未执行 C-09～16，不调整全局进度。

- 起始 main：`2f1595aa3b71fbfdefdbc8801ff6ecd844144b85`，也是负责人合并 PR #7 的提交；C-06 PR HEAD 为 `b13bae7ea80a4c8f45396609b22bfe1f8cc42923`。
- 原仓库个人分支：`Viper-Boss/RustSimHub:codex/nas-history-input-matrix` → main；完整当前 HEAD 和 PR 链接逐次写在 PR 正文，不 fork、不直接推 main。
- 原仓库 [PR #9](https://github.com/Viper-Boss/RustSimHub/pull/9) 交付 C-07/C-08；实现和离线验证完成，待负责人按最终完整 HEAD 审核，不代填审核/合并或实机验收。
- Linux 完整 CI 通过的代码 HEAD：`f9b6f7c40e041895849a533ecb96e50f4b0d075f`，对应树 `6fcf2508433653623fec6b53489c4307ffe4443c`。本次最终补记仅改本报告，最终提交 HEAD 及其 CI 在 PR 正文单独维护，不混用截面。
- 新增历史测试 8 项、会话测试 3 项，复用真实 API/原 fixture 与签名 Request，分别由既有 history/session tests.rs 接入；只改测试、本人 docs/nas-adapter 文档和本报告。
- 无生产逻辑、授权/签名/期限/格式协商变更，无 workspace/Cargo.lock 或新依赖，不改 Core/wire/storage、默认配置、其他成员/全局台账或设备。

### 任务、文件和证据

| 任务 | 实际交付 |
|---|---|
| C-07 权限/输入 | `crates/compat/src/history/tests/input_matrix.rs` 与 `session/tests/input_matrix.rs`；NAS 身份/撤销/范围、游标与 ID、实际过期与签名时间边界、错误版本、非法路径/namespace、BLOB/UTF-8/缺列/复合键、canonical JSON 与稀疏文件上限，拒绝后原文件保持；[完整矩阵](../nas-adapter/history-input-matrix.md)映射已有测试，避免重复实现 |
| C-08 综合交付 | [第二批汇总](../nas-adapter/second-batch-delivery.md)，分别列 C-05/06 已有 HEAD/CI 与本批结果、导入前后数量/重复次数、保持证据、主线集成和真实 NAS 补验，不拿旧 HEAD 的结果冒充新增提交 |

会话到期通过 cfg(test) 设置真实会话的内存 deadline 为过去，再调用原 subject/authorize；验证签发 expires_at 仍为 now+900。
不等待 15 分钟、不新增产品时钟注入、不修改会话期限。相邻 current/outbox 文件只是合成保持哨兵，不冒称 Core 队列集成。

### 已执行、失败与未运行项

- Windows `cargo test -p rustsimhub-compat --locked --offline`：退出码 0，5 项适用回归；新增 11 项 Linux 测试未在 Windows 运行。
- Windows `cargo clippy -p rustsimhub-compat --all-targets --locked --offline -- -D warnings`：退出码 0；硬链接缓存失败后复制的环境提示保留。
- 官方便携 Go 1.27.3，标准库/无模块下载，`go test -count=1 -timeout=60s -v ./tools/nas-adapter`：退出码 0，Windows 14 个顶层测试通过；`go vet ./tools/nas-adapter`：退出码 0。黄金向量明确为 `crates/compat/src/legacy-golden.json`，缓存仅 .local。
- 首次 Go 测试退出 1：我把黄金向量路径误写到不存在的 testdata，且当前受限环境回环 TLS 读取返回 transport_failed；修正路径并允许本机合成回环测试后，上述实际完整重跑通过。未关闭 TLS 校验/重定向保护或改变期望来通过。
- `cargo fmt -p rustsimhub-compat`、`cargo fmt --all -- --check`、`git diff --check`：退出码 0。
- 本机无 Linux/WSL，Linux 执行证据来自本批原仓库 GitHub Actions，不将 Windows 或交叉编译视为 Linux 执行。
- 首轮 [CI 40478550024](https://github.com/Viper-Boss/RustSimHub/actions/runs/40478550024)，完整 HEAD `cf309c95ba7ceedf44de95df21105fb82cb6e36f`：compat 库 41 通过/2 失败，新增 9/11 通过；过期、签名时间边界、版本、恶意游标/ID、非法记录/缺字段/重复、合法未知值及真实文件上限已通过。两处测试假设错误：inactive sources 返回 Forbidden 而非空列表；既有 Path equality 规范化无害 ./，该路径仍是同源且可幂等读取。修正为分别断言明确拒绝与同源 0/15；相对路径、..、链接、NUL、权限/来源等拒绝断言保持，不改生产逻辑。首轮库失败后 bin/Go 及严格 Clippy 未运行，不算通过，最终按新 HEAD 重跑。
- 修正后 [CI 40479180274](https://github.com/Viper-Boss/RustSimHub/actions/runs/40479180274)，完整代码 HEAD `f9b6f7c40e041895849a533ecb96e50f4b0d075f`：`test` 与 `native-audio` 均 success。实际 `cargo test --workspace --all-features --locked` 退出码 0；compat 库 43 项全部通过，包含本批全部 11 项新增矩阵及原 history/session/预检/签名回归。migrate 9 通过/0 失败/1 专用 worker 忽略，包含 C-06 七项真实恢复父场景、Go 真实 CLI 互操作和 C-05 生产预检入口；worker 由父测试实际在子进程调用，不属于静默跳过。
- 同一代码 HEAD 的 `cargo clippy --workspace --all-targets --all-features --locked -- -D warnings`、`cargo fmt --all -- --check`、生产特性检查与现有系统回归均退出码 0。保留既有 proc-macro-error2 未来兼容提示，未改依赖。最终仅报告提交的 CI 以 PR 正文记录为准，不拿本代码 HEAD 的结果冒充最终提交结果。

### 集成、限制、来源与下一步

按 C-08 文档核对同 HEAD 的真实 compat/Go、严格检查；保留原私有文件/离线锁、格式和鉴权边界。
错误版本/冲突不重新初始化，来源/授权/历史不通过删除或改 namespace 来“修复”。大文件是临时稀疏文件，内存保持有界，不填满宿主磁盘。
没有真实 NAS/设备/通信副作用；Core/HTTPS 组合鉴权、真实 SIM/旧页面、声音和硬件故障由主线补验，不扩大离线范围。
回退本批测试和文档即可，无数据迁移/部署或设备恢复。

来源为固定 main 的真实 history/session/签名模块及原独立合成 schema，新增源码保留 AGPL-3.0-only，无新依赖。
Codex 辅助实现、验证和报告；不估造有效工时、不改作者/贡献数字、不代用户或负责人声称人工审核通过。
下一步由负责人按 PR 最终完整 HEAD 审核，随后安排 C-09 等后续包；本批不自动增加已验收计数。整体固定量表仍约 **80%**，真实 NAS/Core/设备补验边界保持。

## 2026-10-27 · C-09～C-12 备份恢复离线交付

成员 yh1030 / member-c；用户明确授权“继续开展 C09-C12，按照要求提交文件与报告”。
从已合并 PR #9 的最新主线独立开工，不执行 C-13～20，不接管主线生产修复或设备恢复。

- 起始 main：`6f228fd3dcb9ce6da54d7910f4b044833e71a91a`。
- 分支：原仓库 `Viper-Boss/RustSimHub:codex/member-c-recovery-matrix` → main；完整当前 HEAD、PR 与对应 CI 在 PR 正文逐次维护，不 fork、不直接推 main。
- 文件仅新增 `crates/recovery/tests/offline_business_matrix.rs`、`tools/test-business-recovery-matrix.py`、`tools/fixtures/backup-recovery/{matrix,business,identity,outbox}.rs` 与控制器 `test_runner.py`、`docs/recovery/` 三份说明和本报告。
- 没有改 storage/Core/forwarder/auth/recovery 的生产逻辑、schema、Root/设备脚本、workspace/Cargo.lock、依赖版本、其他成员文件或全局台账。

### 逐项交付与验收边界

| 要求 | 本次材料与真实模块验证 |
|---|---|
| C-09 盘点备份、一致性、WAL、realm | [范围清单](../recovery/backup-scope.md)：逐接口列 Store 全表快照、明文/加密身份、独立 outbox、NAS 历史/授权和公开部署配置域；实际代码区分 Store SQLite Backup API 与在线主文件复制，列恢复 realm/授权/操作规则 |
| C-10 成功/失败矩阵 | 15 个命名 Rust 场景中，实际 backup/restore/Identity/Auth/Queue 覆盖同批、混批、截断/空/缺包、错 key/pin/device/Owner/realm/version、重复及错误路径；已有数据保持、无效新目标隔离证据保留，不清库来通过 |
| C-11 操作/回执与重开 | 用真实 API 构造短信/通话 prepared/submitting/Unknown/submitted 和 outbox confirmed/unknown/in_flight/queued；恢复核对 ID/attempt/回执/短信/通话/游标，旧八条 SMS/call 操作 begin 许可为 0；正常重开和新 realm 绑定拒绝分别验证，不恢复为自动发送 |
| C-12 文件与交付 | [完整矩阵/命令](../recovery/offline-matrix.md)、[生产缺口与最小提案](../recovery/consistency-gaps.md)及本报告；原仓库小 PR，所有输入合成，仅临时目录，无设备/EDL/分区/维护脚本 |

真实模块由 Cargo 按根锁文件构建到新临时 target，取 compiler-artifact JSON 的精确 rlib，
同一 Clippy driver 对独立夹具 `-D warnings` 编译后运行，15 个固定场景必须逐个通过且零 ignored。
Cargo 的 recovery 集成测试自动调用；不用新增 manifest/依赖或把别人的生产文件加入本批。
四项 Python 单测只验证构建元数据选择，不能替代十五项业务矩阵。

### 已执行与待补验证

- 本机 Windows；`python tools/test-business-recovery-matrix.py`：退出 0，只列计划，未构建/恢复；Linux 实际模式不在 Windows 假跑。
- `python -m unittest discover -s tools/fixtures/backup-recovery -p test_runner.py`：4 项通过，退出 0；首次受限临时目录创建/清理 ACL 失败，换为允许本机临时测试的执行环境后真实重跑，未放宽选择器断言。
- `cargo test -p rustsimhub-recovery --lib --locked --offline`：退出 0，Windows 3 项原加密身份回归通过；新增 Linux 业务矩阵未借此标成功。
- `cargo test -p rustsimhub-recovery --locked --no-run`：退出 0，库/bin/新测试入口编译。最初 offline 因 aead 等已锁依赖本机未缓存失败；联网获取锁定依赖后编译，未改锁文件。保留原 recovery/main.rs 在非 Linux 的未使用 Path 提示，不为本包改生产入口。
- Windows `cargo clippy -p rustsimhub-recovery --lib --locked --offline -- -D warnings`：退出 0；此项仅库，不冒称 Windows bin 全 targets 严格检查通过。既有 proc-macro-error2 未来兼容提示保留。
- `cargo fmt --all -- --check`、`rustfmt --edition 2024 --check tools/fixtures/backup-recovery/matrix.rs`、`git diff --check`：退出 0。
- 本机无 Linux/WSL；15 个真实业务场景、Linux 集成接入和全 targets/独立夹具严格检查待本批 PR CI，未填成功。最终以对应完整 HEAD 的实际结果补记。

### 保持证据、已发现边界与主线接手

关闭的备份/身份/目标比对文件摘要；活跃 SQLite 比对各表真实类型化逻辑记录，避免把 WAL/SHM 内部读标记当业务变化。
Store 恢复可能在新目标留下 pending/无效副本；不声称所有失败零写入，已有活动/关闭目标不覆盖。
同身份跨批 Store B + 关闭 outbox A 的复现明确暴露缺少联合 manifest/一致切面；Store 单库成功不是完整包成功。
旧 outbox 在恢复后被当前 Auth/realm 拒绝，证明隔离而非联合恢复已实现；不重写绑定，不把 Unknown 变 queued。
Confirmed 仅是实际 Queue::finish 的合成确认，无提供商/设备发送；许可和 transport 计数不冒充实机通信计数。

主线按完整 HEAD 运行正常 recovery 测试（需 Python、锁定 Rust/Clippy 和缓存依赖），或显式 --run；
审阅三个缺口及版本化联合包/写者协调的最小提案，再决定是否扩展生产接口。本包不实现替代联合校验器。
真实数据、硬件掉电/文件系统、系统恢复与部署由主线处理；无设备访问、真实短信/电话/推送、网络 transport、Root/签名/分区操作。
回退本批新测试/文档即可，无产品格式或设备状态需回退。

来源为固定 main 的真实公开模块及已有合成夹具方法；新增源码保留 AGPL-3.0-only，无新第三方依赖。
Codex 辅助盘点、实现、测试和报告；未估造有效工时、不改作者或贡献数字、不代负责人填写验收结论。
下一步补本批 Linux 证据，完成 PR 审核就绪交付；整体按固定量表仍约 **80%**，不自行更新全局验收。

### C-09～C-12 补记：原仓库 PR 与 CI 启动阻塞

- 原仓库 [PR #10](https://github.com/Viper-Boss/RustSimHub/pull/10) 已创建，保持 Draft；尚未完成 Linux 验证，不标记 Ready、审核通过或任务验收。
- 首次公开交付完整 HEAD：`befa0307cf59b97179b53830532d982e024659b4`；当前补交后的完整 HEAD 与精确 CI 状态由 PR 正文维护，不能将旧提交测试结果移作新提交证据。
- 该 HEAD 的 [push CI 40492864750](https://github.com/Viper-Boss/RustSimHub/actions/runs/40492864750) 和 [PR CI 40493025940](https://github.com/Viper-Boss/RustSimHub/actions/runs/40493025940) 均失败。检查详情明确：账号付款失败或费用额度需要调整，任务没有启动。PR run 的 `test`（122151413668）及 `native-audio`（122151413896）均没有步骤、runner_id=0；push 的 test（122150885009）亦回读到同一付款/额度注释。没有实际编译/运行结果，也没有可下载的 test 日志；取日志返回 404 BlobNotFound，不能当代码测试失败或通过。
- 没有修改仓库费用、工作流、配额、主线或部署；需要负责人恢复 Actions 可运行条件，或在已有 Linux 环境按矩阵文档执行同一完整 HEAD。未因这次启动失败盲目重复运行，也不把 Ubuntu 镜像迁移通知误当失败原因。
- 本次仅增强 C-11：恢复后对原短信/通话请求逐项重放，断言原 ID/状态、无新许可及全业务表逻辑不变；另对原 Unknown 短信请求连续重放三次，再验证换 SIM 后冲突。所有调用仍为真实公开模块，未增加生产接口或发送器；这些新增断言尚未在 Linux 执行，不能称通过。
- 补交源码本机 `rustfmt --edition 2024 --check tools/fixtures/backup-recovery/matrix.rs`、`cargo fmt --all -- --check`、`git diff --check` 均退出 0；计划模式退出 0，仍仅列 15 个场景；控制器单测实际 4 通过/0 失败，退出 0。这些静态/选择器检查不代替 Linux 15 场景、全 targets Clippy 或完整 CI。
- C-09 盘点及 C-10/C-11 的测试源码、C-12 文档和 PR 已提交；C-10/C-11 运行验收与 C-12 最终证据收尾仍待上述执行条件。保持原生产缺口、来源/许可、真实设备未验收及整体 **80%** 边界，不开始 C-13～20。
