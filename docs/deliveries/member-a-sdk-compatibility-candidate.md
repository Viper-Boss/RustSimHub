# A13～16：SDK兼容候选补丁、真实对照和构建交付

账号wnfhsj88，2026-11-01。起始main
`97f9c52fa832591a5e260d3dc4181847a8b3dfe3`；本轮实现及完整受测源码
`666bdbdb810fdfee7f89a2d0605a7f7992d19bdd`，最终运行candidate_sources_dirty=false，
脚本核验开始/结束Git和8个实际执行输入摘要均相同。之后仅补本人报告/证据快照
及定向保留unified-diff上下文字节的whitespace属性，不改变实现/锁/补丁/测试。
最终完整PR HEAD以PR正文/GitHub回读为准。

同仓库个人分支 `codex/wnfhsj88-nkn-sdk-evaluation-20261101`，不是fork。
A26 PR144已由负责人合并；A27另交PR153，本包不重交其实现/待审依赖。

## 实际改动和产品边界

新增 `tools/nkn-build/sdk-evaluation/` 的固定源码/前后摘要、186行SDK兼容补丁、
实际Go测试、独立Go锁及可重跑隔离工具；本人说明/37模块证据/剩余索引。
原SDK/NCP31文件来自任务书固定上游，逐一核Git blob/SHA256/长度；不提交vendor、
算法源码、缓存、二进制或私有材料，不把官方v1.4.8 sum当本地fork内容证明。

补丁只作用于生成work阶段，五项行为：构造失败nil-safe清理、钱包继承固定dialer/
预算、原生命周期context约束RPC/WS与重连timer、关闭/发送同锁防竞争及迟到连接
拒绝、固定1MiB RPC正文上限和移除原错误正文日志。没有更改NKN密码算法、外部
wire/Auth/Owner/Root证明/Scope/数据迁移，未把SDK链接到原stdio引擎或启用产品。

原版5场景确认实际缺口；同组补丁版确认修正。基线“通过”是缺口复现，不是旧SDK
安全通过。真实socket只到显式创建的loopback夹具，其他dial在DNS/socket前拒绝。
合成高度42、独立随机账户、内存NCP都不是链/真实成员/设备/NKN Ready证据。

## 精确源码最终验证

Go1.27.3，Linux x86_64；C编译器仅用于race，probe制品CGO_ENABLED=0。
继承proxy/CA，显式workspace Go缓存，不写home。总体命令实际退出0：

`python3 tools/nkn-build/sdk-evaluation/evaluate.py --run --online-advisories --source-cache /workspace/RustSimHub/work/a13-eval/upstream --output /workspace/RustSimHub/work/a13-eval/run-final`

| 命令/阶段 | 实际结果 |
| --- | --- |
| 31个固定fork文件验证 | Git blob+SHA256+长度全部匹配；原版运行后再核一次 |
| `python -m unittest -v test_sources.py` | 6通过；改源码/链接/重复源/改补丁/越界目标拒绝 |
| `go mod verify` | 0，缓存模块校验通过；local替换另按源清单核验 |
| 原版 `go test -race -mod=readonly -count=1 -timeout=15s -v .` | 5通过：确认五项接入限制，非修复通过 |
| 原作者SDK/NCP同race命令 | 4通过，保留原测试作者，不计本人新增 |
| 补丁check/apply，fuzz=0 | 0；仅两个允许目标，前后摘要匹配，其余29文件不变 |
| 补丁版相同race五场景 | 5通过；原断言按明确原版/补丁模式比较，不删除问题场景 |
| 补丁版原作者四项回归 | 4通过；NCP/非广播原行为保持 |
| `go vet -mod=readonly . github.com/nknorg/nkn-sdk-go github.com/nknorg/ncp-go` | 0 |
| native/ARM64 `go test -c -mod=readonly -trimpath -buildvcs=false` | 均0，真实ELF machine62/183；native制品再次实跑5项通过 |
| 37个实际测试编译模块+许可文件摘要 | 已列证据，不假装覆盖未选整个历史图；runtime Go许可另记摘要 |
| OSV显式模块版本查询 | 有6模块匹配，标识保留，未作函数可达性/漏洞利用/本地patch审计，不标安全通过 |
| gofmt及Git diff check | 通过；补丁数据上下文保持原字节，以该单文件属性处理格式数据空白 |

这里只新增5个SDK行为测试及6个Python来源/补丁检查；原版/补丁/race/native制品
是同场景的不同实际运行，不相加刷测试数。未运行系统Root/NKN网络/设备或其他
成员的生产测试；本包与起始main的根Cargo/lock、生产crates、configurator、
firmware、Actions和原tools/nkn-engine无diff，不挪用PR144/153结果填本包通过。

实际制品仅在work，均为**测试probe，非引擎发行**：

| 架构 | 字节 | SHA256 | 运行边界 |
| --- | --- | --- | --- |
| amd64 | 23895496 | `e8bba71389272c92df39aca85555bb780f75b19562e6d637b5c5ba97dbb17c87` | 实际运行 |
| arm64 | 22356908 | `0eb62446509b75df96149582f9d4bf0e5947508b6114784b638450a0d94683bc` | 仅编译 |

没有两个独立空缓存构建，不宣称可复现发行；ARM没有运行/签名/安装。1MiB是待审
SDK RPC兼容预算，不能扩大2048B本地业务帧。模块版本未批量升级；OSV命中包含
websocket/dtls/stun/x/crypto/x/net/x/sys，同模块多个GO/GHSA可能是同一通告别名，
不作为独立漏洞数，也不据模块匹配断言本场景全部可利用。

## 保留失败和下一步

初稿夹具将getlatestblockheight的result错误写成对象，原SDK正确要求uint32，导致
2项测试失败；已改夹具为真实协议标量42，未修改SDK解析规则。2MiB输入的race
测试初稿100ms夹具HTTP预算不足，失败在观察完整大响应前；此单项夹具改为1秒
上下文/500ms RPC来观察同一2MiB输入，产品租约、其他100ms场景和1MiB补丁断言
保持，不称旧产品问题。原失败日志仅work。

最初尝试离线列举整个历史模块图需要未缓存的无关.info，按GOPROXY=off实际失败。
工具改为列举实际编译/测试包闭包，明确缩小证据口径，没有打开下载或补编未知
依赖结果。完整第三方来源/许可/通告仍须主线按实际发行图审核。

补丁未获生产批准；仍缺所有cache任务/密钥零化/全部日志/上下文不守约provider的
完整保证。Close/worker退出不等于全PID/cgroup清理，WebRTC/STUN未启用/验收。
公开RPC/设备固定出口、真实SDK自发消息健康、Root低权策略/证明和独立身份仍缺。
产品引擎仍unavailable，A22可信代次/年龄/授权provider、统计SDK交易确认、A26/27
真实安全会话/Core桥、A29队列/Fence兼容和A30完整发行组合仍不写完成。

主线先审固定fork、补丁/1MiB取舍和依赖处置，再接真正受管SDK子进程、重做完整
引擎双架构/异常矩阵和发行；C组装及负责人签名/安装/真实网络/设备验收保持原分工。
回退本包只是不采用独立候选，不修改产品数据库/Root材料/已有监督器。

原始日志/源缓存/制品留 `work/a13-eval/`，精确最终日志摘要：

| 工作区日志 | SHA256 |
| --- | --- |
| report.json | `4be3614d0f4bf4a0ab8c40ec5f55c06de93907ea678ab4f576518feeedd21e74` |
| source-tests.log | `96eda415ff44e8dfa965b3d31275c4382946778c444e859f3e113d7df93a727d` |
| module-verify.log | `b4537ed75f533f993f371954de47e42a793b8e5b0587577de7e27fb3e50696bd` |
| sdk-tests.log | `0251737f5225b647734be06e3823b3a2a5a81eec40e1130d52a86e24c65193f1` |
| upstream-tests.log | `6e2784d4bc13d2d6505a04436ac8d1ce557515fc2fc7f64bfd742ff0c85de506` |
| patch-check.log | `976e4ab16533cdda0c8ab02af822481993b4d8d1e9d965f4db6a1d4609b32b84` |
| patch-apply.log | `a299f2678d18499d4db26b2663656bd98648971e7ce7b0600613ad06c61bc502` |
| patched-sdk-tests.log | `1bcf17891e06f518ca60831793d7b39709d57c1be6a0c6597b0af9fe1dbe6ea0` |
| patched-upstream-tests.log | `57d1070919428aa456752433bf2d999af53979b8ecd2a9ac411666861ba1a21b` |
| vet.log | `e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855` |
| build-amd64.log | `e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855` |
| native-artifact-tests.log | `914b0885097a4b621c9d29cec9686fbda3cf12e9916db5b5b5723bfe73bf2ede` |
| build-arm64.log | `e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855` |
| osv-0.json | `4f61ccb5bf75c796eed22eee1e6203f2d624bafeef26350187200f9d38a049f9` |

来源/许可：原NKN SDK/NCP Apache通知和NKNGuard修改说明/根AGPL来源保留，不改写
其作者；本候选diff为独立待审修改，完整新工具/测试AGPL-3.0-only，OpenAI Codex
辅助实现与验证，无工时估造。无生产设备/NAS/真实通信/链/邮箱/短信/电话/蜂窝/
Root/产品签名安装部署；不自行合并。主线已验收81.5%保持，不由离线测试加点。

## 同步最新main后的再验证

本分支随后正常合并main `b94256930fe0ca14d587d7a54fcc9e80e1f5be6b`，保留
已合并A27 PR153与主线Store/Queue及构建修复；只解决本人剩余索引冲突，生产
源码没有人工冲突修改。最新完整受测合并HEAD `eb67becd30f23568e8642e72fe808075ff866c97`，
开始/结束candidate_sources_dirty=false及执行输入摘要相同；完整原版5/补丁5、
原作者前后4项race、6来源检查、vet、native/ARM编译和native制品实跑均再次通过。
原版与补丁的条件/预算未变，未用旧结果冒充此合并HEAD。日志在
`work/a13-eval/run-main-synced/`，产物字节/摘要与前述精确结果相同。

与最新origin/main比较，仍仅本人SDK候选工具/文档，不包含A27重复实现或任何
产品根依赖/SMTP/Core/Root配置变化。新最终HEAD/CI以PR正文回读为准。

| 同步后实际运行日志 | SHA256 |
| --- | --- |
| report.json | `bd6d9e8efc527f132c9cc699fdaf918bdba095ec473ea65c5546b7d6f3014f24` |
| sdk-tests.log | `7cd1cbe93ed89c11c0f1995d4ee7122761b3f13ae9721127eb5162a3e7d347be` |
| patched-sdk-tests.log | `d0bf36ecdc7d5b5e07937462dbbb150383dd68e10b8bd5866d49f0504f3e20a7` |
| source-tests.log | `66be9e08d5e238a479777374ac1c98bb5c640e0fbd9074b94c22cd9c492963bf` |
| patched-upstream-tests.log | `336ac8bbdb75ca7a9b7efc0a5ae77abe718ff92923220a85ae9e5b55db235824` |
| native-artifact-tests.log | `2de7dfb6b90bc1a4d95a69ab996b6e5199abaf9062f802eb7f5d13f4d3bfa7fd` |
