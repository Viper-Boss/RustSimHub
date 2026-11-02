# D23 恢复命令接原Core Wi-Fi执行器

2026-11-01（Asia/Shanghai）。本包让#140的私有同正文参数、#138的原trial/独立控制能力和#139的持久观察进入**已有**`network_trial::provision::run_trial`，不另建Wi-Fi状态机。基线含最新main `ce9ccb8debb61db49860fe795c1f3e471b749f4c`。沿用已合并#103的流程批准，新代码/兼容变化仍需逐HEAD审核；不当成设备或发布验收。

## 一份原实现与兼容影响

原`coordination`、`network_trial`及provision文件现在由Core库编译；Core二进制改为导入同一库模块，文件没有复制。电话Worker、普通Wi-Fi命令和恢复入口因此引用同一GLOBAL、TRIALS及ACTIVE。没有“库一份、二进制另一份”的锁和actor。跨crate所需的原Rust函数可见性改为public；这是可信Core内部库API扩展，不是新远程命令或权限。

普通入口、配置开关、Principal/口令校验、40秒、文件Audit格式、NM与USB门禁保持。Session所有权明确分为Normal(原完整Principal及内存指纹)、Recovery(获核原trial)、RecoveryClosed；普通Owner/Client不能控制恢复trial，恢复控制也不能操作普通trial。恢复终态清除内存中的原请求/凭据引用，只保留原ID和状态等元数据。

## 实际恢复调用

`network_trial::provision::recovery(auth, AuthorizedRecoveryRequest)`仅在编译了`recovery-execution`且原provision开关已启用时可用。调用者必须保留实际接受的RecoveryChannel；入口不启动监听器、不安装verifier、不制造SO_PEERCRED或A密码证明。默认生产Auth仍无标准A适配，默认Store仍6，入口不能使未认证请求通过。

- 在既有blocking Auth worker上运行#140封闭正文解码。查询只读原UUID，不申请网络锁或生成执行票据。
- Begin先检查原ACTIVE/试连槽位及global电话/网络协调锁，再登记、领取和分配原随机trial。普通事务/电话繁忙时，不先写一条无法派发的Reserved墓碑。
- 控制先检查原Session的Recovery所有权、trial、Ready/剩余期限，再预留原单格控制队列；队列满或未Ready不消耗新的持久控制Claim。登记前还核同一原会话/target，之后仍须消费私有票据并绑定独立Confirm/Cancel。
- 原actor启动/连接/就绪时写#139观察；确认保存前同事务记录Begin+Confirm的saving，backend.commit前后均核**对应Confirm能力**。Begin、Cancel或普通控制证明不能代替它。
- 使用原actor、原NM backend和45秒有限checkpoint。恢复超时直接使用原Auth绝对单调deadline（40秒/grant/原能力最小值），不是每轮重新计时。失败仍走原最多5秒精确补偿；只有backend确实返回补偿成功才记录rolled_back，否则Unknown。
- 撤权、断连和到期后仍可凭原私有Claim记账真实补偿/Unknown，不恢复执行权。写终态失败时仅尝试保留Unknown，不宣称成功。未知回执继续阻止新BLE副作用，普通Owner维护不因此被冒称解决旧请求。

原trial还持有不可从payload构造的原Store记账接点；终态事实不再等待Auth mutex。实测发现初稿在Auth锁被其他工作占用时，backend已回滚却无法更新旧进度、内存只剩Unknown；新增真实失败回归后，改为凭原私有trial/Claim直接在原writer记账。进度/saving仍核当前Auth，但writer fence另外取原trial绝对deadline的最小值，晚到的进度不能利用更长会话期限越过原窗口。

内部返回`RecoveryReply`区分既有快照、已登记启动、实际终态及查询；不是已经冻结/序列化的T5 Operation DTO，不返回SSID/口令，不允许从返回值构造权限。丢回复只查原请求，不能重新领取；开始和控制的副作用不会因等待者丢失就被假报成“从未执行”。

## USB与既有硬件边界

普通`NetworkManager::snapshot_inner`仍必须得到有效USB fence，之后仍核原usb0连接。只有持有私有`CoreRecoveryTrial`的原NM backend分支可调用`snapshot_recovery`，记录明确Recovery fence；不能由payload channel/uid或布尔关闭USB检查。该分支保持相同wlan0、NM/MM owner、空闲电话、WWAN不变、无普通蜂窝autoconnect及profile/checkpoint检查。未关闭WWAN/IMS，不增加APN或隐式蜂窝回退。

本批验证了源码/策略分支和实际Core权限接力；**没有**实际系统BlueZ/NM/Root权限、无USB硬件配网或RF清理验收。不能把单个USB策略测试称作真实NM/设备通过。

## 关闭、凭据和取消边界

恢复actor持有原试连/网络锁直到实际backend工作、补偿和记账结束。控制调用等待者被取消，不丢掉已开始的actor或提前释放锁；真实通道失活仍经原Gate触发有限补偿。可信Core shutdown可请求停止，但不能凭此生成Confirm能力。原单格队列、8秒控制等待、5秒补偿和有限checkpoint保持；队列已被占用或RPC尚未返回时，不承诺关闭立即完成。

终态后ACTIVE的Recovery owner改为无凭据的RecoveryClosed，排空的backend/context在释放排他锁前销毁。测试以Weak引用确认通常排空路径不再由Session长期保留凭据；若blocking读取正在返回，它仍可能短暂持有引用，进度写入受原deadline限制，不能据此延长执行权。这不是对serde/NM/编译器全部临时副本的内存取证擦除保证。release沿用panic=abort；普通取消/丢回复测试不等于进程崩溃或panic隔离验收。

## 本轮真实测试

新增14项恢复actor测试、1项普通USB拒绝测试、1项Storage原deadline拒绝测试；已有13项普通网络/配网测试随原实现移到库中执行，断言保持，不重复计数。测试调用真实原actor/排他槽位/电话锁/Auth/SQLite/解码器；仅外部A证明和NM backend是合成适配器。

覆盖独立确认保存/取消、保存前后断连/撤权、部分持久化和补偿失败、Unknown拒绝新副作用、普通/恢复身份互拒、电话/试连繁忙不登记、未Ready/队列满不领取、丢回复后锁和actor实际排空、缩短绝对期限、可信shutdown、错误普通控制证明及终态释放凭据。测试没有连接设备、系统D-Bus、真实Wi-Fi或蜂窝，也没有伪造正常Principal来执行恢复动作。

精确命令、HEAD、完整回归和证据摘要见[交付报告](../deliveries/member-d-core-native-recovery.md)。剩余仍包括实际A安全会话/host及真实SO_PEERCRED整装、T5 wire/前端适配、Root/UID/发行schema9兼容、私有NM/硬件故障与设备联验；本包未部署。完整阶段保持21/26已提交、剩D22–26五步，固定P0–P7约80%。
