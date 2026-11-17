# A2：NKN 与远程接入离线核对

任务来源：[A 包 A2](../team/OFFLINE-A-PRODUCTION-20261115.md#A2NKN业务与远程接入补缺)。
起始 main：`de663743bd726e77f7c560139d0f91ea78e8fe57`。
实现提交：`bd09e95471f33b6ab9261c1b56a5a65262dc331c`；受测产品 tree：`d67e8f718fcf1f19e73edb59e34b6a125776cd82`。
命令、原失败及测试来源见[交付报告](../deliveries/OFFLINE-A-A2-NKN-REMOTE-20261117.md)。

本表交付 A2 六条要求的离线部分。公网、真实 NAT/TURN、实际 systemd、设备和用户验收均未完成；18 个原动作不是新增的已验收动作。全局 81.5%、原分母和权重保持，由负责人审核后更新台账。

## 已有实现及本次缺口

现有固定 SDK、Root、成员证明、Noise、Core Auth 和三引用监督器已经接线，继续复用。
`nkn_root.rs` 只让真实近期本地 Owner 创建 Root authority；Client、NAS 和远程 token 不能升格。
`nkn_product/candidate.rs` 保存 Manual、Remote、Statistics 独立引用与原操作回执；读取不启动网络。
Root 读取受审策略、安装角色、独立传输种子和固定程序，核对原进程、UID、NNP、capability、BOOTTIME 和 cgroup。
SDK 内部重连不能形成第二套自动重试循环。来源和许可沿用 `docs/nkn/fixed-upstream.json`、`managed-programs-NOTICES.txt` 及四层固定补丁链；Go 服务仍是 Go，不称作 Rust SDK。

本次发现并修复一个实际缺陷：成员文件失效关闭监听后，Owner 在驱动下次轮询前完成 OFF/ON，旧布尔抑制位看不到中间 OFF，导致新请求被接受但监听不恢复。
抑制改为绑定被关闭的 `InstalledMembership` 分配代次；Weak 不延长旧凭据生命周期。旧代次保持关闭，新显式 ON 安装的新代次可以重开；原 Root epoch 和截止策略不变。
没有增加自动续期、VPN、用户库、Root 权限或新的网络入口。

另一处缺口是远程支持结果无法一次复制。本次在现有 NKN 面板加入“只读核对并复制远程诊断”：一次 `nkn_status`，严格解析、年龄/上下文检查后按白名单复制。
没有启动、关闭、探测公网或补发旧请求；错误、陈旧值、退出后的迟到响应不能产生成功复制。
未知计数保留 null，peer_count 明确是连接节点数；不复制 token、主机、设备公钥、成员证明或 Core 上下文 ID。

## 18 个原动作的对应证据

下列“已复验”仅覆盖写明的离线断言；没有设备或公网结果的行仍保留现场验收。

| ID | 已有代码、本次处理和离线证据 | 仍需主线/外部条件 |
|---|---|---|
| NKN-01 | 沿用固定上游、兼容/安全/managed 补丁；验证过的 SDK stage 另复制，原上游逐字节保留，模块 verify 通过 | 上游后续安全维护与生产依赖接纳 |
| NKN-02 | 固定 Go1.27.3，SDK/Root ARM64 ELF 构建；Core ARM lib/bin cargo check 通过 | B 的正式同源制品、完整交叉链接、ARM 执行和发行验收 |
| NKN-03 | 原限权 Root、固定 Wi-Fi/USB 出口及 DNS；地址拒绝测试、实际隔离 nft/cgroup 清理通过 | 安装角色、真实接口和生产 Root/systemd；不新增 TUN/WireGuard/NET_ADMIN |
| NKN-04 | 默认 OFF/被动读取、三引用、Remote OFF 后其他引用仍为2，最终全部释放；新增实际快速 OFF/ON 回归通过 | 真机开关、重启与生命周期 |
| NKN-05 | 原 Noise/成员/注册身份/Auth；新 all-scope Client/NAS 仍不能读 Owner 控制或创建 Root；实际旧令牌、错 pin/成员、篡改、重放、撤权通过 | 真实安装成员、真实客户端/NAS 配对；不动 BLE |
| NKN-06 | `routing.rs` 仅 CoreAuthV2/CallMediaV1；`nkn_business.rs` 仍校验原 token/scope/预算；65 项 NKN 回归 | 实际各获准业务逐项端到端，媒体声音由原窗口负责 |
| NKN-07 | 实际 Root/SDK/Core 状态被生产 JS 合约接受；未知 RTT/字节/节点数；新增只读诊断复制与三宽主页测试 | 真实公网 RTT、DHT、路径和流量计数，缺证据不显示0 |
| NKN-08 | 无外网隔离网络中的实际固定 SDK 有限对端、Noise 和 Core 业务查询通过 | 公网直连/中继，不将测试节点当公网节点 |
| NKN-09 | 原预算/到期/EOF/原作用域测试；12 次实际连接及同一 source 排空；修复新显式授权的重开 | 弱设备长期并发、切网与资源实测 |
| NKN-10 | 提供完整实现 SHA、来源、命令、拒绝和回退说明，默认入口仍关闭 | B 的未签名发行材料；主线签名/部署/真实回选 |
| NKN-11 | 原 Owner 登录、IndexedDB CAS/Unknown 不重发、退出/迟到/CSP/模块缺失负对照；诊断按钮已接实际主页 | 用户设备流程终验，不把合成 API 当真机 |
| REMOTE-01 | 继续采用固定 Core pin、成员证明、注册身份和有限服务；不引入可任意发现/路由的入口 | 可信渠道下发实际入口、公钥及成员材料 |
| REMOTE-02 | 下表列出受审 seed/TURN 输入和预检边界；未配置真实服务 | 负责人提供并批准真实服务及账户 |
| REMOTE-03 | 原 Auth9、all-scope 委派拒绝、实际撤权；冻结媒体契约7项/1忽略 | 真实媒体预授权与用户音频验收；不接管保护代码 |
| REMOTE-04 | 实际 IPv4/IPv6 HTTPS、严格 CA/主机名、拒绝重定向/代理/DNS 混合答案；地址策略与路由拒绝回归 | 真实 IPv6、公网 NAT 及全链路 TLS-only；下文不宣称支持 |
| REMOTE-05 | 仅复验冻结 Core 媒体授权/租约/停止/有界清理契约；列出强制 TURN 验证条件 | 真实 TURN allocate/relay、预算/丢包/双向声音；原媒体窗口处理 |
| REMOTE-06 | 实际 SDK 单次连接预算、原租约到期后 socket 写拒绝；Rapid OFF/ON 修复及旧代码负对照 | 真实 Wi-Fi/USB 切换、公网中继重连；无蜂窝回退 |
| REMOTE-07 | 新一键脱敏诊断只读已有 Core 状态；真实主页与失败路径通过 | 实际远端支持记录；Unknown 保持，不探测公网 |

## 本机端点与离线契约矩阵

`managed/a2_endpoints_linux_test.go` 使用真实 socket、原 scopedDialer、原 Lease/BOOTTIME 和 `newUsageHTTP`。
只有私有测试 launch/CA 是合成输入，不能据此声称取得 Root 授权。
在仅有 lo 的独立网络命名空间降到 UID1000、capabilities 清空后运行；故障也没有公网路由。

| 项目 | 实际离线结果 | 证据边界 |
|---|---|---|
| HTTPS IPv4 `127.0.0.1` / IPv6 `::1` | 各自严格合成 CA 正例通过；空信任库、错误主机名拒绝且业务 handler 未收到请求 | 不使用 InsecureSkipVerify；不是公网证书/IPv6 可用证明 |
| 重定向、环境代理 | 重定向目标0请求；六种代理环境变量设为陷阱端点，代理0请求 | 测原 HTTP transport；不能代替完整 SDK 各传输的 TLS 证明 |
| 固定 DNS 混合地址 | 实际 UDP DNS 返回允许回环及保留地址，先核全部答案，TCP0连接 | 生产拒绝私网/保留/嵌入地址另有原策略回归 |
| SDK 重连/到期 | 同一 lease 第二次 wsDial 拒绝并关闭原租约；旧连接关闭/到期后写拒绝 | 这是 TCP 拨号预算，不宣称完成 WSS/TURN 握手 |
| 实际 SDK 重连业务 | 6次 query 交替6种拒绝，12个原 Noise/Auth 客户端断言通过；每次等真实 SourceGuard 排空 | 正例 SDK 对端均退出0；5个负例对端预期无回复退出1，不写成全部对端退出0 |
| Root/成员失效 | 真实成员文件权限0640→0600关闭监听；恢复文件不自动重开；新显式 OFF/ON 成功；原 epoch 未变 | 隔离容器中实际 Root/SDK/cgroup/nft，不是真实 systemd |
| NAT | 固定接口、全答案校验、无任意 LAN/端口/代理/蜂窝回退的契约与测试通过 | 没有模拟或声称 endpoint-independent/symmetric/CGNAT 穿透成功 |
| 全链路 TLS-only | HTTPS seed/local client 部分已测；没有把裸 WebSocket/其他 SDK 通道标成 TLS-only | 当前没有新增 TLS-only 模式；需原 SDK/Root 负责人明确并验证所有传输策略 |
| 强制 TURN | 原 host 仅在原 Auth/租约/已核 peer 后给原 ticket，撤权和本地停止契约通过 | worker 为合成有限端点，不是 TURN 服务；未验证 allocate、relay 或声音 |

原限制继续有效：Root scope 最长300秒，启动10秒；监督器最多3次/30秒、间隔至少5秒；SDK 一个原 lease 一次 WebSocket 拨号。
Root 原 SDK cgroup 为128MiB、32任务、20% CPU；路由记录2048字节、正文1024字节、128请求和5秒调用预算。
这些是代码上限/契约，不能写成弱设备资源压力测试已达标。媒体强制 TURN 的实际带宽、帧率、延迟、丢包与恢复预算仍由保护路径所有者核验，不重写声音实现。

## 外部输入及现场顺序

1. 负责人接纳固定 SDK/补丁/整程序许可和正式同源哈希；B 提供可核对的 ARM 程序及发行材料。Core ARM cargo check 不是完整链接或执行。
2. 经可信安装链提供六角色、Root unit/cgroup、固定程序/策略、独立传输种子、成员文件和真实 Core pin；无效即拒绝。测试合成密钥绝不用于安装。
3. 提供获审 HTTPS seed 主机、端口、证书链及实际支持声明；接口只选 Wi-Fi/USB，DNS 来自同 boot 新鲜网络报告。不得因 USB/Wi-Fi 失败启用蜂窝。
4. 公网直连/中继测试需两个可信注册端、明确成员代次/撤销方案，以及真实 IPv4/IPv6/NAT 网络。记录实际路径和 Unknown，不从 SDK Ready 推断业务成功。
5. TURN 由原媒体窗口提供获审 endpoint、协议/端口、可信证书、短期账户/过期和 relay 资源预算；证明候选仅走获准 relay，并核验认证失败、到期、断线、拒绝私网/普通 LAN 回退和资源释放。不得由本次表格生成新媒体权限。
6. 真实通话/声音、设备 systemd、长时间负载、换网及用户终验由主线另行安排；热点新增开发/手机验收继续暂缓，PMIC/关机仍搁置。

## 给 B 的接入与回退

正常构建仍沿用 `tools/nkn-build/sdk-managed/{prepare,build,notices}.py`，固定源及补丁校验不可省略。
在新的已验证 SDK stage 运行 `go test -race -mod=readonly -tags=nkn_sdk_candidate -count=1 -timeout=90s -json github.com/Viper-Boss/RustSimHub/tools/nkn-engine/...` 和相同包的 `go vet`；交易组合再加 `nkn_usage_transaction_candidate`。本次使用已有经过验证的 SDK 链副本并核对上游与锁文件未改，未重新执行整套八构建可复现流程。

真实内核联测使用 `tools/offline-a/nkn_kernel_fixture.py`、`nkn_counterpart.go` 和原 Root fixture。
编译 Core bin tests 的 `product-providers-candidate`，用同一验证 stage 编译 Go 对端；输入按原夹具要求复制到只读 `/inputs`，在无外网、独立 mount/net/pid/cgroup 的一次性容器运行。
夹具中的 Root 权限只属于该容器。不要直接在设备或 WSL 主命名空间运行它，不安装/修改主机服务、网络、角色和账户。
普通 `cargo test` 对该内核场景保留 ignored，必须显式运行并记录实际 UID/命名空间及原 SDK 来源，不能把 ignored 当通过。

集成先审核本 PR，不依赖或合入 A1 草稿268，不自合并。现有默认关闭与生产接口不变；只新增网页诊断按钮，没有 wire/Root/IPC 格式变更。
回退可正常 revert A2 实现提交及材料提交，保留故障/Unknown记录；无需迁移数据库或更改安装角色。回退后快速 OFF/ON 的旧缺陷会恢复，不能声称已消失。
