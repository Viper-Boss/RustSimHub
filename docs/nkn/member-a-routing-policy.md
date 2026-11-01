# A-26：实际有限路由策略库（候选，未接生产）

本轮从 main `ce9ccb8debb61db49860fe795c1f3e471b749f4c` 新建分支，承接已合并
PR129 的分层提案和 PR133 的监督库；不重交其代码。新增
`rustsimhub_nkn_agent::routing`，没有新增依赖、wire 编码、签名域或密码套件。
`production_ready=false`、`crypto_wire_frozen=false` 保持。合并提案不等于密码协议获批。

## 实现范围

`Router<TransportPort, BusinessPort, Clock>` 实际执行一会话、一在途请求策略。
`&mut self` 串行调用，没有监听器、调度器、线程、自动重连或第二业务队列。
仅从可信 `TransportPort::open_record` 取得完整认证记录，再按精确服务名路由：

| 服务 | 实际本地派发 | 真正提供者仍须完成 |
| --- | --- | --- |
| `core-auth-v2` | `BusinessPort::core_auth` | D 的普通 Core 桥，复用原 AuthRequest、UUID、Auth/Scope/realm/SIM/撤销与执行器；不得直接派发 raw AuthCommand |
| `call-media-v1` | `BusinessPort::call_media` | 原媒体 Permit/Ticket、peer/call/line/租约/撤销；只送有限信令，媒体传输仍用原授权和 DTLS |
| 其他任何字符串 | 拒绝并退休会话，业务调用 0 次 | 不解析为 URL、host/port、LAN 地址、socket 路径、CIDR、shell 或通用代理 |

上述服务目录仍供负责人/D 审核，尚未成为线上协议。`Service::parse` 不解析正文，
也不授予任何 Scope。现有 10 个 Scope、角色和 Owner 规则均未修改。
当前两种生产端口明确不可用：`Router::default().open(())` 返回 `Unavailable`；
`UnavailableBusiness` 不能生成成功回复。测试端口只在 `cfg(test)` 中编译。

## 可信生产者边界

`TransportPort::Admission`、`Session` 和 `Binding: Eq` 是提供者关联类型。
`SessionFacts`/`VerifiedRecord` 是进程内提供者输出，没有 Serde/Debug，不能当接收
peer JSON 的 DTO，也不能据 public 字段认为调用者已获认证。实现 trait 的本地代码
本身属于信任边界；此库无法认证一个恶意的 trait 实现。

真正 `admit` 必须复用经审核的成员证明校验，另验设备 pin、独立传输密钥、会话
purpose 和真实已注册 Core 客户端。成员身份、统计公钥、NKN 地址、连接成功或
蓝牙 bonding 都不能构造 Core principal。没有这些生产者就不可用，不能写默认
AlwaysAuthenticated/Mock。普通 NKN 与 BLE recovery 的 purpose 严格隔离。

完整 `Binding` 必须由真实 Core/可信传输端持有，等值比较覆盖：Core 实例、Core
连接 nonce、设备 pin、独立传输/会话 nonce、已注册客户端、realm、Owner/客户端
撤销代次以及当前 line/SIM 上下文。不得仅比较一个 peer 自报的字符串、地址或 UID。
库保留最初绑定，每次操作前、解密后、业务后和封装后重新 `inspect`，任何变更、
父连接断开、撤销或 provider 拒绝均退休原会话，不采纳新绑定、不自动续权。

提供者必须在真实认证中覆盖 version/purpose/direction/sequence/service/body 及完整
绑定；本库不定义这些字段如何编码、签名或加密。`version=1` 是此进程内候选记录
版本，不能替换 Core Auth v2，也不是已经冻结的远程 wire 版本。密码 nonce 由
获审密码实现管理，不能使用这里的路由 sequence 代替。

## 已执行的约束

- 完整输入/输出记录各不超过 2048B，正文各不超过 1024B、不得为空。这是保守
  **本地候选上限**，没有扩大 T5 BLE 预算，也没有声称该正文长度是已批准密文格式
  的准确余量。真实编码/tag/转录开销需 A27 获审后重新核算，必要时进一步收紧。
  提供者须在分配/解压之前守界；返回后检查不能挽救 provider 自身的无界分配。
- 接收和发送分别从 0 严格顺序增长；重复、跳号、乱序、反向记录和溢出拒绝。
  业务调用前消耗接收序号；发送序号仅交给设备到 peer 的封装端。最多 128 次请求，
  最后一份有效回复返回后退休会话，不能清零继续用旧密钥。
- 每次 admission/dispatch 共用最长 5 秒绝对预算；所有外部阶段共用原截止。
  更短的可信期限即时缩短预算；后续 provider refresh 永远不能延长最初会话期限。
  单调时钟回拨拒绝；恰好到期也拒绝。没有发明新的授权有效期或续租接口。
- `Revocation` 只是本地不可逆取消句柄。旧句柄不影响显式新建的独立会话。
  取消/错误/显式 close/Drop 各只退休原句柄一次；Drop 不证明 Root/SDK 无 PID。
  `is_open()` 只表示本地句柄尚未退休/取消，**不是实时授权或就绪证明**。
- 业务结果保持原 Core/media 正文，不生成“路由成功=业务完成”的新回执。进入业务
  后的超时、撤销、断连、绑定变更、异常回复长度或封装失败都为 `OutcomeUnknown`，
  丢弃待输出正文并关闭。只保留原业务 UUID 到 Core 查询，不自动重发或重建 UUID。
  `Denied`/`Unavailable` 仅允许后端在保证未准入执行时返回，否则必须 `Unknown`。

`BusinessPort` 必须在**实际执行/提交点**以原 Core 权限/租约重新核验并遵守同一个
`CallBudget`，不是只在调用前看一次布尔值。同步策略不能强占不守约的阻塞提供者，
前后检查也不能撤销已发生的副作用；后端不得分离工作到返回之后。主机若将策略
放进异步执行器，须持有原工作直到排空，调用者 timeout 不能释放在途槽并重试。
回复 bytes 返回到真正发送之间也必须由安全主机继续核验取消/连接/截止，不允许
跨会话排队后发送。此包没有建立这个主机或网络发送器。

代码不打印身份、正文、密钥、端点或 provider 原始错误。Vec 的释放不是可靠擦除；
不能宣称已经实现秘密内存零化。获审 A27 实现需单独核验敏感缓冲区、密钥和 Drop。

## 失败矩阵和接线顺序

34 项新测试直接执行生产 `Router`；仅外部密码/成员/业务端口和时钟合成。
测试覆盖双服务精确派发、成员与 Core 身份分离、BLE 拒绝、任意出口拒绝、
伪认证 JSON、帧/正文边界、十维绑定变更、重复/乱序/方向/计数耗尽、撤销/断连、
原截止贯穿/缩短/不可续期、时钟回拨、业务 Unknown 与封装前后失效、Drop/旧句柄。
这些测试不构成密码校验、真实成员证明互操作或 Core 集成通过。

下一步按精确接口审查：先批准普通 NKN 记录/身份签名域与完整绑定来源；A27 审核
并实现真实安全会话；D 提供普通 Core/媒体桥，不能拿 D23 的 BLE 恢复接口来冒充；
随后做真实 Rust/浏览器记录互操作、原 Auth/媒体失败矩阵和精确主机接线。SDK/Root
出口和 A22 状态采集另依其真实提供者。最终 C 组装、负责人签名/安装/设备验收。

来源：只读沿用 `docs/nkn/fixed-upstream.json` 的
nknguard `c32952d7586676fec10f71a30b22969cb24881f3`（尤其成员/发现/信令分层）；
本实现未复制其 Go 密码或转发实现，没有添加实际成员验证器。新 Rust 为
AGPL-3.0-only，OpenAI Codex 辅助；历史作者和来源保留。
