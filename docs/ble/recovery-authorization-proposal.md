# D-21 接口审查提案：唯一 Core 的有限 Wi-Fi 恢复授权

当前实现请见[2026-10-29局部交付](recovery-owner-management.md)：仅grant/审计/近期Owner管理已实现并通过离线测试；authorize/私有能力/浏览器与生产接线仍未完成。下文“未实现/80%”为10-28提案历史截面，不能替代当前81.5%及本批边界。

2026-10-29更新：用户明确允许先在独立分支离线实现，主线合并前统一审核；原提案未获独立审核的事实保持。A安全会话仍由A完成，服务和设备不启用。以下是原设计截面，实际实现及与之相比的差异须另附交付记录。

状态：**待审提案，未实现、未冻结、未启用**。2026-10-28。来源截面 `9f4ff04373846bd1555b3f471bc92072eaf5c412`（含Claude main dfd11e9与D-17～20离线库）。本文件只完成D-21的前置设计，不把D-21整步或BLE2-01标完成，不扣减成员D剩余6步。

按[任务书T6](../team/ADDITIONAL-OFFLINE-PACKAGES-2026-10-27.md)的 `RecoveryAuthorizerV1` 与“新增能力/位/存储迁移先审”执行。范围只包含恢复权限/存储兼容；密码原语和A的安全会话、服务UID/Root材料、远程媒体格式仍各自独立审查，不能随本提案默认获批。

## 一、实际源码事实

| 入口 | 当前实际行为 | 本提案不能假定的能力 |
|---|---|---|
| [policy](../../crates/policy/src/lib.rs) | Scope位0～9，ALL_SCOPES=(1<<10)-1；Kind仅Owner/Client/Nas；Principal为持久授权记录 | 没有Recovery角色/位，不能扩ALL_SCOPES或伪装Owner |
| [Auth](../../crates/auth/src/lib.rs) | recent_owner调用owner；authorize每次核对当前active/generation/权限；approve与revoke走真实Store | 不能拿此前一次登录的Principal快照永久授权，也不能让续期刷新最近登录时间 |
| [本地初始化](../../crates/auth/src/setup.rs) | LocalSetup是明确的首次所有者准备/窗口，有设备realm/pin和私有文件 | BLE恢复不能重开首次认领、创建第二Owner或重置既有身份 |
| [Store](../../crates/storage/src/lib.rs) | 当前SCHEMA=6，过新版本拒绝打开；真实单writer事务；备份/恢复有realm与Owner隔离语义 | 不能加一份未纳入备份/撤销的平行权限数据库，不能静默改变schema6含义 |
| [Core Wi-Fi](../../crates/core/src/network_trial/provision.rs) | owner函数要求recent_owner且NetworkManage返回同一Principal，OwnerGate在阶段点复核；Session存真实Principal | 不能把恢复请求造为Kind::Owner直接复用；不能跳过Gate、USB门禁或现有网络事务 |
| [Core IPC](../../crates/core/src/authenticated.rs) | 接受socket后用实际peer_cred/forwarding路由；并发受限；各路由有独立权限 | 同UID/任意socket连接不等于业务Owner，不能接受payload channel=ble作为可信来源 |
| [wire Wi-Fi](../../crates/wire/src/auth.rs) | WifiBegin带token/idempotency_key/SSID/passphrase，key1～128且无控制符；Status/Confirm/Cancel查询原trial | 不存在可从蓝牙直接调用的免token生产入口；不新造第二套网络事务 |
| [D-17～20](../../crates/ble/src/lib.rs) | 仅opaque传输/epoch/连接代次；缺注入安全端口拒绝；Mock仅测试 | Connection、Peer、SecureReply枚举都不是Core授权证明或密码验证 |

现有近期Owner规则、token有效期及续期语义以Auth当前实现为准，本提案不调整时间窗口。当前输入/版本限额也保持，不用延长租约或清记录消除失败。

## 二、推荐的权限模型（待审）

**保留全部旧scope编号、ALL_SCOPES、Kind和普通登录协议。** 新恢复授权独立绑定唯一Owner，但不是可登录的普通Principal，也不发普通Owner/Client/Nas bearer。恢复身份只能执行固定Wi-Fi恢复白名单，不能经常规接口调用短信、通话、媒体、客户端管理、更新或shell/AT/QMI。

1. `grant(recentOwner, recoveryPublicKey, constraints)`：Core真实Auth重新核对recentOwner、Owner类别、NetworkManage/ClientsManage与当前持久代次；明确批准独立恢复公钥。只存公钥/约束，浏览器恢复私钥非导出；不能复制Owner私钥，不复用页面普通登录私钥作默认恢复钥匙。
2. `revoke(recentOwner, grantId, expectedGeneration)`：同一Owner检查，真实writer中原子CAS增加代次/置撤销及审计。比较失败返回原状态的拒绝，不对“看起来是同一key”另建授权；撤销广播使活动上下文失效。
3. `authorize(peerProof, sessionProof, requestContext)`：Core自己验证批准过的安全会话证明、device pin、当前realm、唯一Owner仍有效、授权代次/撤销/白名单及请求新鲜性；返回Auth内部私有字段能力上下文。该类型不Deserialize、不由HTTP/IPC字节反序列化，不提供payload可调用的构造器。

公钥算法/编码、握手转录、nonce域和sessionProof格式**等待A的标准安全会话方案审查**。本提案不假定Ed25519/32字节就是最终浏览器恢复密钥，也不选择ECDH/AEAD或自创挑战签名串。未知套件必须拒绝，不能用固定签名/布尔authenticated/蓝牙bonding替代。

推荐由**Core终止安全会话**，低权BLE进程只转发opaque握手/密文；Core持有设备身份和恢复grant权威。这样BLE代理的固定UID仅证明获准使用有限传输入口，不能自行给Core下“已经认证”的结论。具体安全库接线须与A确认，不能在本包另写密码实现。

## 三、能力范围与事务接线（待审）

只开放T5已定的readStatus、beginWifi、queryOperation、confirmWifi、cancelWifi、disconnect。输入沿用SSID/口令、UUID请求ID、64hex设备pin/trial ID、40秒Core动作/45秒NM checkpoint/3秒查询预算。GATT调用5秒不改：长操作先返回受限starting/查询句柄，不能让一个安全端口future等待完整40秒。

恢复能力上下文绑定当前realm、Owner授权关系、grant id/generation、经过密码验证的会话和本次连接；SIM不是恢复身份的信任根，但电话/网络协调与WWAN/IMS保护仍照常检查。Core内部调用现有Wi-Fi执行器和**同一个网络锁/业务事务**，只把授权Gate扩为密封的Owner分支或Recovery分支；不制造Owner Principal，不复制另一份网络控制器。

- begin、confirm、Root保存前、结果发布均复核实际grant。撤销后只允许有限只读核验已接收请求，不能新begin/confirm。
- 请求账本键至少含 `(realm, grant_id, generation, request_uuid)`，旧代次/旧realm不能查询出新会话结果。同ID不同内容拒绝；已接收后丢回复只查同ID，不重做begin/confirm。
- Core接受动作前先持久记收据，再调用唯一网络执行器；崩溃/丢确认保留unknown。不存在以Mock返回saved=true代替Root证据的路径。
- Root保存已接受后发生撤销/断线，可能已经改变配置；必须核验原事务/Root回执，保留Unknown，不能谎称“取消成功、设置没保存”。最终线性化点和现有Root permit/审计契约要在D-23接口PR说明，不能声称跨进程检查具有数据库原子性。
- USB原门禁和现有Owner网页行为保持；未预绑定的无USB请求拒绝，合法恢复走独立受限能力。普通蜂窝APN不会因恢复Wi-Fi失败而启用，不能影响电话需要的IMS无线。

## 四、持久格式与回退：提议schema7，未批准前不写

推荐在**既有Store的单writer/备份事务**中增加恢复grant和受限操作收据，不加第二业务库。建议包含：grant id、当前realm、唯一Owner引用/代次、版本化公钥类型/编码、grant generation/active、约束、审计；收据包含请求ID、授权代次、动作、原trial和prepared/accepted/unknown/terminal状态。只留必要审计，不存Wi-Fi口令、私钥或可重放会话秘密。

这是**候选schema7设计**，不是已有版本，不擅自创建SQLite表或让schema6读者忽略新授权。具体DDL、字段上限、活动grant/待决收据/历史收据容量和保留策略必须在实现前冻结：全量/畸形/容量满拒绝，Unknown和幂等墓碑不得被静默回收后重做动作。不能用更大数据库预算遮住无限记录问题。

必须由更新材料接口审查同时解决：现装/候选/回退三方数据兼容、迁移失败原子回滚、旧schema6程序拒绝schema7后的安全退路、恢复备份后的realm/Owner变更隔离。不能在旧回退组仍只读schema6时自动迁移并健康确认，也不能清floor/schema绕过更新器。若采用其他兼容方案，必须给出旧读者真实行为和备份/撤销一致性证明，不能只说“新增表兼容”。

备份恢复后旧realm的恢复grant和会话默认无效，需当前Owner重新明确批准；首次所有者窗口仍保持关闭。损坏/未知未来格式、缺Owner或授权行不完整均失败关闭，不自动重置/补建Owner。

## 五、阶段交付与真正通过条件

| 批次 | 实现范围 | 要运行的真实检查 |
|---|---|---|
| D-21a，当前 | 读实际Auth/Store/Wi-Fi，提交本提案 | 明确旧位/协议/DDL影响、密码待审点及回退缺口；无生产变更 |
| D-21b，方案获审后 | 同一个Auth/Store中的grant/revoke/authorize、私有能力构造 | 真实SQLite CAS/中途失败/重开/迁移/备份恢复、Owner过期/撤销/旧realm/未知格式拒绝；无普通token提权 |
| D-22 | 固定Unix IPC、真实SO_PEERCRED、Core会话终止与opaque桥 | 临时真实socket/进程，错UID/冒充来源/旧连接/重放；Root权限不足用例必须标未运行 |
| D-23 | 现有Wi-Fi Gate和唯一事务接线 | begin/confirm/save各阶段撤销/断连、网页并发、未知回执只查；USB/IMS/APN原拒绝矩阵不能删 |
| D-24 | 单独账户/服务/Root材料提案并实现 | 不擅占UID，关闭无进程/广播/会话，真实NSS/BlueZ/cgroup由设备主线另验 |
| D-25～26 | 独立远程媒体兼容提案及跨模块故障 | 不把本提案当TURN/DTLS/Opus实现，不更改既有票据/line/call/peer/租约边界 |

新增密码/数据/Root协议各自审查通过后才接生产。当前只完成源码核对与提案，不启动迁移、访问设备、发送业务、配置蓝牙或改变进度done。未运行任何“恢复授权实现测试”，因为实现尚不存在。成员剩余仍D-21～26六步，原P0–P7约80%、当前交付台账20/169及权重保持。
