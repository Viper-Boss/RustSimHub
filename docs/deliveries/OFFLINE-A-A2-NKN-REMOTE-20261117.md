# A2 NKN／远程接入离线交付

日期：2026-11-17（Asia/Shanghai）。按用户要求继续 A2，并在已明确授权的本机 Ubuntu WSL2 中隔离测试。
起始 main：`de663743bd726e77f7c560139d0f91ea78e8fe57`。
实现 HEAD：`bd09e95471f33b6ab9261c1b56a5a65262dc331c`，tree：`d67e8f718fcf1f19e73edb59e34b6a125776cd82`。
实现归属由 GitHub 回读为 yh1030，未重写其他作者；交付材料为后续独立提交，当前完整 PR HEAD 以 GitHub 分支/PR回读为准，避免把材料提交自身 SHA 写入自身。
分支：`codex/offline-a2-nkn-remote-20261116`，目标 main，交负责人审核，不自审核/合并。

对应原 ID 为 NKN-01～11、REMOTE-01～07；[逐动作盘点、端点契约、现场前置及给 B 的接入](../offline-a/nkn-remote-a2-20261117.md)。A2六条离线范围已交付，不表示18个项目动作或公网/设备终验完成；全局仍 **81.5%**。

## 本轮实际完成

- 修复成员凭据失败后的快速显式 OFF/ON 无法恢复监听：按 InstalledMembership 代次抑制，不自动重开原代次、不续 Root 租约。
- 新增实际 Owner/Client/NAS 权限及 Root/SDK/Noise/Auth/成员文件/三引用联测；没有替换原 Auth 或用户库。
- 原 NKN 面板接一键只读核对/脱敏复制，严格保留 Unknown 与节点数含义，拒绝陈旧、失败和退出后迟到结果。
- 新增 IPv4/IPv6 HTTPS、CA/主机名、重定向/代理/DNS 混合答案、SDK 单次拨号及原连接到期测试；公网 NAT、全链路 TLS-only、TURN/声音缺口明确保留。
- 保留 SDK/Root 原来源/许可证，只有测试代码和必要小修复；没有 TUN、任意 LAN/端口、NET_ADMIN 扩权、设备、生产签名、工作流或全局进度改动。热点专用工作继续暂缓。

改动范围：`crates/core/src/nkn_{product,listener,uid_tests}*` 的普通 NKN 路径、`crates/edge/src/web/{app,nkn-product}.js`、原网页装配夹具/专项测试、`tools/nkn-engine/managed/a2_endpoints_linux_test.go`、`tools/offline-a/nkn_{counterpart.go,kernel_fixture.py}`、原 Root 测试夹具的真实 Podman 命名空间/loader/cert 适配，以及本报告/对应盘点。
Core 特性编译会编译既有依赖，不代表运行或接管 BLE/AirSIM。未改受保护声音、BLE、恢复生产代码及他人草稿。

## 固定来源与结果

最终产品源码归档 SHA256：`c8e184cf44fa5d7e93fc8dc040ea16c8772778727a9f56372f1793fbebb49e47`。
下列为实际运行结果；Rust 工具链1.98.0、Go1.27.3。单元测试中的既有 ignored 不计为通过，套件有重叠不相加冒充独立验收量。

| 命令/场景 | 实际结果 |
|---|---|
| `cargo test -p rustsimhub-core --lib --features usage-root-transport-candidate,email-production-candidate --locked --offline -- --nocapture` | 122通过/0失败/3忽略，退出0 |
| 同 feature，`--bin rustsimhub-core` | 238通过/0失败/4忽略，退出0；含新增 all-scope Client/NAS 拒绝 |
| 默认 `cargo test -p rustsimhub-core --lib --bin rustsimhub-core --locked --offline -- --nocapture` | lib21通过/2忽略，bin225通过/0忽略，退出0 |
| `cargo test -p rustsimhub-nkn-agent --all-features --locked --offline -- --nocapture` | 65通过/0失败，退出0 |
| `cargo test -p rustsimhub-auth --lib --features nkn-session-candidate --locked --offline nkn_session::tests:: -- --nocapture` | 9通过，退出0；原 token、撤权、line/realm/截止/成员与连接绑定 |
| `cargo test -p rustsimhub-edge --locked --offline -- --nocapture` | 71通过，退出0 |
| Core上述组合/Edge `cargo clippy --all-targets ... -- -D warnings` | 均退出0 |
| Core `cargo check --no-default-features --all-targets --locked --offline`；`cargo fmt --all -- --check` | 均退出0 |
| Core `cargo check --lib` 和 `--bin rustsimhub-core --target aarch64-unknown-linux-gnu --features usage-root-transport-candidate,email-production-candidate --locked --offline` | 均退出0；仅检查，非完整链接/执行 |
| 原冻结 Core 媒体接口：`cargo test -p rustsimhub-core --bin rustsimhub-core --features remote-media-tests --locked --offline calls::checks::remote_media:: -- --nocapture` | 7通过/1专项忽略，退出0；合成有限 worker，非 TURN/声音实测 |
| 固定 SDK stage：默认 `nkn_sdk_candidate` 与交易组合，engine `/...` 各 `go test -race -mod=readonly -count=1 -timeout=90s -json` 和 `go vet` | 每种58个顶层测试通过，均退出0；含3个新增端点顶层测试，重叠不求和 |
| 相同 stage `go mod verify`、SDK/Root ARM64 `go build -mod=readonly -trimpath` | 退出0；ELF machine183，未执行 |
| Node专项 `nkn-status`、`nkn-product`、`diagnostics`、`test-center`、`test-center-component`、`app-imports`、`app-imports-negative` | 最终源码7套均退出0，原拒绝路径及新诊断复制断言通过 |
| `node tools/test-web-app-assembly.mjs` | Windows Edge Chromium全新context，实际主页/ESM/CSP/IndexedDB/签名登录，320/390/1280、诊断复制和缺模块负对照通过，退出0；API为有限本机合成设备 |
| 显式隔离内核 `nkn_product::a2_tests::kernel_remote` + 12次原 `isolated_nkn_client` | 修复版退出0，实际Root/固定SDK/Noise/Auth；6次查询与6种拒绝交替，原scope和cgroup/nft/listener清理通过 |

实际内核正例源 tree `626326e392bb08237bed0192df7b63cdd436e67d`，归档 SHA256 `0603f8d4b3534cd1a83f8d55ae6f7221cff1a7a0d96c4eead33be0222b5d3f8a`。
与最终 tree 仅四个网页文件不同（app.js、nkn-product.js、装配夹具、专项Node测试）；Core、Go、内核夹具逐字节相同，因此未为网页按钮再次重跑完整内核。
实际 Root/SDK 二进制复用已核验的 `18cbd7c405d844d24de15eb115880217cf8f5fad` 固定链；当前 main 的 `tools/nkn-engine`/prepare 产品字节相同，本次 Go 新增文件只含测试。
SDK SHA256 `643e580a321bd29a6021b3a804f2db076e58e30c4867eadbb53ddec82edd77b0`；原31源及四层补丁验证记录保留，端点测试在其新副本核上游及go.mod/sum未变。没有重新下载依赖、放宽补丁校验或把缓存未核验文件当依赖证明。

## 旧代码负对照与历史失败

最终负对照 tree `90410f449a9d2b1ec4508be66a81a6bb10c94fcc`，归档 SHA256 `ca3b584603edbe2bb380e6f34b34665b793cbedd27ea36a6b03e92f0aff8b08e`。
它与最终产品 tree 的唯一差异是 `nkn_product/candidate.rs` 恢复起始 main；其余最终测试相同。
初次启动/状态采样成功、成员失效关闭、恢复不自动开之后，快速显式 OFF/ON 的3秒监听存在断言失败，Core退出101/容器退出1；没有存活容器。
修复正例完成同一断言、原Root epoch不变及12个实际客户端。没有把超时当作公网连接失败或跳过断言。

原始记录保留：最初旧代码 tree `c57b33bd...` 同处失败；第一次修复 tree `6aab48a7...` 已重开但只读 snapshot 因锁忙返回 Unavailable，夹具改成有界等待该只读观测后才跑通。
首次构建辅助脚本重复 GOWORK 参数导致 Python TypeError，已记录在 build result 的恢复说明，没有伪报那次构建通过。
媒体契约最初错误选择 lib/筛选名两次得到0用例，汇总工具标 nonempty=false 并退出1；这些不是通过证据。后改为真实 bin 目标和完整模块名，实际7通过/1忽略，原记录未覆盖。
负例 Go SDK 对端中5个因预期无应答退出1，原 Rust 客户端拒绝断言均退出0；错 pin 对端退出0。不得把12个Go进程写成全部退出0。

## 隔离、安全边界和剩余项

Linux 测试使用用户授权的 WSL2。端点套件仅 lo、UID1000、capabilities清空、NoNewPrivs；内核联测在一次性 Podman 私有 mount/net/pid/cgroup、network=none 下执行，readonly inputs，不挂载宿主设备或宿主 cgroup。
Root fixture 验证真实命名空间分离；全局形状测试IP只存在于私有网络。实际Root与nft/cgroup有效，但没有真实systemd，也没有访问NAS、SIM、蜂窝、设备或公网服务。未安装主机服务/账户，不改主机网络。

未完成的真实部分及所需输入详见逐动作盘点：获审HTTPS seed、可信真实身份/成员/账户、IPv4/IPv6/NAT条件、TURN服务与短期凭据/预算、正式ARM制品及设备/用户验收。全链路TLS-only与强制TURN没有新增已验证产品模式；不能把局部HTTPS/冻结媒体接口成功当作它们已实现。
冻结接口、公开脱敏材料与回退步骤已经给出；不接管媒体/BLE工作。负责人审核/合并及B正式接入仍在后续。

本机原始证据在忽略目录 `.local/offline-a/a2-*`，仅合成测试日志和源码元数据进入专用交付包；令牌、TLS私钥、生产材料不公开。完整PR HEAD和云端CI的最终状态在提交后回读；本报告不预填CI成功。
