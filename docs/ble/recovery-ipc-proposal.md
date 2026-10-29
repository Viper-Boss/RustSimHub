# D-22前置：可信Unix来源、安全会话接点与旧PR收敛

2026-10-30。基线main `5238670246e7c6e43184534a7a6ee4b13c293ffd`。
PR #84已合并，独立grant与浏览器绑定以当前主线为准。
**本文件是待审核接口/接线提案，不是已实现的BLE IPC服务或安全会话。**
本轮仅增加真实内核/既有Core失败样例，D-22不计完整完成。

## 补齐旧分支核对与唯一数据权威

此前只检索到D-01～20及#80前置，没有覆盖以下仍open的旧局部交付；本轮已读取实际源码补齐。
不能直接把两个都叫schema7的结构当作兼容，也不能把相同类型名当作同一权威。

| 旧PR/精确HEAD | 核对到的实际内容 | 本轮接续决定（待审核，不修改旧分支） |
|---|---|---|
| [#60](https://github.com/Viper-Boss/RustSimHub/pull/60) `d3de1956fbc8fbbcd4bcc36bdda2c603cc1b06ed` | grants含profile/可变长度key、owner_key、session_ms/request_budget；另有recovery_audit，32个grant | 与已合并#84的固定Ed25519公钥/max_trial_ms/16记录/请求回执不同。不得再创建或覆盖同名表；不合入其旧schema7迁移 |
| [#61](https://github.com/Viper-Boss/RustSimHub/pull/61) `bc9fb583debcf58d3bdebed9665ab1b631525328` | 同一Auth中的私有连接、ProofBinding、验证器、动作能力、期限/请求墓碑；无生产验证器/IPC | 复用其能力/连接语义和失败向量设计；实施时移植到主线grant，不能复制其依赖的旧授权表或直接接受payload“已认证” |
| [#63](https://github.com/Viper-Boss/RustSimHub/pull/63) `ad8b87a812317018994bcb5f6df4d00ce3a1842f` | 旧schema7→8的业务请求账本、单次claim/Unknown | 这是D-23动作执行账本，和#84的grant管理回执不是同一东西。后续单独移植/审兼容，不在D-22静默迁移到8 |
| [#53](https://github.com/Viper-Boss/RustSimHub/pull/53) `9a95b239ee5adf3eb5c7a7f9e87b72b1f1dc6efb` | D-24候选服务材料，候选UID/GID982、不可安装标记 | 只作为既有提案引用；没有实际NSS/安装授权，不能把982直接写入生效策略 |
| [#51](https://github.com/Viper-Boss/RustSimHub/pull/51)、[#59](https://github.com/Viper-Boss/RustSimHub/pull/59) | GATT库与清理失败后quiescence修正，仍未合并 | 真实GATT接线前须审核这条依赖链；不能把cfg(test) SecurePort装为生产后端 |

以主线#84表及其固定DDL作为当前唯一恢复记录格式。保留旧分支及合成库证据，不自动采纳
#60/#63的实验schema7/8，不改user_version冒充转换；如需读取旧实验库，另提明确来源、
备份及字段映射方案。旧profile无法无损映射到固定32字节Ed25519授权；不能“取前32字节”。
当前生产默认仍schema6，没有本次设备迁移。

成员A依赖：本轮核对main、可见PR/分支，未发现可接的`crates/recovery-session`或
经审核的SecureSessionPortV1实现；冻结文件仍`crypto_wire_frozen=false`。
这不是算法已获批或统计PR可作安全会话的证据。本提案不自行选择密码套件或实现A的密码库。

## 实际来源：运输身份不是业务授权

当前`authenticated::serve_inner_media`在接受连接后从`UnixStream::peer_cred()`读取UID；
正常模式只允许配置的Edge UID，转发模式另有Browser/Root/Worker三角色。
当前`SocketRuntime`绑定私有目录/锁/精确socket inode，负责清理自己的socket。
这些可复用，但**不能把BLE UID追加为Browser或Root路由**，否则会开放普通AuthCommand面。

候选端点：`/run/rustsimhub/ble/recovery.sock`，独立于现有Core/媒体/转发socket。
安装器预建父目录owner=实际Core、group=实际BLE、mode=02750，socket为0660；
Root安装阶段设置组继承，Core不在运行中chown、不加入任意补充组。
BLE不能写父目录、业务库或identity目录。继续复用SocketRuntime私有锁和精确inode清理。
路径与UID角色须由负责人/D-24安装契约审核后固定，不提供网络可改路径/UID参数。

角色许可候选：

| 角色 | 候选恢复socket许可 |
|---|---|
| 独立BLE进程的实际UID | 仅受限opaque握手/密文/关闭运输 |
| Core、Edge、forwarder、network、Root或其他UID | 不作为BLE peer接入；Root管理走既有可信安装/Owner管理路径 |
| payload中的uid/pid/channel/蓝牙bonding/设备路径 | 不作为许可证明，不能覆写内核凭据 |

配置校验要求BLE UID非0且不同于所有已配置角色；仍要在每条真实接受的连接上核SO_PEERCRED。
UID/GID/PID只表示内核运输对端；**同一进程的两个连接可以有完全相同凭据**，必须由Core
为每个accepted stream生成独立32字节随机nonce及本Auth实例域，断线不复用。
不能只凭PID防重，不能仅以`UnixStream::pair()`内传的UID字符串冒充实测。
实际不同UID进程与上述父目录组继承需隔离Root/NSS试验后再验证；本轮未运行这些Root步骤。

## 候选运输帧（待审核，不是A密码线格式）

建议独立版本化二进制运输封套，不复用通用AuthRequest透传：

```text
u32 big-endian body_length
body = version:u8(1) | kind:u8 | opaque_payload
kind = 1(handshake), 2(encrypted_request), 3(close_transport)
```

T5应用信封上限保持2048字节，运输body最多2050字节（额外2字节仅为IPC版本/kind），
4字节长度头不计应用信封。读取长度后、分配正文前拒绝0/1/>2050；未知版本/kind拒绝，
close_transport必须零payload。握手/密文的最小/合法长度由获审A端口继续验证。
不能调用普通64KiB read_frame之后才把结果当作已经通过2048字节预算。
响应使用同一有界封套；错误只映射T5有限错误码，不返回原始解析/密码/设备数据。

封套没有Owner bearer、普通AuthCommand、明文SSID/口令、源UID或“已认证”布尔。
`close_transport`只关闭本次连接的运输资源，不是撤销grant或取消Wi-Fi的业务操作。
每连接一个在途验证/请求；最多一个BLE活动连接/安全会话和一个仍需排空的验证工作项。
读写/证明阶段有界；沿用5秒调用预算、3秒查询预算和后续Core40秒试连上限，不能增加产品租约。

## 安全会话与私有能力的收敛接口

复用#61的`RecoveryRequestVerifierV1`、私有`RecoveryProofBinding`和`RecoveryCapability`
思路，按主线#84字段重建适配接口，而不是重引入旧RecoveryKey/profile表。

| 绑定来源 | 必须匹配的字段 |
|---|---|
| 实际Core/Store | 设备公钥/pin、db realm、当前唯一Owner id/generation、grant id/独立公钥/generation/约束 |
| Core accepted stream | 本Auth实例、私有连接nonce、真实peer凭据、连接是否仍活动 |
| 获审A验证器 | 已验证会话标识、方向/序号/防重、认证解密后的request UUID/六动作/正文、剩余有效期 |

这些不是由JSON构造的Proof。Context由Core创建，只向可信适配器提供只读访问；
验证器配置只在可信启动装配中注入，默认None立即unavailable且不生成能力。
未获审A端口不得使用Mock、`source=core`或#84的grant管理回执代替。
grant元数据的`active`标记本身不够；增加Store内单writer读取/复核：同realm、当前Owner
active/generation一致、记录与设备/独立公钥匹配且未撤销，前后均核对，再铸造私有字段能力。
能力无Deserialize/Clone/普通Principal或token转换，只准许固定六个恢复动作。

#61现有适配器调用是同步Auth方法；接入异步Core前需审执行/锁边界：
不要跨外部await或未知耗时验证持有全局Auth mutex。建议Core短锁取不可变证明上下文，
在独立有界验证工作项执行A端口，回到短锁后重新核全部Store/连接/期限再生成能力。
验证超时不代表工作已经停止：保留占用与JoinHandle/清理责任，迟到完成只可排空，
不能恢复旧连接或对新连接投递成功。验证器替换/移除及断线都使旧能力失效。
只读业务和普通Core不应等待一个失控BLE验证器；没有BLE/A实现时Core照常运行。

期限使用Core单调时钟；不以远端时间或新session id刷新同连接额度。
会话上限300秒、单连接最多128请求作为候选全局硬预算，不强行从#60缺失字段转换；
最终值须结合A协议审核。每次Wi-Fi动作再受#84 max_trial_ms与原40秒/45秒门禁限制。
内存请求集合不能替代D-23跨重启执行账本；#63的单次claim/Unknown仍需另外评审移植。

## 本轮真实现状样例

`crates/core/src/recovery_ipc_contract_tests.rs`通过实际SocketRuntime、Core serve_until、
Tokio UnixStream、内核SO_PEERCRED及真实Python子进程运行，未创建替代授权状态机：

1. 子进程发送伪造uid=0/pid=1/channel=ble/authenticated=true的JSON；接受端看到的仍是真实
   内核UID/GID/子进程PID，普通Auth解析也拒绝该payload。
2. 同进程两个真实连接的UID/GID/PID完全相同，证明这些字段不能代替连接nonce。
3. Core配置一个与实际peer不同的允许UID时，真实监听器在公开Identity派发前关闭连接；
   关闭服务后socket清理、Store无principal。这里没有创建另一UID进程，不能冒称跨UID运行通过。
4. 配置匹配UID时公开Identity可读，但无Owner证明的恢复管理仍被拒绝；Store无新principal。
5. 通用Auth合法Identity加JSON空白至2049字节仍可解析，说明通用64KiB预算不能当作BLE信封预算。
   这是既有通用协议的正确行为及新增解码器需求，不是宣称发现生产漏洞。
6. 使用#61继承自#60的逐字schema7 SQL在临时真实6版库上生成旧7版；当前显式恢复入口明确
   Corrupt拒绝，源数据库字节/版本/旧表保持，不自动解释为#84新7版。夹具只由cfg(test)引用。

这些只证明**当前边界/接线需求**，不是D-22新端点、A会话互操作、合法BLE恢复或Root许可已通过。
实现后仍须补：真实不同UID/Root角色拒绝、socket gid/mode/FD传递边界、全部帧拒绝/截断/超时、
连接/owner/grant/realm轮换、慢验证迟到与清理、A互操作、#51/#59真实Transport接线和
D-23事务Gate/持久Unknown。不得把合成外部证明统计为真实密码验收。

## 本PR审核决定点

请审核以#84为唯一grant格式、旧#60/63实验库不自动转换；独立socket与最小角色；
2050字节运输封套和真实peer来源；移植#61私有能力语义时的适配字段/锁/迟到处理；
A验证器未就绪时保持拒绝。接线实现需此接口和A安全会话协议获审，安装角色仍单独审。
本轮不改旧PR/其他成员文件/生产协议/数据库/服务，不合并、安装或关闭其他PR。
