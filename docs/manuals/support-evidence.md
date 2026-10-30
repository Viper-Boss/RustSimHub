# 支持范围与证据 · 草稿

[手册目录](README.md)

本轮资料基线为 `6b5de5f3ca5ff706adacd8f25ed7f638546739a9`，日期 2026-10-30。
本页是 C-15 手册的证据索引，不替代负责人冻结的 REL-01 正式支持矩阵。
“有限实机”只按链接记录的版本和场景成立；历史文档中更早的失败和待验段落仍须保留。
本次没有重新操作设备或执行这些验收。

<a id="s01"></a>
## S01 · 连接、身份、首次认领和配对

**有限实机验收 + 部分离线范围。**
[P6AE 正式首次认领](../VALIDATION-FORMAL-FIRST-OWNER-P6AE.md)记录原用户完成认领/登录、唯一所有者、窗口关闭和严格 HTTPS/公钥核验。
[P7A 网页接线](../VALIDATION-TEST-CENTER-P7A.md)与[网页身份实现](../../crates/edge/src/web/app.js)对应持久浏览器钥匙、正常登录和配对申请。
新增浏览器的完整批准、各浏览器恢复方法和所有首配网络组合不据此宣布实机通过；没有忘记私钥后的任意在线重置承诺。
本机手册只核对这些公开记录，没有读任何人的浏览器钥匙。

<a id="s02"></a>
## S02 · 检查状态、复制结果和测试入口

**已实现及网页离线验证。**
[P7A 简化首页](../VALIDATION-TEST-CENTER-P7A.md)记录实际生产 app 接线的状态检查、有限诊断复制和旧操作核对回归；
[当前页面](../../crates/edge/src/web/index.html)列实际按钮文字，含后续加入的 Bark 入口。
只读检查不代替拨号、声音、发送或恢复验收，复制功能也不等于任意截图都不含个人信息。

<a id="s03"></a>
## S03 · 收件箱、完整正文和候选比较

**有限实机验收。**
[P2H 短信实机记录](../VALIDATION-ONE-CLICK-SMS-P2H.md)证明该次实际收件与完整中文一致；
[P2E 候选两栏核对](../VALIDATION-SMS-CANDIDATE-REVIEW-P2E.md)及[P2G 日常短信](../VALIDATION-DAILY-SMS-P2G.md)说明预览、详情、候选与当前记录边界。
不同来源/不同 ID 不因正文相同自动合并；完整旧 NAS 页面或全部历史导入路径另验。

<a id="s04"></a>
## S04 · 短信发送、回信与余额

**一次正式场景通过。**
[P2H](../VALIDATION-ONE-CLICK-SMS-P2H.md)记录该次 10010/CXYE 的唯一操作/本地发送尝试、sent 与实际回信核验。
回信仅引导到运营商 App，没有余额金额；不据此支持余额查询，也不保证运营商网络绝对恰好一次或其他目标送达。
该次一次性短信授权已消费；编写手册不产生新的发送授权。

<a id="s05"></a>
## S05 · 持久防重、Unknown、停止与查询

**真实模块/网页离线回归通过，完整实机故障矩阵待验。**
[P2G 日常短信](../VALIDATION-DAILY-SMS-P2G.md)、[P3M 日常电话](../VALIDATION-DAILY-PHONE-P3M.md)及[P7A](../VALIDATION-TEST-CENTER-P7A.md)
记录提交前持久标记、同 ID 核对、重复点击、存储失败、注销、迟到回复、Unknown 和收尾边界。
这些结果支持“未知时不自动重发/重拨”的流程；不保证所有断电或运营商故障已完成实测。
[C09～C12 PR #10](https://github.com/Viper-Boss/RustSimHub/pull/10)已合并；
[成员报告](../deliveries/member-c-nas.md)的“主线接入与 WSL2 原生完整复测”一节记录真实模块的十五项恢复场景在 WSL2 通过，含原操作及 Unknown 保持。
这属于合成数据的隔离验证，不是当前设备全量恢复或硬件掉电验收。

<a id="s06"></a>
## S06 · 所有者回收站和最近登录

**正式页面单条删除/恢复闭环通过。**
[P2F](../VALIDATION-SMS-FORMAL-MANAGEMENT-P2F.md)记录重新登录后同一 ID 的删除/恢复及后台事件；
[P2D](../VALIDATION-SMS-TRASH-P2D.md)记录权限、事务、非安全擦除、SIM 存储不变与离线失败路径。
五分钟管理资格独立于普通读取会话；权限拒绝不一定只是过期。其他独立客户端完整补同步仍待验。

<a id="s07"></a>
## S07 · 一键通话与双向声音

**局域网短时场景有限实机验收。**
[P3L 实际双向验收节](../VALIDATION-ONE-CLICK-INCOMING-P3L.md)记录用户两端反馈、同一呼叫 accept/hangup、媒体帧和关闭检查。
[P3K 语音恢复](../VALIDATION-AUDIO-STARTUP-P3K.md)记录另一次 10010 完整下行听音、自动结束和 PCM 关闭；该次本身不证明双向说话。
只支持该次短通话结论；长期、远程 TURN、其他网络、欠载改善及完整竞争矩阵均不能据此写成通过。
约 30 秒是页面流程设计提示，终态仍需实际回执/观测。

<a id="s08"></a>
## S08 · 日常电话、拒接、按键与会话

**日常入口已实现/交付，部分真实场景待验。**
[P3M](../VALIDATION-DAILY-PHONE-P3M.md)记录拨号/接听/拒接/DTMF/声音控制的生产模块离线回归及实际交付；
[P3N](../VALIDATION-CALL-SESSION-P3N.md)记录签名凭证续期的实现与边界。
拒接、DTMF、多客户端控制、跨会话长通话不能由上述页面或短通话推定全部实机通过。

<a id="s09"></a>
## S09 · 加密身份备份

**匹配身份/数据库的历史受控场景有实测。**
[P1C](../VALIDATION-STATE-RECOVERY-P1C.md)记录实际加密身份、错误钥匙/pin/篡改拒绝、空目标恢复及匹配库读取。
[加密身份说明](../encrypted-identity-recovery.md)列该接口只包含设备身份、全新目标和密文限制。
历史场景不是当前全业务联合恢复证明，也不是整机/掉电/全量数据恢复验收。

<a id="s10"></a>
## S10 · 主业务恢复与跨库一致性

**接口已存在；当前联合恢复范围未闭环。**
[恢复语义](../recovery-clients.md)和[实际 Store 实现](../../crates/storage/src/lib.rs)区分身份、业务快照、恢复域及重新授权。
早期说明按其历史版本阅读，不能将当年的“尚无 HTTPS”等文字当作当前状态；当前身份密文边界见 S09。
[PR #10](https://github.com/Viper-Boss/RustSimHub/pull/10)已于 2026-10-29 合并，
测试代码提交为 `c9aed5f4ef3c84fec92b7ac4dd2f297fa70910a8`；十五项真实模块恢复矩阵在 WSL2 通过。
旧 `bfaa8dc` 包的 11 通过/4 失败仍是历史结果，不能用来判断更新后的源码。
[矩阵说明](../recovery/offline-matrix.md)和[一致性缺口](../recovery/consistency-gaps.md)仍区分单库恢复与跨域联合协议。
Store、身份、outbox、部署配置等没有在本稿获准成为普通用户“一键全量恢复”；不提供设备写入或整机恢复步骤。

<a id="s11"></a>
## S11 · 签名应用组升级与整机 OTA

**已初始化设备的正常签名应用升级有有限实机验收。**
[P6AF](../VALIDATION-INITIALIZED-APP-UPGRADE-P6AF.md)记录选中、健康核验、准确确认及原数据保持，明确同 schema 场景的限制。
[schema 准入](../VALIDATION-SCHEMA-ADMISSION-P6AA.md)说明旧程序与新数据库的维护边界；
[更新设计边界](../rtc-and-application-update.md)明确应用选择器不是整机 OTA。
[UPD-01 实机记录](../VALIDATION-UPD01-FIELD-20261028.md)补充真实签名程序、Root 配置和持锁数据比较后的暂存/确认；
[UPD-04 实机记录](../VALIDATION-UPD04-FIELD-20261028.md)证明该次候选 Core 异常退出后整组回退及新收记录保持。
[C13 矩阵](../update/failure-matrix.md)和[C14 矩阵](../update/crash-matrix.md)补充合成签名、真实 IO、2 MiB tmpfs 写满及明确 API 边界的 SIGKILL 重开验证。
这些隔离测试不证明内部 sync/rename 中断、eMMC 掉电持久性或所有版本可恢复；本次没有重新操作设备。
当前范围不提供已验收的普通用户整机升级步骤；不让“上传成功”“版本兼容”或服务 active 替代升级验收。

<a id="s12"></a>
## S12 · 热点、Wi-Fi 试连与保存

**安装材料有实机验收；完整首配流程仍有待验。**
[P4W 最终实机交付](../VALIDATION-SETUP-AP-PRIVATE-PREPARE-P4W.md)记录联合材料/权限安装完成，明确未开启 AP、试连、保存或手机交接；
[P4P 网页实现](../VALIDATION-SETUP-AP-WEB-P4P.md)记录查询、防重和页面接线；
[P4J](../VALIDATION-SETUP-AP-BOOTSTRAP-P4J.md)保留首次窗口语义。
不据已安装材料推断实际热点隔离、手机接入、保存后冷启动或证书交接已通过。

<a id="s13"></a>
## S13 · 网络读取、流量与主动测试

**有一次明确授权的链路/流量实测，范围有限。**
[P4B](../VALIDATION-CELLULAR-NETWORK-P4B.md)记录明确接口绑定、同意门槛及一次定量样本；
[页面网络入口](../../crates/edge/src/web/index.html)区分刷新状态、主动测试和保存策略。
单次样本不代表所有卡/网络的吞吐、计量故障或持续蜂窝测试授权。
本项目现行“不启用普通 APN、换卡后另行安排”的测试限制见[开发约定](../../AGENTS.md)。

<a id="s14"></a>
## S14 · Bark 与短信转发

**一次真实手机合成通知验收通过，其他业务范围分列。**
[X1Q](../VALIDATION-BARK-ONE-CLICK-END-TO-END-X1Q.md)记录用户收到一键合成通知、唯一任务/attempt、确认回执及默认关闭保持；
[X1P](../VALIDATION-FORWARDING-ONE-CLICK-DEVICE-X1P.md)保留更早页面交付及当时手机待验的历史记录。
[X1T 隐私规则](../VALIDATION-FORWARDING-RULES-PRIVACY-X1T.md)及[X1X 故障矩阵](../VALIDATION-FORWARDING-FAILURES-X1X.md)是后续源码/隔离验收。
这些不是本手册新增外发授权，也不意味着提供商投递、真实新短信转发及硬件冷启动都已验证。
通知包含信息和目标范围由负责人另行核对，本文不提供实际目标或令牌。

<a id="s15"></a>
## S15 · NAS 兼容、历史和旧页面

**有限只读及离线范围。**
[正式只读桥说明](../nas-online-bridge.md)、[主线 C01～C06 审核](../team/REVIEW-MERGED-PRS-2026-10-27.md)
及[原里程碑边界](../PROGRESS-SCALE.md)分别说明只读桥、成员离线验收和未完成旧页面/媒体范围。
旧页面离线标签不能独立证明 SIM 无法注册；NAS 控制、声音和全部页面适配不能由 HTTP 可达推定支持。

<a id="s16"></a>
## S16 · 升级版本比较与准入

**版本比较及有限维护准入已有分阶段验证；维护发起授权另列。**
[U3](../VALIDATION-UPGRADE-COMPARISON-U3.md)记录真实 CLI、数据观察、候选/回退比较及需要维护/拒绝的退出分类，
其中尚缺接线的结论属于 U3 当时的历史截面。
后续[U6 实现](../VALIDATION-UPD01-ADMIT-STAGE.md)及[UPD-01 实机记录](../VALIDATION-UPD01-FIELD-20261028.md)
记录有限准入命令对签名程序、实际生效 Root 配置、数据锁和候选/当前/回退的联合检查，UPD-01 已由主线记为完成。
该记录的 `authorization_verified=false` 仍明确保留：谁有权发起维护属于 UPD-02，不由比较结果授予。
本稿不把准入工具或 C14 的合成元数据比较转换成普通用户升级命令；当前源码以负责人交付版本为准。
