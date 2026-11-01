# 成员 C · NAS 兼容与历史离线交付

本报告按日期追加交付；当前 C-07/C-08 见文末及 [PR #9](https://github.com/Viper-Boss/RustSimHub/pull/9)。第一批、C-05 和 C-06 各节保留提交时的历史快照；PR #3、#6、#7 后续已由负责人合并。当前批次的实现、测试和审核状态以本批节及对应完整 HEAD 为准。

## 2026-10-30 · C17 来源清单、下一阶段接口与最小失败样例

成员 yh1030 / member-c。同仓库分支 `codex/member-c-kernel-sources` → main。
起始主线为 `6b5de5f3ca5ff706adacd8f25ed7f638546739a9`。
C16 PR #98 的 HEAD `5cba7bd0a609ed44eed7f125f4275a1c3982aeec` 当前仍待审核；
用户在知悉该状态后再次要求“继续开展推进”，本轮先独立提交 C17 来源/接口/最小样例供负责人审阅，
不把 C16 计为已验收，不进入 C18～C20 检查器实现。
本节放在报告前部以避免与 PR #98 在末尾新增的 C16 报告交叉覆盖，原历史记录保持。

本包实际校验的内容提交为 `6a20e075a270cb3c627d71fbc9c5f5c4e3bccb26`，
tree `1c501ee59fc73431b983f2f7fc6d55adc3a0db70`；随后仅补文档（本报告及README的板型/架构/PRELOAD说明），
测试脚本和固定JSON输入与已测tree相同；最终完整 HEAD 在 PR 正文回读，不把后续文档字节称为已运行源码。

### 文件与实际范围

- `docs/kernel-artifact-audit/README.md`：固定内核 release/commit/tree、上游和板级 boot、Image/压缩内核、DTB、模块归档/清单各摘要；22 项模块身份/依赖与外部 APR 缺口；厂商固件实际摘要和再分发许可未知。
- `source-index.json` 固定31个公开跟踪文件的长度/SHA256/Git blob；`source-checks.json` 记录19项来源交叉结果，其中7项为实际跟踪源文件字节匹配。声明相符与实际制品匹配严格区分，不生成另一套发布manifest。
- `proposal.md` 提出原锁消费、默认只列计划、显式公开/合成输入、有限读取/路径/版本、缺证据输出与退出分类、FDT板型和模块依赖验证方式；列10类后续失败规格，均待负责人审核，不冒称已冻结或已测。
- `minimal-failures.json` 是现有 `audio_modules.modinfo` 的1个合成ARM64 ET_REL正例及错架构/越界节表/截断3个最小失败字节样例，从不执行这些字节。
- `tools/kernel-artifact-audit/test-source-examples.py` 默认只列计划；显式 `--run` 要求普通Linux UID，先核对31个固定来源的长度/SHA/blob，再调用现有真实解析器验证上述样例。它不是C18制品检查器，没有新生产逻辑、依赖、编译器、外部命令、设备或网络。

### 实际验证

| 命令/来源 | 实际结果 |
|---|---|
| `python tools/kernel-artifact-audit/test-source-examples.py`（Windows） | 退出0；execute=false，列5项计划；没有读取制品或执行Linux测试 |
| 固定tree的Git归档 → WSL2 Ubuntu-24.04 普通UID1000、Linux原生目录：`python3 -B tools/kernel-artifact-audit/test-source-examples.py --run` | 退出0；5通过/0失败/0错误/0跳过，实际0.202秒；包括1项31文件来源校验、1个合成正例、3个真实解析器拒绝样例。Python3.12.3，kernel 6.18.54.1-microsoft-standard-WSL2 |
| 归档/执行后字节检查 | 37个公开文件逐个与固定Git归档相等，执行后源码未改变；归档SHA256 `bc9bdf8b9ffb70c6ea5a0ea838c24fb84b0a806a2035fd24c771319ea32a4898` |
| 文档/固定来源复核 | 两页11个本地或精确SHA/行号来源链接存在；31来源长度/SHA/blob与主线固定对象相等；重新计算19项交叉结果与交付JSON完全相同，7项实际源字节匹配；退出0 |
| Python AST / 三份JSON / `git diff --cached --check` | 均通过；未修改受测生产源或隐藏失败断言 |

既有13项内核/DTB/模块隔离测试的历史来源是2249762；31个受查输入与该截面逐文件相同，
本次没有重跑这些测试或将旧运行标为当前HEAD的新增执行。新的5项只证明固定来源/现有解析器最小样例，
不代表C19完整混版矩阵，19项来源交叉检查也不折算成19项新业务/验收任务。

### 证据与主线交接

仅日志/摘要包 `C17-source-validation-1c501ee.zip`，5,448字节、7条目，SHA256
`5fd84276de2ff0e395e03103bdc75e77122e28e119c424fd6f4abcf8bad38eb1`；ZIP CRC及逐文件内容核对通过。
含实际summary/逐项日志、37文件摘要、文档结果和来源交叉结果；无源码归档、ELF、固件、数据库、密钥、私人路径或设备资料。
只在本机准备并向用户提供，未代发负责人。

待审核：本来源提案精确HEAD、原锁消费/合成输入边界、缺证据退出语义、预算/FDT解析范围；审核后才进入C18～C20实现。
未验证：完整源归档、实际boot/Image/DTB、681原装/22音频模块二进制、厂商固件摘要/许可、Root/ARM加载、冷启/恢复/掉电、生产签名及发行支持。
板级锁中的历史rebuild=false不改写；主线历史重建报告保留其本来来源，不当本轮重建。
本PR不修改其他成员文件、既有锁/固件/重建器/校验器、全局看板或审核台账，不自行合并或更新已验收步数。
来源/许可：项目公开原锁及工具，GPL-2.0-only/原SPDX保留；合成ELF布局复用固定版本的 `tools/system/test_audio_modules.py`，新测试脚本AGPL-3.0-only。无新增依赖或第三方库代码复制。
Codex辅助读取、整理、代码及命令验证；没有作者重写，成员自报实耗未填写。
主线集成只审本包7个文件并按需合并；撤回只删除新增资料/测试脚本和本节，无设备或业务状态回退。
原固定P0～P7整体仍约 **80%**；来源交付不等于真实制品、支持矩阵或硬件验收。

## 2026-10-26 · 第一批历史截面

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

## 2026-10-27 · C-15 用户手册草稿

成员 yh1030 / member-c。用户在已转交 C09～C12 Linux 测试包、等待负责人运行期间，明确要求开展 C15；
本轮按 C15 / REL-05 编写草稿，不执行 C13/C14 更新故障测试，不将本稿计作 C16 第四批综合交付或 REL-01 正式支持冻结。
C09～C12 仍在独立 PR #10 等待真实 Linux 结果，本分支不包含、替代或撤销其源码/报告/待验状态。

- 起始最新 main：`6f228fd3dcb9ce6da54d7910f4b044833e71a91a`，已实际 fetch 核对；独立分支 `codex/member-c-user-manual-draft`。
- 当前完整提交 HEAD 与草稿 PR 链接由 PR 正文单列；本节不自行声称负责人已审核或正式发布。
- 只新增 `docs/manuals/README.md`、`getting-started.md`、`daily-use.md`、`one-click-tests.md`、`troubleshooting.md`、`backup-and-maintenance.md`、`support-evidence.md`、`check-links.py`，并追加本人报告。
- 没有改生产源码、接口/权限/签名、其他成员文件、任务书、workspace/Cargo.lock、全局进度或设备配置，无新依赖。

### C15 要求和实际材料

| 要求 | 本次草稿 |
|---|---|
| 普通用户入门 | 安装交付后的可信地址/设备身份核对、已有浏览器登录、负责人安排的首次认领、新浏览器申请配对；不将申请视为批准，不提供清库重认领办法 |
| 日常操作与常见错误 | 短信读取/发送/回收站、电话入口，身份/权限/存储/Unknown/声音/分页/版本/配网错误的有限处理；每类处理链接证据范围 |
| 一键测试 | 区分只读状态与拨出/自动接听/短信/外部合成通知，说明确认、停止、原操作核对及实际通信授权边界；不因写手册发起测试 |
| 备份/恢复 | 分列身份、Owner/浏览器材料、业务库、outbox、配置和系统域；说明一致性与恢复域、重新授权和 Unknown，不把旧身份恢复或待验 PR10 写成全业务恢复 |
| 升级与维护 | 区分有限已验收的签名应用升级、离线版本比较和未获支持的整机 OTA；保留 schema/回退/真实 Root 与数据锁缺口，不提供整机升级/分区操作步骤；维护交接仅列脱敏材料 |
| 支持声明与隐私 | S01～S16 每项写实际证据、验证层级及排除范围，链接公开验收记录与必要源码；Bark 使用 X1Q 手机已收合成通知事实，保留真实短信/其他场景未验边界；手册无私有脚本路径、设备地址、凭据、原始日志或通信内容 |

### 实际检查、未运行与下一步

- `python docs/manuals/check-links.py`：退出 0；七页 Markdown 的 117 个本地链接/锚点、16 个唯一支持 ID 及所有引用均通过，无缺文件/缺锚点/尾部空白或受限材料标记。脚本只核对这些手册及显式目标，不联网，不扫描私有目录或判断实机验收真伪。
- Python 标准库 `ast.parse` 核对 `docs/manuals/check-links.py`：退出 0；无第三方安装。`git diff --check`：退出 0，最终暂存后再次核对新文件。
- 首次内联链接核对命令在 PowerShell 引号解析阶段失败，未执行 Python、未产出文档验证结果；改用明确的独立标准库脚本后，实际完整核对通过，没有删除链接或放宽证据 ID 清单。
- 逐页对照实际生产页面按钮、P6AE/P2E/P2F/P2H/P3K/P3L/P3M/P3N/P7A、P1C、P4B/P4W、P6AA/P6AF、U3 与 X1Q/X1T/X1X 的公开记录；设计、源码/离线、有限实机及当前待验分开，历史失败不改写。
- 本次只写文档和只读文档检查工具，未运行 Rust/Go、Linux 恢复矩阵、网页自动化/截图或设备实测；没有拨号/短信/推送/普通流量、部署、签名或分区操作。
- 当前为可审阅草稿；正式版本支持、交付地址/证书办法、浏览器范围、实际恢复与升级入口、最终页面核对仍由负责人确认。C16 的综合交付/截图及 C13/C14 保持未完成，不把本轮本地链接核对扩写为其全部验收。

来源为上述固定主线公开源码与验收文档，保留仓库 AGPL-3.0-only；文档检查脚本同许可证，无新第三方依赖。
Codex 辅助资料核对、写作和检查；未估造实际工时、不修改作者/贡献数据、不代负责人声称验收。
回退本轮新手册及报告追加即可，无产品数据或设备状态需要回退。
整体按固定 P0～P7 量表仍约 **80%**，C15 草稿不自行增加正式支持或主线验收点数。

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

### C-09～C-12 补记：负责人 Linux 补验包已准备

用户确认负责人有 Linux 环境，并要求把待测材料打包交负责人执行。
已从完整代码 HEAD `bfaa8dcea7980e9e95fad5c6754fb3482f55a33d` 的 Git 归档生成 `C09-C12-Linux-bfaa8dc.zip`（2,257,992 字节）；
压缩包 SHA256 为 `4ad2b9d25bb3600d0b5c719fc10f944eb9534f0440aca5cdee6d032eeb328099`。
包经当前聊天交付用户转交，不声称已发送负责人或负责人已经执行。

- 内容为中文说明、`SOURCE.json`、独立 `run-linux.py` 和固定源码 `source.tar.gz`；766 个公开源码文件逐个 Git blob 核对一致，保留许可证/来源；没有 .git/.local/.private、真实数据、凭据、生产数据库或预编译产物。
- 解压后 Linux 普通用户运行 `python3 run-linux.py`；缺依赖缓存且允许下载锁定依赖时用 `python3 run-linux.py --fetch`。需要既有 Rust 1.98.0/rustfmt/Clippy、Python 3.10+、C 编译器；不用 sudo 或设备访问。
- 外层收集工具先验证归档，再从固定源码解压到独立新目录，依次运行四项控制器单测、fmt、直接十五项矩阵、Cargo recovery 接入和严格 Clippy；逐步保留真实命令/退出码/耗时/日志，受限超时及失败不静默跳过。两次矩阵分别验证直接入口和 Cargo 接入，不重复增加任务数或验收比例。
- 结束生成单独 `results-*.zip`，只包括步骤日志和 `summary.json`；不打包工作区、数据库、密钥或缓存。负责人回传此结果后，成员 C 核对完整 HEAD、修正范围内问题并补交实测报告。
- 本机实际验证：外层脚本 Python 语法解析、`--verify-only`、766 文件逐个 Git blob 比对、ZIP CRC 和四文件清单均通过；没有执行外层 Linux 路径或十五项业务测试，不把打包/完整性验证写成 Linux 成功。
- 本次报告补交只记录交接，不改变已打包代码；源码归档固定在上述完整 HEAD。PR 当前报告提交 HEAD 单列在 PR 正文；完整仓库 CI、真实恢复、主线审核/合并仍未验收，整体仍约 **80%**。

### 2026-10-27 · 本机 WSL2 测试环境准备（待 Windows 重启）

用户改为在本机安装 WSL2，并要求后续 Linux 测试默认由代理在 WSL2 执行；不再把负责人 Linux/Actions 额度作为唯一执行条件。
本轮已完成 Windows 侧组件安装，尚未运行 Linux 测试，不能将环境准备计作 C10/C11 验收。

- 硬件只读检查：x64 Windows build 26200.9711，固件虚拟化与二级地址转换已启用；安装前 hypervisor 未运行。没有更改 BIOS 或安全策略。
- 下载 Microsoft 官方稳定 WSL 3.0.3.0 x64 MSI；SHA256 `ae977286974255bd26bf925f00f40f5692c6b2084155cbc0563c09aea091f211` 与官方发布一致，Authenticode 为 Valid，签名者 Microsoft Corporation。
- 标准管理员确认后，Windows 原生 DISM 启用 VirtualMachinePlatform，实际退出码 3010、日志明确需要重启；`/norestart` 生效。原生 msiexec 安装 WSL，实际退出码 0，没有自动重启。
- `wsl.exe --version`：退出 0，实际 WSL 应用 3.0.3.0、内核 6.18.54.1-1；`wsl.exe --status`：退出 0、默认 WSL 版本 2，但仍提示当前 WSL2 无法启动，不能仅凭退出码称环境可用。`wsl.exe --list --online`：退出 0，确认 Ubuntu-24.04 可用；尚未安装发行版。
- 保留失败：原精简 wsl 安装入口带参数未开始安装；旧 Windows PowerShell 默认脚本策略阻止安装脚本，未执行。没有放宽策略，改用 Microsoft 签名的原生 DISM/msiexec 完成上述实际安装。原始安装日志与接续记录仅本机保管。
- 下一步需要用户保存其他应用工作并允许/手动重启 Windows。重启后继续安装 Ubuntu、普通 Linux 测试用户与锁定工具链；使用 Linux 原生文件系统执行固定 HEAD 的十五场景、Cargo 接入及严格检查，再验证默认 WSL2 测试入口和结果收集流程。
- 本轮无 Ubuntu/WSL2 用户进程测试、无 Rust/Go/业务矩阵新结果、无设备/真实通信/推送/流量或分区操作。C09～C12 的运行证据和验收仍缺，整体按固定量表保持 **80%**。

### 2026-10-28 · 本机 WSL2 已启动，真实矩阵复测

用户已完成重启，并要求离开期间继续推进。实际 HypervisorPresent=true；官方 Ubuntu-24.04 已安装并启动，
发行版为 Ubuntu 24.04.5 LTS，内核 `6.18.54.1-microsoft-standard-WSL2`，测试用户 UID/GID1000，未授予 sudo。
本机新建 WSL 配置限制6GB/4CPU且保持防火墙。镜像网络失败（ConfigureNetworking/0x8007054f）后安全回退到 NAT，
普通用户直接访问 Ubuntu/Rust/Cargo 官方下载地址成功，未开放LAN代理或放宽TLS验证。

- 已实际安装 C 编译器/Python/Go/Node 与必要开发库。Rust 官方脚本下载停滞、静态安装器直连180秒超时的失败保留；改由本机代理下载官方程序与五个锁定组件，逐项核对官方 SHA256 后导入 Linux 缓存，原生 rustup 安装 1.98.0/rustfmt/Clippy 成功。
- 只复用 Cargo.lock 校验过的164个公开 crate 包和292条公开索引，未复制凭据、Windows可执行程序或修改锁文件；其余依赖由 Linux Cargo 实际准备。
- 新增 `tools/fixtures/backup-recovery/wsl_test.py` 与 [WSL入口说明](../recovery/wsl-tests.md)：固定 Git commit/tree 归档、SHA256、普通用户/WSL2内核核对、Linux原生源码/TMPDIR/target、有界步骤、完整日志/退出码/实耗和仅日志的 evidence.zip。默认只列计划；后续 Linux 测试采用这一入口或同一 WSL2 发行版的明确命令。

首轮精确提交 `45c54d4b32bc00a1093cf54d4faead9795cdbdc7`，源码 tree `f9aad114260be0e3d7c2748fcd3543f15d0baffb`：
`python tools/fixtures/backup-recovery/wsl_test.py --run --suite recovery --fetch --ref 45c54d4b32bc00a1093cf54d4faead9795cdbdc7` 返回1。
该轮 WSL 执行器为另行记录 SHA256 的新本地文件，不冒充该提交已有工具。
依赖/格式/四项控制器/严格Clippy均0；直接矩阵11通过/4失败/0忽略；Cargo recovery库4通过，集成矩阵同样11/4导致退出101。
失败和成功步骤原样保存，日志压缩包CRC检查通过且只含步骤日志/来源/summary，没有数据库或密钥。

四项失败发生在只读观察/恢复的来源目录检查：SQLite WAL格式文件被普通只读打开后可新建 WAL/SHM。
备份输出单文件清单原本在观察后才检查，混入观察器副作用；其余整目录等值也没有区分原文件和新增辅助文件。
修正仅在测试：发布后先检查单文件；已完成、无非空WAL的快照用 immutable URI 观察；产品 restore 打开方式不改。
来源原有文件逐个摘要保持、有效快照完整逻辑保持；新增只允许空WAL与有界SHM；已有目标仍严格不覆盖。
没有删辅助文件、清库或屏蔽失败。新增 GAP-REC-04 明确来源目录非零写入及只读介质/恶意旁路WAL尚未验收，最小生产提案交主线。

修正后精确暂存 tree `a7fd84f98d7a6a30e0c217d34b3cf1846194ce21`，
`python tools/fixtures/backup-recovery/wsl_test.py --run --suite recovery --ref a7fd84f98d7a6a30e0c217d34b3cf1846194ce21`：
该轮已实际完成：直接十五项真实模块矩阵全部通过、0忽略；Cargo recovery库4项通过、集成入口1项通过（内部再次执行全部十五场景）、bin/文档0测试正常退出；包全targets严格Clippy、依赖/格式/四项控制器也均为0，执行器最终 passed=true。
所有步骤命令/退出码/实耗与来源保存在同轮日志；既有 proc-macro-error2 未来兼容提示保留，未变更依赖。
该证据明确标记为 tree，而不是最终提交HEAD通过；发布后仍须核对最终完整提交的适当回归及范围。
无设备/实际通信/普通蜂窝/签名部署/分区操作；整体仍按固定量表 **80%**，成员不更新主线验收。

#### 完整代码 HEAD 的最终 WSL2 回归及交付

完整代码提交 `f5e4cb8cfc886828e43394b016a3df9d9834e2e6`，tree `dbb3a95f4ef29a5d4231a003b1d40aa83d680892`；
与上述已通过的 a7fd84f 暂存树逐文件比较，仅本人报告不同，测试/工具/矩阵文档未变。
之后补记仅更新本人报告与 WSL 环境依赖说明，PR 正文分别维护代码 HEAD、最终文档 HEAD 及各自检查范围。

- `python tools/fixtures/backup-recovery/wsl_test.py --run --suite workspace --ref f5e4cb8cfc886828e43394b016a3df9d9834e2e6`：最终返回0、passed=true。实际 `cargo test --workspace --all-features --locked --offline` 退出0，Cargo结果共621通过/0失败/2标记忽略；其中 recovery 接入完整再跑十五场景通过。`cargo clippy --workspace --all-targets --all-features --locked --offline -- -D warnings` 与 fmt 均0。
- 两个 ignored 分列：C06 import_worker 是由父场景实际调用的专用子进程入口，不能算父测试未跑；forwarder 专用真实 OS ENOSPC 仍需独立有限 tmpfs 脚本，本轮未执行，不计通过。Root安装/系统服务/签名专项、ARM及实机验证未借此填成功。
- 原生音频同一完整提交首次 `--suite native --fetch` 依赖准备达到600秒边界，退出124、passed=false、未编译测试；Windows代理下载仅缺少的9个原锁文件包并逐项校验后导入。随后离线编译明确缺少CMake，test/Clippy均101、未执行测试；这轮失败原样保留，没有改源码或提高限额。
- 安装 Ubuntu 官方 CMake3.28.3 后，`python tools/fixtures/backup-recovery/wsl_test.py --run --suite native --ref f5e4cb8cfc886828e43394b016a3df9d9834e2e6`：最终0、passed=true。实际 native Cargo 库23项+bin10项，共33通过/0失败/0忽略，全部targets严格Clippy和fmt均0。这是主机原生/回环验证，不是实际ALSA设备、电话或ARM测试。
- 实际默认 `wsl -d Ubuntu-24.04 --exec id` 为UID/GID1000；测试目录 stat 文件系统类型 ext2/ext3，实际4 CPU。所有恢复数据库/TMPDIR/target位于Linux原生用户目录，未用Windows挂载盘的SQLite权限行为冒充。
- 已生成 `C09-C12-WSL2-validation-f5e4cb8.zip`，61,054字节，SHA256 `34c08ec07f7dadee20bacba4f04adf15eff4e05eec98aca58a85fb24614460d8`。六轮来源/summary/步骤日志及真实失败全部保留、ZIP CRC通过；不含源码归档、数据库、密钥、工作区或缓存。经当前聊天供用户转交，不声称已向负责人代发。
- 当前代码 HEAD 的 GitHub CI40630396003 仍因账号付款/额度未启动，已回读test检查注释；WSL通过不表示Actions/账单已恢复，也不冒称完成整套CI的Root专项。负责人可以按本机证据核对代码，成员不绕过保护或自行合并。

C09盘点、C10/C11完整合成矩阵及C12离线文件/报告交付已具备真实Linux证据，主线逐HEAD审核/合并及真实设备恢复保持待验。
四项生产缺口未修复；没有将测试观察器当作新生产恢复器。后续Linux测试默认复用已验证的本机WSL2入口，
格式或依赖变化时按实际失败修正环境/范围内测试，不能退回Windows跳过结果报成功。整体固定里程碑仍约 **80%**。


### 2026-10-28 · 按负责人 PR #10 审查接入主线 #14

审查文档 `docs/team/REVIEW-PR-10-45c54d4.md` 对旧45c54d4提出需修改；此前本机f5e4cb8结果属于旧6f228fd基线，不能替代本轮新源码验证。
本轮正常合并主线 `516228a73a7de1fa4a8a2dac40ca9f57e96a07ca`，保留双方提交及C15已合并交付记录；不修改他人生产实现或全局台账。

- F1：主线PR #14已把新备份发布为DELETE journal单文件。撤回旧基线临时允许空WAL/有界SHM的夹具助手，三处来源检查恢复完整目录/摘要严格相等；不是忽略sidecar。旧WAL格式备份的限制另记，不把历史f5通过外推到只读介质。
- F2：发布后先检查单文件，再进行immutable离线观察；活动业务库仍正常WAL感知读取，不改产品打开方式。
- F3：未来schema负例改为99，并先读取合法备份当前版本断言小于99，避免主线升级后负例失效。
- F4：拒绝来源后明确断言pending标记状态：缺文件在标记前拒绝，其余无效数据库留下隔离标记且普通open拒绝，不以条件断言跳过。
- C15已由负责人合并PR #13，仅草稿离线范围通过。手册过期Linux措辞由后续C16按真实证据刷新，本PR不扩写正式发布支持声明。

本轮合并与修改的精确提交、Linux实测退出码及剩余项待执行后补记；没有宣称旧审查已批准新HEAD。真实恢复/设备/只读介质/硬件掉电仍待主线，整体固定里程碑仍 **80%**。

#### 接入主线后的精确源码及实际复测（本节为最新结果）

代码完整HEAD：`c9aed5f4ef3c84fec92b7ac4dd2f297fa70910a8`；tree：`e7edac9490d46994135de47811f981e5f171edab`。
它是正常双亲合并，双亲为本人旧HEAD `8fa23619a868d0ab280bf5f80205a774bdd37964` 与主线 `516228a73a7de1fa4a8a2dac40ca9f57e96a07ca`，原yh1030和主线作者/历史均保留。
相对该主线只有本批13个测试/工具/文档路径；C15报告两边追加均完整保留，没有修改他人生产逻辑或全局台账。
之前45c需修改、旧基线临时辅助文件断言和f5实测均是历史截面，已由本节的主线#14修复及严格断言复测取代，不能混作同一源码证据。

| 精确来源 / 实际命令 | 本轮结果 |
|---|---|
| tree e7edac9：`python tools/fixtures/backup-recovery/wsl_test.py --run --suite recovery --fetch --ref e7edac9490d46994135de47811f981e5f171edab` | 退出0、passed=true；四项元数据单测、直接15个业务场景零忽略、recovery库4项+接入1项（内部再跑同15场景）、fmt、包全targets严格Clippy全部0；直接矩阵65.878秒、Cargo121.011秒、Clippy44.944秒 |
| commit c9aed5f：`python tools/fixtures/backup-recovery/wsl_test.py --run --suite workspace --ref c9aed5f4ef3c84fec92b7ac4dd2f297fa70910a8` | 退出0、passed=true；虚拟workspace根的实际`cargo test --all-features --locked --offline`共653通过/0失败/4标记忽略，包含完整recovery接入；`cargo clippy --all-targets --all-features --locked --offline -- -D warnings`及fmt均0，Clippy78.325秒 |
| Windows静态/控制器 | rustfmt及git diff检查0；四项控制器首次因默认临时目录ACL失败，未改产品/单测，指定本仓库私有临时目录后4通过/0失败。不能把这次Windows结果代替Linux执行 |
| 手册结构回归 | `python docs/manuals/check-links.py`退出0：7页、117个本地链接/锚点、16个证据ID，无错误。没有改已合并的C15手册或把结构检查称为实机验收 |

两轮均为WSL2 Ubuntu-24.04、普通UID1000、Rust1.98.0、813个公开/可交付源码文件，源码/TMPDIR/target均在Linux原生用户目录。
e7edac9树与发布代码c9aed5f的tree完全一致；树验证没有冒充提交，提交另跑了整仓。
入口在虚拟workspace根选择全部成员，日志中的Cargo命令没有显式`--workspace`；此前报告用该选项描述整仓范围，本轮按日志记录实际参数，不修改历史日志。

4个ignored逐项说明：C06 import_worker由父测试实际调用；forwarder真实OS ENOSPC需要独立有界tmpfs脚本，本轮未执行；
主线新增的两个actual_root_capture测试需要隔离Root/CAP_SETUID/CAP_SETGID及CAP_KILL正/负路径，本轮未执行，不算通过。
原生音频33项及严格检查的历史实测来源仍为f5e4cb8。逐文件核对native（含独立Cargo.lock）、media及auth/policy/wire/domain路径、工具链在c9均未变，本轮未重复跑native，不冒称c9存在新的原生运行记录。
既有proc-macro-error2未来兼容提示保留，没有升级依赖或把提示隐藏。

- 新源码复核包`C09-C12-Linux-c9aed5f.zip`：2,490,677字节，SHA256 `bf44a95c348811b1ffd699a745e9d640c3df152aa8f8a6398abf97f291d136f5`；813个文件逐个与c9 Git blob相同，SOURCE.json含完整HEAD/tree/归档摘要，外层`--verify-only`和ZIP CRC通过。该检查只证明打包完整性；旧bfaa8dc源码包及11/4失败保留为历史，不能继续把旧包当已修正代码。
- 新结果包`C09-C12-WSL2-validation-c9aed5f.zip`：35,967字节，SHA256 `655ec8f6d9cad8611de9da7283e27d3bde5b6fbc4a66c2d453edcedb3ebe53aa`；包含上述两轮来源/summary/日志、真实命令/退出码/实耗和历史包索引，CRC通过。不含源码、数据库、密钥、工作区或缓存。两包仅在本机准备，经聊天供用户转交，未代发负责人。
- 代码c9的[Actions run40634706914/test122638880008](https://github.com/Viper-Boss/RustSimHub/actions/runs/40634706914/job/122638880008)实际检查注释仍为付款失败/费用额度，任务未启动；本机通过不表示Actions或完整Root专项/ARM/设备CI成功。
- PR #10回读为Ready、open、未合并、无代码合并冲突；需负责人按新HEAD重新审查，旧45c的需修改结论不自行改成已批准。最终报告补记提交的完整HEAD由PR正文单列，若只改本报告则与c9代码逐文件等同，不重复运行相同代码的全套测试。

实际完成：F1～F4范围内修改、主线接入、C09盘点/C10-C11真实矩阵/C12离线交付和默认WSL2执行流程；无设备/真实通信/普通蜂窝/部署签名/分区操作。
剩余：跨域一致切面与联合恢复协议、旧WAL/只读介质/恶意旁路日志准入、真实恢复/硬件掉电、上述专用测试及主线精确HEAD复审/合并。
下一步按任务书串行规则等待当前包复审，期间已只读盘点C13/C14现有Store接口/重复覆盖与实际IO失败方案；未将C13/C14/C16～30标完成或另造未批准格式。
整体固定P0～P7仍 **80%**，成员C不更新全局验收/面板或代负责人合并。

#### 2026-10-28 · WSL2隔离专用边界补验（更新上述未运行项）

源码仍为完整代码HEAD `c9aed5f4ef3c84fec92b7ac4dd2f297fa70910a8`、tree `e7edac9490d46994135de47811f981e5f171edab`；不改生产代码或测试断言。
对之前同一WSL2原生副本的813个源码文件逐个与原Git归档字节相等核验后，普通UID1000分别运行
`cargo test -p rustsimhub-forwarder --lib --all-features --locked --offline --no-run --message-format=json`、
`cargo test -p rustsimhub-update --lib --all-features --locked --offline --no-run --message-format=json`，各取得唯一精确Cargo测试ELF及摘要，再运行主线既有夹具。
测试所需Root仅用于独立WSL网络/挂载隔离；未给rsh-test授予sudo，没有设备、生产秘密、外网或部署。

| 既有脚本 / 实际边界 | 结果 |
|---|---|
| `python3 -I tools/test-forwarding-storage-full.py --cargo-artifacts <forwarder-artifacts.json>` | 退出0；独立mount/net命名空间，2,097,152字节tmpfs，实际测试UID65534/NoNewPrivileges，真实errno28；Unknown和游标保留、释放空间后只恢复一次，1通过/0失败/0忽略。其余92个被精确筛选的用例没有借此计通过；退出正常卸载，不填宿主磁盘 |
| `unshare --net setpriv --no-new-privs python3 -I tools/test-metadata-process.py --cargo-artifacts <update-artifacts.json>` | 退出0；合成ELF实际UID/GID986、附加组与环境清空，密封执行、3秒/16KiB预算验证，1个指定Root场景通过 |
| `unshare --net setpriv --bounding-set=-kill --no-new-privs python3 -I tools/test-metadata-process.py --cargo-artifacts <update-artifacts.json> --missing-kill` | 退出0；保留SETUID/SETGID但移除CAP_KILL，在启动降权子进程前拒绝，1个指定Root场景通过；没有把未启动子进程的UID或输出伪填成功 |

各步由空环境和固定PATH执行。Linux侧源码/产物准备加三项专项全部0、最终passed=true；不是四个新业务测试。
普通整仓653通过/0失败/4标记忽略的原始结果保持不变：C06 worker已由父测试调用，另外三项现在有独立实际通过证据，不再处于未运行。
这没有补齐其余Root/服务协调CI、生产ARM或真实恢复；原生33项仍是已说明的历史源码等同证据。

新增仅日志证据包`C09-C12-WSL2-special-c9aed5f.zip`，26,775字节，SHA256 `793ee1383d626a3f989e165bf15bc3ca5949fab9574f58a415aabf2dabf40942`。
包含来源/summary/四步日志/两份Cargo产物清单与逐文件原始/共享摘要，ZIP CRC及仅允许文件清单检查通过；没有ELF、源码、数据库、密钥、工作区或缓存。
这是明确标注的路径脱敏副本，仅将Windows用户目录中的结果/源码归档/执行器路径替换为固定占位符，真实测试输出、退出码、摘要和实耗不改。
首次打包的路径检查发现执行器路径尚未替换而拒绝，该不完整包仅在本机标为失败保留；补齐该路径脱敏后再生成上述通过检查的包，未把打包检查失败称为业务失败或删除原始证据。
包在本机准备供用户转交，未代发负责人。WSL说明补上现有专项的复现方法；本次只改说明和本人报告，与c9产品/测试/工具代码一致，不重复跑相同代码全套。

当前PR #10仍等待负责人按最终HEAD复审/合并；未自行改审核结论、全局验收或开启下一修改包。C13/C14继续只读预研；任务书要求当前包审核后串行接续，实际接口/Root授权方案仍由主线收口。
项目整体按固定P0～P7仍 **80%**，上述隔离通过不替代实机恢复、只读介质、恶意WAL准入或硬件掉电验收。

## 2026-10-29 · C13/C14阶段一接口与最小失败样例提案

上一包PR #10已由Viper-Boss于2026-10-29T14:50:30Z合并，被合并完整HEAD为
`b66c0e1df606980524d94fb26585b4a73aa7f7e3`，merge/main为
`e48072ba9760c18f10c44847a1c6d94863eafd91`。主线reviews.json当前仍记录旧45c的needs_changes，
本成员不据合并自行改审核数据、全局进度或已验收步数。

本包起始main：`e48072ba9760c18f10c44847a1c6d94863eafd91`；
接口提案内容完整提交：`b36a4d9019f2f0f729c18b3dff49074b53773a25`。
本报告随后独立补记，最终PR完整HEAD在PR正文及Git记录中回读，不能将起始main冒称为本包HEAD。
分支：`codex/member-c-update-failures`，仓库`Viper-Boss/RustSimHub`，同仓库PR到main，不fork/不自合并。

依任务书T6“新包第一份小PR只固定接口/来源/失败样例，主线审核后再实现其余代码”，
本包只新增[接口提案](../update/failure-matrix-proposal.md)并追加本报告：

- 真实Store公开接口及完成快照/台账/候选选择的观察边界；7类候选用例、5个详细最小失败样例方案。
- C14有界tmpfs、异常退出及真实兼容模块的后续边界；精确persist内部同步点待主线审核。
- R1错误后同句柄复用、R2运行链接发布失败后的attempt消耗、重复确认的增量与最小cfg(test)接入决策。
- 现有11个updater文件与原只读预研基线dfd11e9逐字节一致；源码链接固定到本包起始main。

真实验证：执行`python -X utf8 .local/wsl-setup/prepare-c13-interface-pr.py`退出0，
它是本机私有文档整理工具，不是产品或新增测试驱动；固定Git对象11文件摘要核对通过。
随后执行本机Python文档检查：18处固定源码/相对引用可解析、7类case齐备、Markdown尾空白和公开信息检查通过；
`git diff --check`退出0。私有准备脚本不提交，提案保留实际接口与许可来源。

本包没有编写/执行新Rust/Cargo/WSL业务用例，尚无C13/C14通过数量；
没有改生产Store/admission/schema/签名/Root单元/锁文件、其他成员或设备。
既有C09～12和C17的WSL结果归属各自固定源码，不借入本包。

未实现/未验收：真实写失败矩阵、tmpfs ENOSPC、精确子进程退出与重开、联合兼容/回执证据；
普通UID夹具不代替Root准入、ARM或设备掉电/恢复验收。合成密钥仅在后续临时夹具生成，
合成ARM头不执行，不含生产密钥/数据库/硬件日志。

下一步由负责人审核本接口小PR，确认上述决策点，再串行实现真实测试与WSL驱动。
提案先审不是C13任务完成声明；C15已合并草稿与后续C17～30均不借此计完成。
源码按本项目AGPL-3.0-only等各文件SPDX保留；Codex辅助盘点与整理，未新增依赖，工时无本人自报、不估造。
整体固定P0～P7仍80%。

## 2026-10-30 · C13真实Store失败矩阵实现与WSL实测

PR #81已由负责人合并（merge c65cd256e699636eab5b58c656c2aee7cd775953），
本实现起始main完整SHA为`b6c73c6a47e00654bff51ae2b221fdcd7e9458e0`；
本人同仓库分支`codex/member-c-update-failure-tests`，PR到main由负责人审核，不自行合并。
精确实测代码提交：`438fe6e4a61ca7bc1686235720c9c78e98b3fc01`，
tree `fe4f25929c57afc3562797a057cea1af075f25c7`。
实际测试先按暂存tree归档运行，随后发布的代码提交树与该tree完全相等；
本报告/说明随后补记，最终PR完整HEAD列于PR正文，不将后续文档提交冒称为另一轮代码执行。

实现文件：crates/update/tests/failure_matrix.rs、tools/test-update-failure-matrix.py、
本人既有tools/fixtures/backup-recovery/wsl_test.py增加update suite；说明见
[C13矩阵](../update/failure-matrix.md)。仅公开Store API，无生产hook或依赖变化。
7个Rust测试分别验证目录真实EACCES、stage/choose/confirm的state.next冲突、
运行链接发布EACCES、重复确认、有效但低floor/异key的next拒绝；每项覆盖format1/2。
合成包验签/载荷由原更新器完成，合成ARM头不执行。独立可写对照验证正常stage/choose/confirm，
权限负例先用真实probe核对errno13，Store错误仍只称Io；孤儿组和原state保持证据。

真实命令：
`python tools/fixtures/backup-recovery/wsl_test.py --run --suite update --ref fe4f25929c57afc3562797a057cea1af075f25c7`。
WSL2 Ubuntu-24.04、内核6.18.54.1-microsoft-standard-WSL2、普通UID1000、Rust1.98.0，
904个源码文件由Git归档到Linux原生source/TMPDIR/target；依赖锁定且离线。
北京时间2026-10-30 00:23:04至00:24:40，元数据/依赖/格式、直接矩阵、Cargo包测试、严格Clippy全部退出0，passed=true。
直接矩阵7通过/0失败/0忽略；Cargo库42通过/0失败/2个既有Root metadata专项ignored，
新集成7通过/0失败/0忽略；主程序和doc测试无用例。严格Clippy覆盖包all-targets、-D warnings。
同7项经驱动与Cargo各运行一次，不能计成14项新测试；参数组合不折算成验收步数。
两项Root专项本轮未运行，未借用旧c9结果伪称本HEAD通过。
Windows上两个入口不带--run均实际返回execute=false计划，未启动Linux或Cargo。

实际观察：P02/P03/P04证实当前错误返回后内存与磁盘可不同，测试丢弃句柄再重开，
不把status当持久成功；P05证实运行链接创建失败前attempt已经持久化，重开回退A1且floor不降。
本包保留这些生产行为，不擅自更改错误后句柄契约或试运行预算。

仅日志包C13-WSL2-validation-fe4f259.zip：7766字节，SHA256
`59de95aea9217edbde1591ead0ba2658f06fe4735acacdd895c443b39f6798d7`，CRC通过；
只含来源/summary和步骤日志，不含源码、ELF、密钥、数据库或缓存。本机保留供用户转交，未代发负责人。
源码固定tree、代码提交与文件范围检查后提交PR，完整HEAD由PR回读。

未验收：C14 tmpfs真实ENOSPC、精确子进程退出/内部persist同步点、联合schema/outbox/回执矩阵，
以及Root/ARM/真实升级、硬件掉电/fsync与设备恢复。没有设备、生产数据、Root安装、实际通信或签名发布。
C13本包为已实现且本机自测通过，仍待负责人审核/合并；不自动宣布C13全范围验收或C14完成。
项目AGPL-3.0-only/原SPDX保留；Codex辅助实现和核对，无新增依赖，无本人自报有效工时，不估造。
整体按固定P0～P7仍80%。

## 2026-10-30 · C14真实ENOSPC、异常退出与兼容/回执

C13的PR #85已由Viper-Boss合并，merge ce36d37d0358a5c5d6e2413602c212ea68f757a4。
本包起始main完整SHA：`5238670246e7c6e43184534a7a6ee4b13c293ffd`，
本人同仓库分支`codex/member-c-update-crash-tests`，只交当前C14修改包到main，不自合并。
精确代码提交：`8fb320942c635331c50291f8d4617cd861f3d179`，
tree `d51542ffa6f47440418b1e15e4b4e8849126c961`。
普通和tmpfs专项先按该固定tree运行，后续发布代码提交树完全一致；本报告/说明随后补记，
最终PR完整HEAD列于PR正文，不冒称文档提交另有一轮业务执行。

只改分配范围：新crash_matrix集成测试及c14_support合成包模块、新有限tmpfs执行器、
已有C13驱动/WSL入口接入C14、[C14说明](../update/crash-matrix.md)和本人报告。
不改生产Store、schema、签名、Root单元/账户、锁文件、他人代码或全局台账。
核心失败逻辑全部由原Store、Queue、inspect_outbox、check_upgrade处理，没有自建替代更新器或新增生产hook。

普通实测命令：`python tools/fixtures/backup-recovery/wsl_test.py --run --suite update --ref d51542ffa6f47440418b1e15e4b4e8849126c961`。
WSL2 Ubuntu-24.04、内核6.18.54.1-microsoft-standard-WSL2，UID1000、Rust1.98.0，
926个固定Git源码文件、Linux原生source/TMPDIR/target、锁定离线依赖。
北京时间2026-10-30 00:53:22至00:55:29，C13直接回归7/0/0、C14普通直接4/0/0，
Cargo库42通过/0失败/2个既有Root metadata ignored，crash集成4通过/0失败/2个专项/worker ignored，
C13集成7/0/0；fmt和包all-targets严格Clippy全部退出0，passed=true。
同病例由驱动和Cargo重复执行不增加新增数量：本包4项普通测试，另1项有界tmpfs专项，worker只供父测试调用。

SIGKILL实际定位为原stage成功返回后、原choose成功返回未confirm后。每项format1/2各一次，
父进程核对phase/hash握手、精确子PID、实际signal9及wait，然后原Store重开。
直接矩阵实际4次终止，Cargo接入重复4次；原组内容保持、候选只试一次/回退、floor2不降。
签名/载荷/schema用独立合成输入，真实API分别拒绝且台账不变。
outbox由原Queue公开API创建真实Unknown及原attempt；只读inspect和真实数据版本比较拒绝投影后旧fallback，
实际库/业务库比较前后保持，正常Queue迁移重开后原回执、payload、attempt和游标保持，
显式打开合成策略仍不重新claim Unknown。程序metadata为合成比较输入，authorization_verified=false，
不是已验签程序的真实build-info或Root维护准入证明，也未通过网络发送任何请求。

tmpfs专项在同一原生源码先以UID1000离线生成crash_matrix Cargo产物清单，逐文件重核926个源码与原Git归档。
随后WSL root只调用`python3 -B -I tools/fixtures/backup-recovery/update_full.py --cargo-artifacts <本轮实际清单>`，
完整路径保留本机原记录，共享包将两项Windows输入路径显式替换为占位并记录原始/共享SHA。
北京时间00:56:21至00:56:28，两步退出0；新mount/net命名空间传播private、2097152字节tmpfs、nosuid/nodev/noexec。
实际测试UID65534、有效/许可/ambient能力零、NoNewPrivileges=1；两种包格式都实际写到filler errno28、可用空间0，
原stage返回Io，原state/A1和floor保持；释放的仅filler，重开仍选A1，半成品未删除规避；独立可写对照通过。
单列Rust专项1通过/0失败/0忽略，正常卸载后才报告tmpfs_unmounted=true。
本轮测试ELF SHA256 `b92214b16daa000bf2e745fd226b00be67364c3ca41c30abccde69c4228d1434`。

前两轮普通树93446f8、a91a1feb均通过；第一轮tmpfs也通过。补强迁移重开后的payload/attempt断言，
再收紧产物清单读取上限及成功报告卸载边界后，按上述最后d515树全部重跑；旧来源/日志保留，不混算执行。
Python三文件语法、默认无运行计划、说明链接和git diff --check另核对，无新增依赖/真实设备或通信数据。

共享日志包C14-WSL2-validation-d51542f.zip：13463字节，SHA256
`e48730d8fd8f4b066b292602bc7019db9b4d6fbbe3d3d592c12af86ec14b39fb`，17条目、CRC/逐文件摘要通过。
只含来源/summary/脱敏日志和原始/共享SHA索引，不含源码、ELF、密钥、数据库或缓存；本机保存供用户转交，未代发负责人。

普通Cargo原始ignored统计保留：worker已由父测试实际调用，tmpfs已单独实跑；两个既有Root metadata专项本轮未跑。
未验收persist内部write/sync/rename精确中断、eMMC/硬件掉电/fsync断电保证、Root/ARM或实际设备升级恢复；
没有新增内部hook，不把API返回后的SIGKILL冒充内部故障命中，不把tmpfs当物理磁盘证据。
下一步按最终完整HEAD提交负责人审核；本包离线自测通过不等于C14全范围自验收。
AGPL-3.0-only和复用源SPDX保留，Codex辅助实现/核对，工时没有本人自报、不估造；整体固定P0～P7仍80%。
## 2026-10-30 · C16 第四批综合交接与手册复核

成员 yh1030 / member-c。C13 PR #85、C14 PR #86 均已由 Viper-Boss 合并后，
从主线 `6b5de5f3ca5ff706adacd8f25ed7f638546739a9` 建立本人分支
`codex/member-c-manual-delivery`。C14 被合并 HEAD 为 `b625bc8dff65104dd3e147dfe622ea0fe17e4866`，
merge `81a822e3f916dd1324e2cac24145d3f426159807`，北京时间 2026-10-30 03:29:07；没有新增评论或文字修改要求。

本批实际校验的手册内容提交为 `344acacb5bee5c936036e8a425381c0fbd016d3f`，
其 tree 为 `7a453a9b461a03d4e0bda8e023b82601d1f7e680`；本报告另行补记，最终完整 HEAD 在 PR 正文回读。
没有将随后报告提交冒称为新的业务测试源码或运行记录。

### 修改范围与交付内容

- 仅五个 `docs/manuals/` 文件及本人报告：README、support-evidence、backup-and-maintenance、getting-started、新增 c16-handoff。
- 修正 C15 审核指出的 C09～C12“仍待 Linux 结果”过时文字，按 c9 真实复测与 PR #10 合并事实引用；保留旧 bfaa8dc 的 11/4 失败历史，不覆盖或重写原报告。
- S11/S16 分别补充主线 UPD-01/04 的有限实机记录和本人 C13/C14 隔离证据；U3 当时未接线不再冒充当前状态，UPD-02 发起授权仍明确未由比较结果授予。
- 修正日常登录入口文字，限定旧临时设置码首配流程适用范围；新版用户名密码/开放热点、最终地址与证书交付仍由负责人提供版本匹配指引。
- C16 交接页逐项列 C13～C16 的精确来源、错误后句柄/磁盘差异、格式与 Unknown 保持、未运行项及主线发行条件。七页原手册加一页交接不改变任务分母或项目百分比。

### 实际命令、环境与结果

| 检查 | 实际结果 |
|---|---|
| Windows：`python -X utf8 docs/manuals/check-links.py` | 退出0：8页、139个本地文件/锚点、16个定义且引用的支持ID、零错误；4个外部PR链接仅列出，不联网验证 |
| WSL2：固定 tree 7a453a9 的 Git archive，普通 UID1000，Ubuntu-24.04，kernel 6.18.54.1-microsoft-standard-WSL2，Python3.12.3；Linux 原生副本中 `python3 -B docs/manuals/check-links.py` | 退出0，8页/139链接/16 ID/零错误；51个公开源码/显式链接目标逐个SHA256与Git归档相等，校验后源码未改变；归档SHA256 `b02ec79656b8b897ab8a1a9f4de2415edfbc7a9d31b47631811bcbe5517a28bb` |
| 本地文档预览：`python render-c16.py --repo SOURCE --out PREVIEW --tree 7a453a9b461a03d4e0bda8e023b82601d1f7e680` | 退出0；使用已安装的外部复核工具 Python-Markdown3.10.2，八页输入的规范UTF-8字节与固定Git tree逐页一致，生成八页离线HTML；不新增项目依赖 |
| 无头 Edge：`node capture-c16.cjs PREVIEW` | 退出0，Edge156.0.4314.50；1440×1000及390×844各8页、共16张完整截图；全部无横向溢出、无脚本错误、无外网请求、所有预览本地导航目标存在；离线context和外网route拒绝保持 |
| 目视抽查 | 实际查看手机目录、手机支持索引、桌面C16交接和桌面维护页；中文/表格/草稿标记及段落可见。自动检查覆盖全部16张，目视抽查不冒称全部真实设备/手机测试 |
| `git diff --check` | 退出0；无生产代码变化，不为文档修改重复跑已通过的整仓业务测试 |

Windows初次编辑检查发现新增交接页尚未建立及一个不匹配的报告锚点，修正为现有本地目标后上述最终检查通过。
第一次截图尝试的默认Playwright浏览器未安装；改用本机已有Edge，受限环境启动退出，随后在获准的隔离无头会话完成。
这些是文档/工具准备失败，不是产品测试失败；未下载浏览器、读取日常浏览器资料或重启设备。

### 证据包与边界

`C16-manual-validation-7a453a9.zip`：4,479,429字节，32个条目，SHA256
`e1ebab1896619e8062bd7834d946b80dda7cf7a665a1f8e08daee29432fd1d76`。
包含八页HTML、16张PNG、固定来源摘要、真实浏览器/WSL检查输出、两份本人复核脚本、说明和逐文件摘要；CRC及逐条内容核对通过。
脚本只作为证据包中的外部文档复核工具，不接产品入口；不分发第三方库或新增项目依赖。
包内无用户目录、私人维护路径、真实设备地址、凭据、数据库、ELF、固件或原始通信数据。
仅本机准备并向用户提供，未代发负责人。

实际完成：C16文件、手册状态更新、本地链接与离线渲染校验、截图和第四批综合交接。
待负责人：按最终HEAD复审；正式REL-01支持/发行/签名、最终地址证书和恢复升级入口、真实手机及设备补验。
GitHub CI过去因账单/额度未启动；本批是否启动以新PR实际状态为准，本地检查不称Actions通过。
本批没有访问设备/用户数据、通信、生产签名、安装、Root产品或分区操作，也没有更新全局看板/审核台账。
来源为项目既有公开源码与验收记录，项目AGPL-3.0-only保持；AI（Codex）辅助整理、命令执行和目视抽查，无作者重写，自报实耗未填写。

主线集成：先审六个范围内文档，再复核证据包/当前手册链接；撤回本PR仅影响文档，不改变任何设备状态。
下一步：本PR审核后串行推进C17～C20的来源/输入接口第一阶段；未获该阶段审核前不实现其余检查器代码。
原固定P0～P7整体仍约 **80%**；C16自测和合并不自行等同正式发行或全部实机验收。
## 2026-11-01 · C18 有界只读制品检查器交付

成员 yh1030 / member-c，分支 `codex/member-c-artifact-checker`，同仓库PR到main。
C17提案PR #99已由负责人合并；本轮从最新主线
`ce9ccb8debb61db49860fe795c1f3e471b749f4c` 开发。
实际测试内容提交 `0608ae73b79d619342c8a68b24c6a89ec509f2be`，tree
`e096a66df63dc28d56187fa5ab6209a2029c5ab6`。随后只补本报告，最终完整HEAD在PR正文回读。

### 实现文件与行为

本包只改六个范围内文件：新增 `tools/kernel-artifact-audit/audit.py`、`test_audit.py`，
新增 `docs/kernel-artifact-audit/usage.md`、`pinned-input.json`，README补入口及本人报告本节。
没有改既有锁/来源索引/固件/重建器/原校验器、生产Rust/Root、其他成员或全局台账。

- 默认只输出execute=false计划，不读取用户清单/制品或写报告；实际检查要求普通Linux UID，使用WSL2原生隔离目录。
- pinned模式只接受C17 RustSimHub来源截面及显式24对象路径映射；从7个经C17索引摘要核对的原锁/解析器字节推导Image、音频DTB及22模块期望，调用方不能替换hash/版本。不是另一套发布manifest。
- synthetic模式严格版本/字段/类型，四类必需输入缺项仍为缺口；所有输入都标合成，不把自报passed/healthy当证据。
- 实际读取并核对长度/hash、ARM64小端Image头、FDT v17根compatible、现有真实audio_modules.modinfo解析的ARM64 ET_REL/name/srcversion/vermagic/depends；模块名称规范化、依赖缺口和环分别记录。
- 逐条输出observed/mismatch/not_verified；缺证据退出3，明确不匹配退出4，非法输入退出1；有限reason不回显主机路径/原始异常。执行完整的字节检查可以退出0，但永远不授予安装/confirm/发行资格。
- 保留stock 681模块、APR等外部依赖、厂商固件摘要/许可缺口。即使固定24项全匹配，当前pinned模式也仍应退出3；synthetic的许可声明不升级成已验证许可。
- Linux目录逐组件openat/O_NOFOLLOW，单链接普通文件、预算及读前/后稳定性检查；拒绝路径逃逸、链接/设备/FIFO、重复/别名冲突、未知版本和超限。不扫描维护材料或执行制品/脚本，不运行编译器、modprobe/depmod或网络/设备命令。
- 清单1MiB/2048项，累计1GiB；Image文件及有效内存范围各128MiB、DTB128KiB、模块/固件各2MiB；FDT token8192/深度64。范围扩展需要另审，不默默放宽。
- 可选报告仅在制品目录外新建0600/O_EXCL文件，目标必须不存在；不覆盖输入、清单、旧报告或其他证据。

使用说明明确区分Image头的有效内存范围与文件长度、原始Image与boot/压缩归档、声明release与实际模块vermagic。
格式依据在usage.md链接Linux ARM64头与DTSpec原始文档；仅有限静态结构观察，不等于完整ELF/FDT语义、真实启动或硬件兼容证明。

### 实际验证命令与结果

| 检查 | 实际结果 |
|---|---|
| Windows：`python tools/kernel-artifact-audit/audit.py` | 退出0，execute=false；没有输入读取/制品检查或报告创建 |
| 最终固定Git tree归档 → WSL2 Ubuntu-24.04、普通UID1000、Linux原生副本：`python3 -B tools/kernel-artifact-audit/test_audit.py` | 退出0，21个测试通过/0失败/0错误/0跳过；真实audit.main/inspect和原modinfo解析器，纯合成文件；实际进程0.853秒 |
| 源码/归档 | 41个公开文件逐个与Git归档字节一致，执行后未改变；归档SHA256 `d1aa6a7cfa53fde97213e89855330daf793f5d4375387ee0c50962556fe7ad81`；Python3.12.3，kernel 6.18.54.1-microsoft-standard-WSL2 |
| Python AST / 文档链接 / Git检查 | 两个新增Python文件AST通过；19个文档链接已清点、本地目标存在，外部格式文档另已读取核对；`git diff --cached --check`通过。不是19个外网或设备验收 |

21项覆盖合成完整匹配、空/缺文件、hash变化、hash自洽但错板DTB、坏Image/FDT/ELF、Image有效内存与文件长度区别、
schema/额外字段/重复JSON/非法类型、路径/大小写别名、文件及父目录symlink/hardlink/FIFO、单文件/清单/条目及累计预算、
真实模块元数据/release、依赖缺口/规范化正向链/环、固件许可缺口、报告新建与拒绝覆盖、pinned原锁派生及公开24映射示例。
测试只调用现有生产解析器和真实新检查器，Fake仅构造输入，不以另一个检查器产生预期成功。

早期16/19/20项迭代轮次也通过；复核修正Image内存范围语义、有限错误原因及边界后才执行上述最终21项，轮次不相加计数。
一次Windows私有执行器生成命令因PowerShell引号解析失败，没有运行Linux测试；改为保存明确脚本后运行成功，未改产品断言掩盖失败。
本轮没有改Rust，不为Python/文档交付重复运行Cargo/Clippy/Rustfmt或既有未变的13项内核测试；不将旧结果标成新HEAD实测。

### 证据、未验收及集成

仅日志/摘要包 `C18-WSL2-validation-e096a66.zip`：4,680字节、5条目，SHA256
`e2076df65442b0d85b4af9d6af9aee56e6471b90a3e522b7be699168712cede8`；CRC及逐项内容核对通过。
含实际summary/逐项日志、41文件摘要和命令说明；无源码归档、ELF/固件、数据库、私有路径、凭据或用户资料。
本机准备向用户交付，未代发负责人。

实际完成：C18只读实现、两种输入模式、显式缺口/拒绝与受限制品观察、必要隔离验证、用法/映射示例及本报告。
未验收：负责人精确HEAD审核；真实完整内核源码归档/boot/681原装和22音频模块字节、厂商固件实际许可、Root/ARM加载、实机启动/恢复/掉电、签名发行。
C19完整逐样本矩阵/C20后续交付与主线补验另记，不按本轮单测数量直接标完成。
报告始终保留firmware_license/boot/recovery/release_support为not_verified及authorization_verified=false。

来源/许可：项目原锁和现有GPL-2.0-only audio_modules解析器按固定摘要复用，未改原文件/SPDX；
新Python脚本按项目AGPL-3.0-only，合成ELF布局沿用项目既有test_audio_modules方法；无新第三方依赖、许可推断或厂商二进制分发。
Codex辅助实现/阅读/复核，未改作者，成员自报工时未填写。
主线按本PR六文件和精确HEAD审阅；撤回只移除新增工具/说明及本节，不改变业务或设备状态。
下一步按审核意见修正后再开展C19/C20；主线维护正式支持/全局验收。整体固定P0～P7仍约 **80%**。
