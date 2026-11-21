# 主线复验：网络持久回执与统计候选（2026-11-16）

## 实际交付与来源

热点专用开发/手机验收继续暂缓，USB电脑工具优先；蓝牙与完整统计传输草稿仍由原窗口开发。
本轮主线审查并独立复验五笔非草稿，未改成员分支或作者：

| PR | 固定HEAD | 接纳边界 |
|---|---|---|
| 263 | `9d3aed7cc3ae658a17c8841b93132b49151d488f` | 网络策略原ID先持久化、Unknown恢复、精确观察、会话隔离与本机CAS |
| 264 | `e12ea2fba2255f75dd311ab70c756e8b670059f3` | 短信8项/电话10项的证据与缺口审计文档，不是18项功能全部通过 |
| 265 | `89a4b5c95958bc25a64d8ee61fa55eb8ad584939` | 默认关闭的固定用途本地统计签名候选及原Auth/许可门禁 |
| 266 | `83ab91a557f1fe8bfd7827f527436ae54c724137` | 默认关闭的有限Root/owned SDK交易候选，query仍unknown |
| 267 | `5a79e6119925f63323dab6ee7a2a72ec9064af62` | 原Root单次观察连接、当前许可/引用代次/预算与最终写入保护，仍非完整CheckInTransport |

263～266已正常固定HEAD合并。267在GitHub回报CONFLICTING；本机以最新main为父进行三方集成，
不需要修改任何产品文件，生成负责人合并提交`97effc40224ab14f5b179f5332d78ef0f20ef96e`。
该合并保留267完整原历史；在负责人分支提交PR，不强推成员分支或main，不改保护规则。

实际测试输入为263和267的独立Git归档。267已经包含被审265/266依赖：
已核usage-stats与265、Go引擎/prepare.py与266产品字节相同。
集成提交的Core/usage-stats/Go/prepare.py与267、Edge/网络及装配测试与263逐字节一致。
后续交付文档不改变受测产品；最终main以合并PR实际回读为准，不在本报告循环写自身最终SHA。

## 主线独立运行结果

Rust/Go使用固定工具链、原锁、Linux原生源码及独立target/cache、TZ=UTC。
接口夹具在新建的仅lo网络namespace中以UID1000运行；SDK整组额外清空全部能力并设NoNewPrivileges。
没有设备/NAS/公网节点或真实通信数据，没有改变主机网络/服务。

| 范围 | 结果与限制 |
|---|---|
| 263网络/限额Node专项 | 两组通过 |
| Edge `cargo test -p rustsimhub-edge --locked --offline` | 71通过，0失败/忽略 |
| 统计 `--features transaction-candidate` | 92通过，0失败/忽略；含原90，不另累加 |
| Core `--features usage-root-transport-candidate,email-production-candidate` | lib122、bin237通过，合计6项原专项忽略；含签名5/观察9及原Auth/业务，不当作全仓测试总数 |
| Core/usage-stats all-target严格Clippy、全仓fmt | 通过 |
| Core `--no-default-features --all-targets` check | 通过，默认不启用新候选 |
| 单独签名feature、不含观察候选 | 原签名5通过；原Root2通过/1原私有夹具忽略，与上组有重叠 |
| 原Go引擎无SDK race/vet | 55顶层/195含子测试pass；4个无测试包，不当作通过用例 |
| 固定SDK默认/交易双tag race/vet | 各55顶层/195含子测试pass，4个无测试包；两组重叠，不相加为110个独立测试 |
| 原SDK Managed专项 | 实际运行通过，未冒称独立重跑全部SDK/NCP原上游测试 |
| 默认/交易SDK实际amd64程序 | 均构建且实际--version通过；默认能力省略/0，显式双tag精确为1，源码固定为267，非fixture |
| SDK/Root实际ARM64程序 | Go交叉构建成功，ELF e_machine=183；未运行，不当作QEMU/设备通过 |
| Core ARM64 check | 本机失败：固定Rust工具链缺aarch64目标标准库；未下载替代版本，不记通过。成员原ARM检查证据与本机结果分列 |

### 实际浏览器与旧生产负对照

使用已存在的专用Playwright虚拟环境、已安装Chrome和独立临时profile，有限回环API为合成后端。
原`test-web-app-assembly.mjs`、`test-web-console-browser.mjs`和缺导入负对照均通过；
实际生产HTML/ESM/CSP/IndexedDB在320/390/1280三宽执行，截图390宽已检查，无横向挤坏。
原请求在丢回应前持久化；真实reload/重新登录后只读恢复原ID，策略/应用标记不匹配不确认，匹配才归档。
浏览器使用合成身份，不使用用户原浏览器钥匙/站点数据，不改证书或操作设备页面。

同一最终测试/HTML/夹具，仅将app.js和network.js还原为6e1cd19原版本：
Node实际退出1，`Original ID must be durable...`；真实浏览器实际退出1，
`Original policy ID must be durable...`。均为实际行为断言，不是缺依赖/启动/编译失败。
不把一次JS调用当绝对单HTTP请求；测试覆盖写出响应头后丢响应体，预头断线的传输重放边界按成员报告保留。

### SDK来源和准备器

复用此前固定31项公开SDK/NCP缓存并重新按manifest核验；原compatibility、websocket-only、
官方mask backport及managed四段补丁的原哈希和前后文件校验保留。
只补原链已经固定的WebSocket1.5.3公开zip/mod/info，并按原版本元数据/Go sums验证，不升级依赖。
所有输入/输出均在导出源码的work/内，实际调用原managed prepare CLI及status.verified_stage，未放宽路径/来源门禁。
为原prepare读取Git提交提供任务专用bare对象元数据：从原仓库只读对象取精确267提交；源码仍为该Git归档，
没有另建成员checkout、改共享HEAD/refs或用编造提交号充当来源。SDK构建输入记录和实际--version均核对同一267。

## 失败和未运行项

- WSL/默认Windows Python缺Playwright导致装配未运行；改用既有专用虚拟环境后实际三宽通过，原失败保留。
- Core首次离线复验缺原锁`snow0.10.0`；只获取原Cargo.lock checksum匹配归档，原锁/版本不变，随后组合通过。
- SDK第一次stage复制了Go缓存的只读目录属性，原安全补丁无法创建临时文件；保留失败目录，
  新建第二stage，仅将新副本改为属主可写，重新走原全部哈希门禁后通过，没有改共享模块缓存内容。
- 一次本地namespace包装器把二进制当shell参数退出，另一次日志汇总误将无测试包的null当数值；
  均为夹具/统计脚本错误，未记作产品测试，已修正并保留原输出。
- Core ARM64 check因缺目标标准库失败；Go ARM程序只是构建。未独立重跑成员审计报告的全部53个筛选、
  受控候选变异、全仓测试/全部发行profile、实际Root kernel/cgroup/UID安装准入或手机/设备/公网验收。
- 五笔被审PR的精确HEAD check annotation均为付款/消费上限导致未启动；不将云端CI记通过，未改工作流或保护规则。

## 保留边界和下一步

固定用途签名有代码，Root/SDK/Core观察通道有显式候选，但完整CheckInTransport、
精确Statistics引用获取/释放、生产入口、链确认的可信证据/深度、长期开启策略仍待继续；
query候选只回unknown，节点accepted不等于confirmed，不能释放原Pending或宣称真实活跃安装上报完成。
默认OFF、reporting_available=false、原300秒有限Owner授权、共享Remote/Manual引擎及原Unknown不重发约束保留。
PR268仍是原窗口草稿，不抢做、不合并；蓝牙250/259/262及其他草稿保持，热点延期安排不变。

审核账本按实际作者yh1030记录5笔固定HEAD为offline_accepted，steps为空，不虚构旧C01～C24完成。
原Git作者保留，由team.py按实际合并main刷新贡献；候选本机通过不计作设备或整项验收。
整体验收仍81.5%，20/169、加权10.6%及原分母/权重不变。本轮没有生产签名、上传、安装或分配设备epoch。
