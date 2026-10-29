# D-21独立恢复授权：离线实现与启用边界

前置[接口提案](recovery-authorizer-proposal.md)已通过PR #80合入主线。
本批实现独立公钥批准/撤销/查询、显式存储迁移、同一Core的管理派发和浏览器密钥绑定。
没有实现安全会话握手、可信BLE恢复上下文或Wi-Fi恢复事务接线，这些属于D-22/23。
没有部署或为项目BLE整项增加验收分数。

## 默认路径与显式schema7路径

`storage`、`auth`、`core`新增无默认启用的`recovery-grants` feature；Core的feature传递给Auth/Store。
**即使`--all-features`，普通Store::open/restore仍保持最高schema6**，不会自动迁移到7。
现有生产Core启动仍使用普通入口，build-info继续声明database_schema=6。
不能把带feature的Core安装到已迁移的生产库并宣称可用，它仍会明确拒绝schema7。

显式离线库入口：

- `Store::migrate_recovery_grants(state, private_backup)`：先按既有路径初始化/升级到6，
  同一writer、同一文件锁内先通过真实Backup API生成完成的schema6备份，再事务性创建两张表并验证、提交schema7。
  不释放锁再重新打开；备份失败不执行DDL，DDL失败保留备份与原schema6。
- `Store::open_recovery_grants(state)`：只打开完整合法的7，不自动升级6。
- `Store::restore_recovery_grants(backup, fresh_destination)`：显式恢复7，保留既有realm轮换、
  全部principal停用/代次递增，以及旧prepared/submitting变Unknown语义。

这些是测试/后续经批准安装器可调用的库API，没有新增生产CLI、Root命令或自动迁移调用。
**UPD-01/候选与回退组契约尚未增加schema7支持；设备迁移/生产启动接线仍被此依赖阻止。**
本批不修改成员C的更新器、发行格式或Root材料；后续必须独立核对其明确版本范围。
回退生产代码不改变schema6；合成schema7只能由相应显式入口读取，禁止删表降级。

## 管理权限与回执

`Auth::grant_recovery_key/revoke_recovery_key/recovery_key/recovery_key_request`全部重新验证近期Owner。
Store执行前比较完整当前Owner，避免排队期间换代；独立密钥不能进入principals，
既有或已撤销的Owner/Client/Nas公钥、设备自身公钥也不能成为恢复密钥。
Auth复用已有非弱Ed25519公钥检查，重开时也检查恢复表中的密钥材料。
旧十位scope及ALL_SCOPES=1023完全保留。

表结构严格匹配固定DDL，检查外键、额外触发器/显式索引、行约束、最大记录数和回执。
格式损坏/高版本拒绝，不重建空表掩盖证据。
最多16个密钥记录（含墓碑）、256条回执；每个活动grant预留一个撤销回执位置。
新批准必须满足`回执数+1+变更后活动grant数<=256`；满容量拒绝新增/重新批准，仍允许撤销。
代次上限也预留撤销增量，撤销后不允许回到旧代次。

相同requestId和精确参数返回原成功快照；同ID不同参数拒绝；事务失败无成功回执。
摘要使用固定版本域、动作/公钥/约束/expectedGeneration/Owner/realm/pin的长度限定编码，
独立SHA256黄金向量见Store测试。摘要只用于防重，不是签名或安全会话证明。
重复撤销已停用项返回停用终态，不创建新变更/回执。
历史成功快照中的`active=true`**不能作为现在仍有权限的证明**。
记录还必须匹配当前realm/Owner/代次和可信会话；能力构造/重核由后续D-22实现。

## Core管理命令

四条严格解析的Auth协议2管理命令：

| op | 额外字段（均含token） |
|---|---|
| recovery_grant | public_key[32]、max_trial_ms(1..40000)、expected_generation、request_id |
| recovery_revoke | public_key[32]、expected_generation、request_id |
| recovery_key | public_key[32] |
| recovery_request | request_id |

request_id必须canonical UUID；expected_generation是canonical十进制**字符串**，0表示新记录，
上限小于i64::MAX。返回的generation/owner_generation也使用十进制字符串，避免JS整数精度损失。
没有新增`channel`、uid、peerProof、Owner角色或可反序列化能力字段，旧命令/字段保持。
未带feature时Core明确拒绝；带feature但普通schema6同样拒绝；只有显式创建的7版Auth/Store
能在离线Core IPC夹具中执行。所有命令使用现有管理Auth实例，不创建第二权限引擎。

## 浏览器绑定库

`configurator/src/security/recovery-binding.js`通过WebCrypto生成独立Ed25519私钥
（extractable=false），只导出公钥；按已核对device pin和realm分隔IndexedDB记录。
readBinding/getOrCreateBinding/rememberGrant/loadSigningKey/forgetBinding均校验绑定和版本。
实际IDB读写事务做revision CAS；并发创建只保留先提交的密钥，不覆盖；损坏/不支持时拒绝，
不清库、换密码Owner、退到localStorage或内存假成功。
刷新后保持CryptoKey非导出属性。忘记本地密钥不代表设备授权已经撤销。

rememberGrant只存管理适配器核对后的公钥/realm/pin及grant引用，不能制造Core权限。
此函数输入的投影格式为`{device_pin,realm,public_key,grant:{id,generation,active}}`：
pin/public_key为canonical64位hex，generation为字符串。管理适配器必须核对真实Core响应
与当前设备身份后显式投影，**当前尚未接UI/生产HTTP适配器**，不能直接信任任意网页JSON。
loadSigningKey只向经审核的安全会话库提供非导出CryptoKey句柄；本库没有定义握手、签名转录、
ECDH/HKDF/AEAD或网络请求，不修改成员A密码原语及成员B的UI/操作回执。

## 验证与后续

[交付记录](../deliveries/member-d-recovery-grants.md)列出精确HEAD、真实命令、结果和失败保留。
Linux普通UID上的完整workspace全feature测试、严格Clippy、默认feature拒绝路径，以及真实
Chromium/WebCrypto/IndexedDB均已验证。测试只使用临时合成库和本地回环夹具。

D-21离线实现可评审；D-22/23仍需经审核的A安全会话端口、可信Unix来源/连接代次、
同一Wi-Fi事务权限枚举、撤权与保存线性化证明。缺失时保持拒绝，不安装Mock端口。
D-24账户/许可和D-25远程媒体另包；ARM、设备、BlueZ、真实Wi-Fi/撤权、真人听音未运行。
不将13个浏览器存储场景称为安全协议互操作或完整BLE可用。
