# USB优先安排与离线成果主线整合（2026-11-16）

## 用户安排与本轮结果

用户明确：热点配网先跳过，主要使用USB电脑工具配网，后续有时间再开发。
PR260已同步AGENTS、接手/进度/计分记录、开发计划、A/B任务书及实测清单。
热点专用新增开发和手机测试延期，保留既有源码、应用66～68等失败、草稿PR69和原验收缺口；
共享Wi-Fi事务、USB工具授权和其他业务继续。没有宣称USB工具在当前设备已经端到端通过。
不删原任务、不改分母/权重，整体验收仍81.5%，20/169与加权10.6%保持。

主线独立审查并复验后，正常合并以下四笔非草稿成果；没有修改成员产品代码、作者或分支：

| PR | 固定受审HEAD | 实际交付 |
|---|---|---|
| 255 | `fc5493d0b912db259c4770b1ab406a502889e20e` | 单一统计身份跨窗口Pending串行门禁，旧摘要未决时不prepare新交易 |
| 256 | `0bb53817478e390b8407e2a31780edfe47fa3d21` | 固定SDK离线交易向量及有限Root接口提案，没有新增生产签名/提交能力 |
| 257 | `58b0b18da56b50d0e83fc5def1bfb3aea40b1713` | NKN运行按钮和过期定时器纳入后端样本年龄 |
| 258 | `ce09ff8430776d00ffbb8ce2abdf7910b9b023a2` | ready回调撤权/阻塞耗尽预算后，不继续下一连接或提交阶段 |

产品组合实测提交：`35643994cd31cced552c12b53d32ca4e5159f8ba`。
随后仅合并PR256的工具和文档，最终主线为`d79dc8345ecbdb8bf467d48e9e5b5a4acbd5b2d1`；
逐字节核对受测usage-stats/nkn-agent/forwarder、NKN网页与四组网页测试、Cargo.toml/Cargo.lock保持。
本报告后续提交只记录证据，不循环宣称它自身的最终SHA。

## 主线实际复验

WSL Ubuntu，Rust固定工具链，普通UID1000，Linux原生导出源码及独立target，TZ=UTC，
Cargo使用原锁和`--locked --offline`。组合测试在新建、只有lo的network namespace中运行；
仅接口绑定夹具保留CAP_NET_RAW，未改主机网络或其他进程。普通严格检查另行运行。

| 命令/范围 | 实际结果 |
|---|---|
| `cargo test -p rustsimhub-usage-stats --locked --offline` | 90通过，0失败/忽略 |
| `cargo test -p rustsimhub-nkn-agent --locked --offline` | 65通过，0失败/忽略 |
| `cargo test -p rustsimhub-forwarder --locked --offline` | 127通过，0失败，1原专项忽略 |
| 同forwarder命令加`--features smtp-production-candidate` | 129通过，0失败，1原专项忽略；与默认组重叠，不相加称独立总数 |
| 三个crate的`cargo clippy --all-targets --all-features --locked --offline -- -D warnings` | 通过 |
| `cargo fmt --all -- --check` | 通过 |
| `node tools/test-web-nkn-product.mjs`、`test-web-nkn-status.mjs` | 两组通过 |
| `node tools/test-web-forwarding.mjs`、`test-web-forwarding-history.mjs` | 两组通过 |

负对照使用完全相同的新测试字节，生产文件还原为b15ea3f对应版本：

- PR255两条新回归分别实际运行失败101：未决/已准备未提交的原交易没有挡住其他窗口prepare。
- PR258两条ready-fence回归均实际行为失败101：原实现连接调用1，预期0；不是编译失败。
- PR257原网页配同一新测试实际失败1：后端样本年龄未纳入门禁，actual true/expected false。
- 正向修复及合并后组合均通过。原Unknown不重发、不放宽10秒预算或300秒Root权限。

### 固定SDK向量的独立复验

按PR256原README和run.py运行，Go精确为1.27.3 linux/amd64；
原SDK/NCP31个文件从固定GitHub blob获取，长度/SHA256/Git blob SHA1全部与原manifest一致。
仅为依赖准备获取原go.sum锁定的224个公开代理文件（18,731,223字节），不改版本/锁；
随后从本地文件代理离线准备模块，并在无外网namespace中以普通用户运行原runner。

- 原`go mod verify`、build、gofmt、vet通过。
- 15组独立编码/签名/交易摘要向量与固定SDK、提交的vectors.json逐字节一致。
- 每组重新解码后两种签名/主题篡改共30次拒绝；4种错误编码反例均以预期行为退出1。
- 正向HTTP调用计数0；没有创建真实SDK网络客户端或发送链上交易。
- 本结果不等于真实链共识接受、费用/确认深度、生产prepare零网络或长期权限验收。

## 原失败与环境前置保留

1. 普通UID首次跑转发时5项接口绑定测试失败；原未改转发生产文件的基线也复现相同5项。
   在独立lo namespace中仅补夹具所需NET_RAW后，默认及SMTP完整组通过；没有改产品来绕过权限。
2. 第一批/tmp源码目录后来不可用，后续重新从相同Git归档导出到任务专用Linux持久目录。
   该次未运行退出单列，不当作产品失败或测试通过。
3. 已有SDK准备目录的client.go与原manifest不符，拒绝复用；从固定blob重新取得原文件并三重校验。
4. 用户/共享Go缓存均缺`github.com/pion/datachannel@v1.5.6`等原锁输入；两次module-verify未通过保留。
   补齐原锁定公开依赖后原runner通过，未升级、删断言或使用修过的上游冒充原SDK。
5. 四笔成员PR及PR260的实际check annotation均确认账户付款/消费上限导致未启动，零步骤。
   云端CI不记通过；正常固定HEAD合并，不用管理员绕过、不改保护或工作流。
6. 一次查询工作流时使用错误run ID返回404，随后按PR返回的精确run/check核实；不是CI或产品结果。

主线本机原始日志/归档/摘要保留在忽略的任务证据目录，未提交真实通信数据、密钥或设备日志。
成员报告中的其他Core/IPC/worker/Go/NAS结果属于成员证据，本轮没有冒称全部独立重跑。

## 提案审核边界与继续工作

PR256作为离线参考工具及待决接口提案接纳；合并不等于批准新生产Root能力或自动启用统计上报。
Core固定用途签名/有界操作/能力协商方向可以继续离线实现与拒绝测试；
操作10～12、空attributes和帧预算仍是候选契约，实际接入须随实现核对旧版本兼容和原注册绑定。
真实链确认RPC/证据、信任来源与深度尚缺，不能开放confirmed或将节点接受当确认。
长期授权另行完成契约审核，当前300秒有限Owner权限不变；`reporting_available:false`保留。
这些限制不阻塞A包其他业务、B包离线集成或USB配网共享事务。

仍未运行：ARM/QEMU、真实浏览器三宽装配、设备/NAS/手机/真实SMTP/公网/链上、生产签名安装，
以及主线全仓/全部发行profile回归。对应项留B或主线后续，不用本轮定向复验替代。
本轮没有访问设备、变更服务或触碰蓝牙/AirSIM/Native/Media源码；草稿250/259/69/52/53/56保持。

审核账本按实际作者yh1030登记四笔固定HEAD为offline_accepted，steps为空：
本次是新A执行包交付，不能硬算成历史C01～C24全部验收。Git贡献由真实作者和当前main刷新。
热点专用部分统一延期，其他产品接线/组合/真实验收继续按两包和主线职责推进。
