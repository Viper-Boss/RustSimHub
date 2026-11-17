# A3 NAS 接线修复与短信失败边界交付

日期2026-11-17（Asia/Shanghai），账号yh1030；本人同仓库分支`codex/offline-a3-business-20261117` → main，交负责人审核，不自行合并。
起始main `de663743bd726e77f7c560139d0f91ea78e8fe57`。
实现HEAD `72789c7bbaf215ef32f3970394f1d5bf0c62b056`，tree `74a4f2f614d1c1f4218123731458211121f374bb`。
测试归档SHA256 `a56ac4152657f0244c69dd111320f4d1adb6443d0566653fc91157e32931a879`。
材料为后续单独提交，完整PR HEAD在PR正文及分支回读，不循环写入材料自身。

对应A3的FWD12/SMS8/NAS8共28个原动作；[七条要求、逐动作证据及外部缺口](../offline-a/a3-business-matrix-20261117.md)。本轮可独立离线子包已交付，旧NAS页面/旧PCM来源与真实验收仍未完成；全局**81.5%**和原分母/权重保持。

## 实际改动

生产修复只有`tools/nas-adapter/control/client.go`两行：原固定origin作为Origin，加入`X-RustSimHub-Request: 2`。
旧客户端缺这两个头，真实Edge在到达Core前拒绝；旧模拟HTTP测试遗漏了原Edge门禁。没有放宽Edge/认证或扩展API。
原单元测试补头断言，新Go/Core/Edge ignored夹具实际联测TLS、原Unix IPC、Core Auth与原call actor/私有MM。

新增短信失败/视图证据：

- `storage/outgoing_recovery_tests/enospc.rs`：独立16MiB tmpfs真实填满，在prepare/begin/bind/permit/confirmation五个落盘边界取得实际SQLite Full。检查事务/游标、已有ID、重开状态及不可重复permit。prepare失败前尚无成功ID；其他阶段已有ID保持。确认后的SENT证明在该Storage测试中合成，真实sender对账另用原私有MM重启回归。
- `observer/checks/a3_sms.rs`：真实私有D-Bus返回命名的`org.freedesktop.ModemManager1.Error.MobileEquipment.MemoryFull`；Create或Send失败后原sender保持Unknown，反复Submit/Query仍Create1、Send0或1。
- `authenticated/tests/a3_sms_views.rs`：两套合成但真实Auth身份经过原IPC，分页/Unicode/二进制、Owner回收站、委派拒绝、来源换代新ID、撤权不影响另一身份、重启旧token失效/原ID保留。

其他改动仅测试module声明、测试helper可见性及上述专用夹具。未改转发、短信、Auth、Edge门禁、Core calls的生产算法，未加依赖/迁移/Root能力。
未改BLE、AirSIM/VoBLE、native、媒体声音、B工作流/发行/更新路径或全局台账，不访问设备/NAS/真实消息或签名。原SMTP/导入/PCM适配继续复用，不把Go服务称作Rust实现。

## 真实正反例

NAS负对照tree `1f7cb4905219c6a123b1110aeb056148fad57d6d`，归档SHA256 `f9da83cd23c38244739ef3079ca5cac32a0530db38d8717e80fd04a815408eb3`。
旧客户端运行原Go整链路测试退出1，实际Edge http_denied，Core转发0次；这是行为失败，没有把编译失败当负对照。
与正向tree相比还有两个独立测试夹具修正及原Go单元头断言差异；NAS三段新fixture blob逐字节一致（Core calls_nas_a3_tests、Edge tests/a3_nas、Go a3_core_linux_test）。

修复版相同NAS回归退出0，原Core/Edge各一项ignored测试显式运行通过；Core转发9次，合成MM Create1/Start1/DTMF0/Hangup1，所有子进程已退出。
客户端在读完实际Submit回复后故障注入丢弃回复并返回错误；保留原RecoveryID查询，重复原Submit不再拨号。第二Client不能使用第一Client的lease，撤权后旧token拒绝。
没有真实拨号/短信/公网请求，也不是模拟“整个旧NAS应用”。原Edge router/TLS和Unix转发实际执行，但不调用其启动AirSIM/媒体监听的完整run入口。

## 验证环境与命令

Ubuntu24.04 WSL2，Rust/Cargo1.98.0、Go1.27.3；原Cargo.lock、`--locked --offline`，Go关闭模块下载。测试source/target在Linux原生目录，归档哈希先核验。
普通回归与真实端点在仅lo的私有net/mount命名空间，降至UID1000、capabilities/bounding全0、NoNewPrivs=1；无宿主Wi-Fi/路由/DNS/防火墙更改。
ENOSPC另挂独立16MiB tmpfs（nosuid/nodev/noexec），确认实际挂载及命名空间后降权。根权限只用来建立隔离，测试本身无Root权限。

| 命令/范围 | 实际结果 |
|---|---|
| `cargo test -p rustsimhub-core --lib --bin rustsimhub-core --features product-providers-candidate,email-production-candidate --locked --offline -- --nocapture` | lib108通过/3忽略；bin239通过/4忽略，退出0 |
| Core默认相同命令去feature | lib21通过/2忽略；bin227通过/1忽略，退出0 |
| `cargo test -p rustsimhub-storage -p rustsimhub-auth -p rustsimhub-compat --locked --offline -- --nocapture` | Auth45、compat lib43/CLI9（1忽略）、Storage74（1忽略），退出0 |
| `cargo test -p rustsimhub-compat --features normal-nas-candidate --locked --offline -- --nocapture` | lib49+CLI9通过/1忽略，含原六项合成PCM契约，退出0 |
| `cargo test -p rustsimhub-forwarder --locked --offline -- --nocapture` | 127通过/1忽略（105 lib/4兼容/18邮件），退出0 |
| 同命令加`--features smtp-production-candidate` | 129通过/1忽略（108 lib/21生产邮件），退出0 |
| `cargo test -p rustsimhub-edge --locked --offline -- --nocapture` | 71通过/1专用忽略，退出0 |
| 六crate Core/Edge/Storage/Auth/Compat/Forwarder `cargo clippy --all-targets --features rustsimhub-core/product-providers-candidate,rustsimhub-core/email-production-candidate,rustsimhub-compat/normal-nas-candidate,rustsimhub-forwarder/smtp-production-candidate --locked --offline -- -D warnings` | 退出0 |
| Core默认`cargo check --no-default-features --all-targets --locked --offline`；`cargo fmt --all -- --check` | 均退出0 |
| `GO111MODULE=off GOPROXY=off GOSUMDB=off RSH_GOLDEN_FILE=<source>/crates/compat/src/legacy-golden.json go test -race -count=1 -timeout=120s -json ./tools/nas-adapter/...`；`go vet ./tools/nas-adapter/...` | 25顶层通过/1新fixture跳过，退出0；跳过不计通过，下一行显式运行 |
| 启动原Core/Edge bin test ignored fixture后，`go test -race -count=1 -timeout=45s -v -run '^TestA3ActualEdgeCoreAuthControlRevocationAndOriginalOperation$' ./tools/nas-adapter/control` | 1真实整链路通过；Core/Edge fixture各1通过，退出0 |
| Storage测试ELF `--ignored --exact outgoing_recovery_tests::enospc::actual_enospc_at_five_send_boundaries_preserves_original_id_and_permits --nocapture` | 1测试内五个实际ENOSPC阶段全部通过，退出0 |
| `GO111MODULE=off GOOS=linux GOARCH=arm64 CGO_ENABLED=0 go build -o <private>/nas-control-arm64 ./tools/nas-adapter/cmd/control` | 退出0；ELF183，SHA256 `dab1cae30599b83d40ffb217d957e0958c1bab8bbecb3e50324a1f80ec5e426e`，未执行 |
| `cargo check -p rustsimhub-core -p rustsimhub-edge --bins --target aarch64-unknown-linux-gnu --features product-providers-candidate,email-production-candidate --locked --offline` | 退出0；仅交叉检查，不称完整链接/ARM执行 |
| Node `test-web-{forwarding,forwarding-history,daily-sms,sms-management,clients,nas-bridge,operation-receipt,diagnostics,test-center,console-fencing}.mjs` | 10套全部退出0 |
| `node tools/test-web-app-assembly.mjs` 与 `node tools/test-web-console-browser.mjs` | Windows Edge Chromium新context，原主页/控制台/ESM/CSP/IDB/合成签名登录，320/390/1280及模块缺失/退出/迟到/回收站/65页回执等原断言通过，退出0；本机API为合成，非旧NAS页面 |

各feature/库测试互相重叠，不合计成独立验收分数。专用NAS、ENOSPC、导入worker等仍保留ignored/Skip，只有实际显式运行的场景计入上述专项结果。
转发真实TLS/SMTP端点在本机，覆盖撤权/关闭/凭据代次/断线/迟到/Unknown无重试；邮件生产接线已存在，未为凑任务重写。
本轮未重复历史四UID/worker进程/转发Root ENOSPC或PID1部署；没有把历史结果冒充本轮新运行。

## 保留的失败、未完成及交接

首次构建新增Core测试缺Instant导入，编译101，修正测试导入后构建通过；不是产品负对照。
首次ENOSPC夹具降权后直接读`/proc/1/ns/mnt`被拒绝，在填充前停止。改为Root隔离创建者先核真实命名空间、读取原namespace标识，再传给降权测试比对；独立挂载/真实16MiB容量/路径门禁未删。随后五阶段ENOSPC通过。
首次委派视图夹具给同一消息同时填写text和data，按原输入契约InvalidInput拒绝；改成独立二进制样本，保留原失败，未放宽生产校验。

仍未完成：准确旧NAS页面源码/版本及原页面只读装配、旧PCM跨主机协议/时钟与真实声音、真实账户和消息投递、真实NAS/设备、B完整发行/迁移/签名/回退及用户终验。新NAS联测的正常Client控制不代表旧NAS UI或旧NasKey已获得控制权。
给B固定实现SHA、上述命令和原始日志；按原profile构建并独立核源码/锁文件。无wire/IPC/数据库格式变更，回退正常revert A3实现与材料提交即可，保留原Unknown记录；回退后NAS缺请求头问题会恢复。
新测试/材料为AGPL-3.0-only，固定旧Go契约来源`ea019485e5d890a96acae357c52b94c746de837f`及原许可保留，未引入新上游源或依赖。AI辅助代码/测试/材料均经上述实际检查。
完整HEAD、PR状态和云端CI在提交后回读，不预填云端成功；不自审核/合并，不改作者或全局看板，热点延期/USB优先和整体**81.5%**保持。
