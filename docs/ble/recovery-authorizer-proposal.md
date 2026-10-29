# D-21：独立恢复授权与持久迁移提案（待审核）

日期：2026-10-29。基线 main `e48072ba9760c18f10c44847a1c6d94863eafd91`。
本文件是 RecoveryAuthorizerV1 的接口/兼容提案，不是已实现的恢复服务。
对应 BLE2-01；D-21 仍未完成，D-22～26 未实施。不能据此批准真实设备操作。

## 现状与不能复用的入口

| 实际来源 | 当前行为 | 对新增恢复授权的约束 |
|---|---|---|
| `crates/policy/src/lib.rs` | Scope 位 0～9，ALL_SCOPES=1023，Kind 仅 Owner/Client/Nas | 不追加 scope 位、不改原位含义、不新增伪 Owner |
| `crates/auth/src/lib.rs::network_manager` | 近期 Owner 或已批准且当前代次的 network.manage Client；Nas 拒绝 | 正常网络客户端有现有权限，但不是独立 wifi.recovery 身份 |
| `Auth::recent_owner` | 同一当前 Owner，原登录距今小于300秒；续期不刷新登录时间 | 只有这里的成功结果可创建/撤销恢复授权；调用时重新核验 |
| `Auth::recovery_challenge/recovery_finish` | 可信离线工具用原 Owner 密钥恢复被停用的 Owner；无远程命令 | 不能用作蓝牙恢复，不能要求 PWA 持有 Owner 私钥 |
| `crates/storage/src/auth.sql`、`lib.rs` | principals.scopes 约束<=1023；当前 schema=6；单 SQLite writer | 即使选 bit10，现有存储也拒绝；必须先审独立持久结构 |
| `Auth::durable_realm`、`Store::restore` | 普通重开保持 db_epoch；恢复轮换 realm、停用 principals 并递增代次 | 相同设备 pin 不代表同一授权 realm；恢复不能复活旧 grant |
| `crates/wire/src/auth.rs` | 严格命令/字段解析；没有 recovery_grant/authorize 或来源字段 | `channel=ble`、uid、bool、JSON capability 均不能提供可信来源 |
| `crates/core/src/network_trial/provision.rs` | 唯一 ACTIVE、完整 Principal 绑定、40秒截止、重复键/输入核验 | 复用一条事务，不能在 BLE 中另建 Wi-Fi 控制器或锁 |
| `crates/core/src/network_trial.rs` | 现有 USB/NM/线路检查 | BLE 例外只允许已验证恢复上下文；原 USB 路径保留 |

本 PR 的六个公开 API 夹具只验证上表现状。恢复授权不存在是待实现功能，
不是声称现有正常 network.manage 委托有漏洞；没有“修复前失败、修复后通过”的生产修复。

## 建议冻结的权限模型

推荐**独立持久 grant 表，同一业务库、同一 writer、同一 Auth/Core 权威**。
`wifi.recovery` 是此表中唯一允许的能力值，不是普通 Scope，也不进入 ALL_SCOPES。
恢复密钥不写 principals，不授予短信/通话/媒体/用户管理/系统/升级权限。
不得与当前 Owner/Client/Nas 公钥重复；被撤销的记录保留，不能删除后重用 generation=1。

建议的内部方法（伪签名，当前无实现）：

```text
grant(recentOwnerBearer, independentPublicKey, constraints, expectedGeneration, requestId)
    -> GrantView
revoke(recentOwnerBearer, independentPublicKey, expectedGeneration, requestId)
    -> GrantView
queryGrant(recentOwnerBearer, independentPublicKey) -> GrantView | NotFound
authorize(peerProof, verifiedSecureSessionProof, requestContext)
    -> CorePrivateRecoveryContext
revalidate(context, operation, trialIdentity) -> LiveRecoveryAuthorization
```

grant/revoke/query 都在 Auth 内重新调用 recent_owner；`recentOwnerBearer` 仅表示
输入 bearer，不能由调用者传入公开 Principal 后跳过认证。Store writer 再比较完整
Owner 快照，阻止排队期间的撤销/换代。三项管理方法只由既有可信管理通道装配；
BLE 数据端没有创建/批准/撤销 grant 的入口。

`constraints` 固定为 Wi-Fi 状态/开始/原操作查询/确认/取消；
`max_trial_ms` 在 1～40000 内，不能更改原NM checkpoint期限、WWAN/IMS或默认APN。
最多16个独立密钥记录（含墓碑），满时拒绝并保留旧证据，不驱逐旧代次。
grant 不以墙钟时间自动延长；持久授权直到显式撤销或 realm/Owner 换代。
临时会话/上下文使用本机单调时钟，最多300秒且不能超过安全会话剩余寿命；
单次 trial 仍以原40秒和请求/授权期限的最小值为准。已有产品期限不增加。

`CorePrivateRecoveryContext` 字段和构造器私有，无 Deserialize/Serialize/Debug，
不接受远程提供的 bool、Kind、UID、channel、grant generation 或 realm 作为证明。
它绑定 device pin、realm、当前 Owner id/generation、grant id/key/generation、
Core 进程实例、可信 peer/连接代次、安全会话 id/期限与请求 id/action。
它只能授权精确的恢复操作，不转成普通 Owner bearer。

## grant 持久格式与迁移（建议，未运行）

建议 schema **6→7**，新增 `recovery_grants` 和 `recovery_grant_receipts`，
不改 principals 的类型、scope CHECK、旧表或业务幂等键。
以下是字段约束提案，不是可交付的迁移 SQL：

| 表 | 字段与约束 |
|---|---|
| recovery_grants | id：服务端UUID主键；independent_public_key：32字节、唯一且有效非弱Ed25519公钥；device_public_key：32字节；realm：当前db_epoch；owner_id：引用principals.id；owner_generation：1..i64::MAX-1；generation：相同范围；active：0/1；capability：精确wifi.recovery；max_trial_ms：1..40000；format：精确1 |
| recovery_grant_receipts | realm + owner_id + request_id 唯一；action：grant/revoke；payload_digest：32字节；grant_id 与 resulting_generation；成功回执与grant同事务提交；最多256条，满则拒绝新变更，不删证据、不自动重放 |

请求ID沿用冻结 canonical UUID 格式。digest 由 Auth 内对固定版本、有长度界限的规范
参数编码计算，只用于请求去重，不作为签名或安全会话证明；编码必须在实现 PR 中附向量。
参数包含动作、独立公钥、constraints、expectedGeneration及当前 Owner/realm。
同 requestId/同参数返回原成功回执；不同参数拒绝。回执丢失时只查询原请求，
不创建新 requestId。失败/事务回滚无成功回执；generation 满、容量满均拒绝。

预期迁移过程：先读取且核对精确来源/数据版本、私有一致备份与剩余空间；在同一
writer 事务创建两个 STRICT 表、索引与约束，最后设user_version=7再提交。
旧 schema6 的 Owner、全部历史/墓碑/Unknown/事件和 realm 原样保留，初始 grant 为空。
首次初始化、v1～v5升级先走既有升级到6的路径；不得手工跳过旧迁移。
事务中途失败回到完整6；故意构造的7版损坏/不完整表结构或更高版本拒绝，
不能重建空授权表掩盖损坏。启动检查实际表结构/记录约束，不能只看 user_version。

普通重开保留合法grant，但全部旧进程上下文/握手失效，重新证明私钥后才可调用。
`Store::restore` 仍轮换realm并停用principals；保留的旧grant/receipt的realm不能重写为新realm，
恢复记录只作历史证据。原Owner经现有**离线**维护恢复后，仍须近期登录重新授权，
不能自动激活旧grant。Owner换代/停用也使所有关联grant无效；重新批准需新代次。
外部明文授权文件/固件镜像不能导入或覆盖该表；通用业务备份不包含浏览器私钥。

兼容门禁：现有schema6二进制应拒绝schema7，禁止提供“降级去掉表”的捷径。
实施前必须独立更新并测试UPD-01数据契约、当前/候选/回退三方schema支持矩阵与恢复工具；
本提案不代改成员C更新器/发行格式。没有能够读取schema7的安全回退组时禁止部署迁移。
这部分实现需主线/成员C接口评审，不通过本PR默认获批。

## 可信来源、安全会话与 Wi-Fi 事务的接点

1. D-22 Core固定 Unix socket 由安装器分配的实际低权服务UID访问。
   内核SO_PEERCRED及服务实例/连接断开用于拒绝冒用传输。UID不能证明恢复公钥的所有权。
   不在本PR分配UID、不增加可自选socket路径/Root执行口。
2. 成员A的 SecureSessionPortV1 验证设备pin、独立恢复公钥/代次、会话转录、方向/序号。
   其标准套件、密钥导出边界和具体 proof 类型须先获审；验证器必须在可信Core边界执行，
   不接受BLE进程自报“已认证”。若跨进程传递验证结果，先审认证IPC协议，不能只信任JSON摘要。
3. authorize 在本次连接上核验当前realm/Owner/grant及安全会话证明后构造私有上下文。
   缺端口/未批准/错误pin/旧代次/重放/超时/撤销/服务重启全部拒绝。
   #51 GATT库的可注入 SecurePort 只是适配边界，不能把测试端口安装为生产实现。
4. D-23将上下文接入现有Wi-Fi事务的单一Authority枚举（NormalPrincipal/RecoveryContext），
   在同一 ACTIVE 锁内排他。普通网络Principal原有语义保持；恢复操作不借Client/Owner伪装。
   begin、confirm、保存前和保存后分别重核同一授权、线路/后台和连接代次。
5. 不把Wi-Fi对象换代等同于SIM失效；使用既有Core网络/电话协调规则，
   不绕过正在通话的约束，不停止WWAN/IMS，不开启普通APN。
6. 撤销与保存必须有明确线性化顺序：同一授权gate/提交临界区内，若撤销先成功，
   禁止保存；保存先完成则记录Saved事实，随后撤销不能把它冒充RolledBack。
   保存期间断连/回执缺失保持原操作Unknown并查询；不能自动再begin或无限重试确认。
   NM还原失败不能释放为“已恢复”，全部清理须有真实结果。

撤销后只允许Core内部执行已经登记的固定清理，不开放已撤销会话的新业务命令。
重连即新连接代次，不收旧Future的迟到成功；查询原操作仍须新建合法会话并核验grant。
授权gate和NM异步commit的实际同步实现/锁顺序必须在实现PR证明，不能以检查前后一致
冒充没有竞态；本PR不声称已经实现该线性化保证。

## 浏览器绑定（成员A/B接口依赖）

PWA为每个已核对device pin/realm生成独立Ed25519签名密钥，私钥
`extractable=false`；只导出公钥供近期Owner在可信管理页面逐字批准。
私钥以CryptoKey存入IndexedDB，绑定pin/realm/授权公钥/代次和格式版本，不保存Owner bearer或私钥。
恢复密钥与现有基于密码的Owner密钥不同；不得由Owner seed派生或复制。
不支持非导出Ed25519/CryptoKey持久化时明确unsupported，不退回明文/localStorage或密码Owner登录。
Origin/浏览器存储删除后需重新批准新密钥，不能匿名首次认领。
IndexedDB的具体CAS/迁移、用户手势和旧pending保护对接B组件；
签名与加密协议交成员A，禁止D自创密码套件或改其crypto目录。
真实浏览器密钥非导出/重开/realm隔离尚未运行，不能据设计称已通过。

## 最小现状样例与实施后验收门槛

执行 `python3 tools/test-recovery-authorizer-boundary.py --run`：

| 真实夹具 | 输入序列与预期 |
|---|---|
| independent_recovery_key… | 已有Owner→独立密钥登录/离线恢复拒绝→合法独立签名bootstrap仍拒绝；principals完整不变 |
| normal_network_client… | 原批准Client的network.manage成功，近期Owner/批准管理拒绝；Nas即使有该scope仍不能管理网络；唯一Owner保持 |
| revocation_restart… | 批准→登录与待定challenge→撤销→重开→原token/回复拒绝→重新批准提高代次，只有新token可用 |
| business_restore… | 真实Backup API→新目录恢复→pin不变/realm改变/所有principal停用→只允许原Owner离线恢复，旧Client不复活 |
| existing_scope_mask… | 核对10个旧位与JSON枚举→真实Store拒绝bit10→关闭合成库后SQLite CHECK同样拒绝；原Owner重开保持 |
| payload_cannot_assert… | 真实wire解析可用WifiConfirm→加channel/uid/peer_proof/recovery_context均拒绝；伪新命令亦拒绝 |

这些是**现有边界保持证据**，不是新grant、安全会话或迁移成功证据。
实施后至少新增：grant/revoke成功与精确参数防重、错/弱/重复密钥、非近期Owner、
坏/更高版本、全部迁移中断、满容量/代次溢出、恢复旧realm、Owner换代、断连与旧Future、
真实SO_PEERCRED/不同UID可执行范围、合法BLE无USB、原USB保持、网页/BLE抢同一网络锁、
保存前后撤销顺序和失败还原、浏览器非导出密钥、Rust/JS真实密码互操作。
随后再做负责人授权的BlueZ/NSS/cgroup/设备/实际Wi-Fi验收。D-25远程媒体另交独立包，
TURN/ICE/DTLS/Opus不由本恢复授权授予权限。

## 本 PR 的审核决定点

请按本PR精确HEAD审核：独立grant而不扩scope；单writer schema7与回退门禁；
Owner/realm/代次/会话绑定；拒绝payload自证来源；保留原Wi-Fi事务与权限；
授权撤销/保存的线性化要求；A安全会话和C数据契约仍待各自接口审批。
审核通过才进入D-21实现及D-22/23接线；本PR不包含schema7迁移、Root账户、生产启用或部署。
