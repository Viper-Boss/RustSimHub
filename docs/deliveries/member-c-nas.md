# 成员 C · NAS 第一批离线交付

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
