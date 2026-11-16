# A1原Root与受管SDK交易候选接线（2026-11-16）

PR266原有限RPC组件已进一步接原Root注册、严格私有stdio与实际SDK runtime，全部默认关闭。
本包完成Go侧候选接线和完整固定SDK验证；Core原Auth/许可/本地签名/CheckInMachine适配仍需实现。
没有启用生产上报、访问设备或公网节点、发交易、生产签名/安装或更改全局台账。

## 固定源码

- 实际作者yh1030，同仓库本人`codex/offline-a-statistics-rpc-20261116`，不自合并或推main。
- 原main：`6e1cd19bc9dd05fd150157c422590c1a801cc0b0`，包含原任务产品基线`2ae840fbffff04691ccf8cedd49d554093dd4032`。
- 本轮父HEAD：`ac7cd2875515c1b344a1520f9a23262bd059f80f`，即PR266初次RPC交付。
- 本轮实现/受测完整HEAD：`18cbd7c405d844d24de15eb115880217cf8f5fad`。
- 受测tree：`840732eece2456e060073ccb31d30b82d78362ce`。
- Git完整归档SHA256：`05a850a6c7b4617735be4e9a85efc1bb3fe4a36d69747de7cd0fc2e8ceb29465`。
- 随后仅更新A自己的说明/报告；最终PR完整HEAD在正文和回读中登记，不在本报告循环引用自身提交。
- 新实现共23文件：22个Go源码/测试文件及实际SDK prepare的原固定向量复制。无依赖版本/锁、四段补丁/manifest、B工作流/发行工具、Core/wire、BLE/AirSIM/声音、coordination、设备或成员审核记录变化。

## 实际接线

1. Root策略新增可选`usage_transaction_protocol`，缺失/0禁用、未知版本拒绝；版本1还必须启用原status_protocol=1。
   同时核实际SDK `--version` 能力、固定来源/程序SHA/原UID与策略。默认SDK二进制省略新字段，保留旧握手形状。
2. 只有双编译标记`nkn_sdk_candidate,nkn_usage_transaction_candidate`才声明能力1；
   私有launch未带能力时仍不创建RPC backend。字段null/0/未知版本/未编译新能力均拒绝，原12字段launch继续有效。
3. 保留原82字节RSCN头，新增候选10/11/12严格RSTX正文（最多2048字节）。
   实际内核Core UID、原generation/boot deadline/nonce/policy、固定统计公钥均要匹配；不是用请求里的参与代次代替Core许可。
4. 只允许独立的单次Core观察连接，原owner控制连接不得执行交易。取消/EOF/额外管道输入只取消该观察操作。
   仍需原真实进程/cgroup/exe/UID/能力健康证据；注入ready=true没有授权效果。主线原300秒上限不延长。
5. Root先做固定用途规范交易/Ed25519/真实摘要校验，SDK再核预检的原nonce及一次性槽；不透传任意交易。
   原87字节注册快照+RSTX回包保留原注册，结论再绑定操作ID/参与代次/公钥/原意图/摘要。
6. 原public Parse与普通/旧owned Serve均拒绝交易；单独显式owned入口才接受新有限枚举。
   加接口本身不能开放通道。status/usage回包里夹带transaction（包括null）也拒绝。
7. Root交易与status/usage共用原statusMu、1024帧预算，三种操作各5秒冷却。
   观察期限最多1750ms且不超过原Root期限；实际引擎IO最多1500ms，单次实际RPC仍最多1秒。
   nonce临时槽继续保留首次预检的最多10秒截止，重复请求不续期；原CheckInMachine总预算仍待Core适配按原对象贯通。
8. 取消以实际管道deadline中断阻塞读写；错帧/超时毒化本次共享观察管道，禁止重读迟到帧。
   不关闭/重启共享SDK、不调用Root stop/release；其他业务FD不在这条路径中。
9. 真实managed.Run只在原私有launch及编译能力同时允许、原租约仍活着且SDK source ready后调用RPC组件。
   accepted不是confirmed；候选回复类型根本不允许confirmed，query仍unknown，不清Pending或消耗槽。

## 真实验证

固定Go1.27.3 linux/amd64，WSL2 Ubuntu24.04，Linux原生Git工作树/源码/stage，TMPDIR=/tmp。
SDK整组执行在独立namespace，实际接口仅lo，UID/GID1000，全部CapInh/Prm/Eff/Bnd/Amb=0、NoNewPrivs=1。
只在该namespace启用回环；没有修改主机路由、服务或防火墙。

| 检查 | 结果与边界 |
|---|---|
| 未链接SDK的全引擎race/vet | 55顶层、195含子测试pass，0失败/忽略；测试tree f412c327c5469adb7c0417ddf1a3e8c96c781e11，后续只加prepare.py向量复制，Go字节一致 |
| 固定SDK链 | 原31SDK/NCP文件三重校验；compatibility/websocket-only/官方mask backport/managed四段原hash和前后逐项校验，原status.verified_stage与实际prepare均成功 |
| 原SDK受管回归 | 4项真实`TestManaged*`、race，通过；包括旧解析器panic重现和受管拒绝路径 |
| 默认SDK全引擎race/vet | 完整HEAD18cbd7c，55顶层/195含子pass、0失败/忽略；4个无测试包不计用例 |
| 显式交易SDK全引擎race/vet | 同55/195、0失败/忽略，和默认组重叠，不加成110或390 |
| Go全引擎gofmt -l | 0且输出0字节，覆盖所有新增Root/协议/SDK文件 |
| 默认/交易SDK实际amd64程序 | 两者构建及实际--version通过，来源均完整HEAD18cbd7c；默认不含新能力，交易版本精确声明1，均非fixture |
| SDK/Root ARM64 | 两个实际ELF交叉构建成功，e_machine=183；未执行、不称真机或QEMU通过 |
| 原规范编码 | 原15固定SDK向量，Root独立提取nonce后规范重建验签通过；额外字段拒绝 |
| 真实私有bridge→TLS/RPC | 真实原scoped dialer/合成CA/HTTP、nonce一次、丢回复重复submit实际一次、query unknown、原截止不刷新通过；此测试host不是Root进程或SDK bootstrap证明 |
| 原Root边界 | 实际Unix peer UID、四项注册绑定、禁原owner/缺策略/错误公钥、fake ready不产生IO、observer EOF不关闭owner、共享管道锁/帧序/冷却/取消毒化通过 |

产品程序哈希（只供复核本次构建，不是官方可安装包）：

| 程序 | 字节数 | SHA256 |
|---|---:|---|
| 默认SDK amd64 | 16879633 | `92e87c0ed7f81d0ed49ef9454e5a3d57ea6497afb73400075742da3b81bbf505` |
| 交易SDK amd64 | 16920034 | `643e580a321bd29a6021b3a804f2db076e58e30c4867eadbb53ddec82edd77b0` |
| 交易SDK ARM64 | 15726817 | `322b56258704ecc08add389e6d3c314d41f7b61f37ca35adfeea4fe3b8e99b03` |
| Root ARM64 | 5195225 | `55f8efb73d1d304c95960b8fd3fef28161843520ffaf3691c6755299a724fe5b` |

三项受控候选mutation均真正断言失败退出1（无编译错误）：

- 让旧owned Serve自动启用交易：`interface alone granted transaction capability`；变更文件SHA256 `cf96b10583be7b04f2de754d832806534004d1de7e6fc82b0f7d2ab55c65d3b0`。
- 去掉原registration.matches：`wrong original registration admitted`；`af2e5cb413655bc7fa5378ec2d99c00f7c16870fcd5c71c56653c400f77bae39`。
- 取消时不立即中断原输出读：`cancelled pipe read blocked`；`f3a7f93cb78381b28456ace635a3c428618562ef1125db7a911ae7a682cf95ce`。

这些是新候选的受控错误对照，不称旧生产实现存在同一完整交易通道。

## 失败保留与给B的输入

第一次完整SDK准备因原safe_path要求repository/work而拒绝任务目录；没有删除该门禁。
改为Linux原生精确HEAD工作树的work/，第二次因直接Python函数调用没有继承固定Go/cache环境，
原resolve-tag再次拒绝；后使用真实prepare CLI并显式传入固定环境，全部原门禁通过。
这两次是环境/夹具前置失败，原日志、退出码和来源均保留。未放宽SDK补丁、依赖、产品断言或能力限制。

只补既有固定WebSocket v1.5.3公开缓存：zip SHA256 `dbbd31dd0f08548c5dc43e4c2f12bbba66abff5252b224b1cd06afbb72783a76`、
mod `09dc8a9227a54c1069ea065d610a5f2ca195f1f5fba6e7dbb2d7eb0fd6e01df6`，原module sum及Go mod sum匹配。
生成stage的go.mod/go.sum在构建和测试前后完全一致；仓库原锁不变。没有升级固定版本或改补丁。

B从精确HEAD18cbd7c及后续只有文档的最终HEAD消费，沿用原四段校验和实际prepare，
按[接口说明](../offline-a/statistics-rpc-candidate-v1.md)分别测默认/交易组合。
实际prepare复制原固定向量，不提供重复造的自比向量。正式profile/8次独立复现/CI/发行签名安装仍由B/主线处理。
本机原始日志、精确源码、环境与正反例另打包，不能把本机可执行文件直接当作正式交付包。

## 下一步与未完成项

继续Core原Auth/recent Owner/realm/参与许可代次、PR265本地固定用途签名与原CheckInMachine的实际transport接线：
无Pending才预检、纯prepare、持久摘要后单次submit、重启只查原意图和摘要，贯通原单一总预算与取消。
再实测只释放Statistics引用而Remote/Manual继续运行。当前Go组件回传代次不等于Core当前许可验证。
真实Root安装资源/cgroup/SDK启动到链路的全程验收尚缺；内核拒绝单测与管道单测不能替代该闭环。
可信链交易/payload/区块/深度/重组证据和长期权限仍待明确；不猜confirmed、不延长300秒、不改reporting_available=false。
其余A0/A2/A3/A4/A5任务继续，全部A包未完成。热点专用延期，USB共享事务继续；不接管B或蓝牙窗口。

本轮不新增验收分：整体81.5%、20/169、加权10.6%及原权重保持。CI另按最终PR HEAD回读，不把本机通过写成云端通过。
