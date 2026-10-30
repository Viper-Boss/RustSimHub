# T3 / T4 / T5 / T6 · 四位成员追加离线任务与主线边界

2026-10-30分配更新：用户要求wnfhsj88接手fungi1220的全部A-01～30。当前A无待办，B共62步；下文历史阶段与原粗估保留，实际当前归属以roster.json及offline-ownership.json为准，接口/审核/设备边界不变。

日期：2026-10-27，最新为T5接口固定与T6最大化离线交付。用户明确授权“把能分出去的任务分出去”，并要求继续写在本文件。本文件是追加包的完整任务说明；个人任务书保留原包并链接本文件。可独立离线交付的实现、构建、测试与工具交给成员；真实设备/生产密钥签名、接口与权限方案审核、集成和最终验收归负责人。分配不是完成声明，本次不启动设备操作。

## 五人分工（当前T6口径；下文保留各次追加记录）

| 人员 | 原包（保留） | 正式追加 | 累计成员步骤 / 已验收 | 全部分配计划工时 |
|---|---|---|---|---|
| Viper-Boss负责人主线 | 接口/权限方案审核、真实环境、签名、集成和发布 | 当前UPD-01/U4收口；65项主线执行及90项成员实现的审查/集成 | 不将团队步骤折成主线百分比 | 未估造 |
| [fungi1220](MEMBER-A-USAGE-STATS.md) | 原统计/NKN/依赖/扫描包保留 | T5 NKN状态；T6引擎/会话/邮件/安全，详见下文 | 30 / 0 | 39～88小时 |
| [wnfhsj](MEMBER-B-NETWORK-ACCOUNTING.md) | 原网络/测试中心/性能/回执包保留 | T5配置器/PWA；T6网页装配/发布材料/报告 | 32 / 0 | 28～63小时 |
| [yh1030](MEMBER-C-NAS-COMPAT.md) | 原NAS/恢复/升级矩阵/制品包保留 | T5 AP失败测试；T6更新/系统材料/NAS控制音频/证据 | 30 / 6，仅离线通过 | 35～80小时 |
| [RedamancyGTW](MEMBER-D-SMS-RECOVERY.md) | 原通信观测/灯控输入包保留 | T5 GATT适配；T6恢复授权/Core/远程媒体 | 26 / 0 | 33～75小时 |

表内工时是截至T6全部分配包的粗略工程计划，不是剩余工时、实际工时或承诺期限；不与项目台账重复求和。成员交付步骤按T3的60步、T4的76步、T5的96步，扩大到当前T6的118步，均映射原有169个项目动作。旧包新增估时保留在对应历史章节，项目冻结估时/权重不改。**全局仍14/169、加权7.3%，原P0–P7仍80%**。C既有6步审核及原作者/提交保留，当前扩大范围后为6/30；依据见[精确HEAD审核](REVIEW-MERGED-PRS-2026-10-27.md)。未验收不代表未开工。

当前90项有成员离线负责人，65项保留主线专属执行。28→35→43→90为各次扩大涉及动作的历史变化，不是完成量；当前155项未闭环及41个真人验收点保持。90项的主线审核、集成和最终验收仍保留，不能将整个动作视为已交付或已完成。

## 执行顺序与依赖

1. 每人先提交手上当前批次的小PR；等待审查可只读预研追加包。接口未批准不同时造第二套实现。
2. A第一批统计契约获审后，可接NKN边界/依赖盘点；最小引擎实现及构建等待A第三批接口审查及NKN-01前置。
3. B计量第一批获审后，可接测试中心独立组件；T6已进一步开放有限页面装配，按对应边界与审批顺序实施。性能工具可随后离线交付，真实预算由主线确认。
4. C先完成当前C-07/08历史导入第二批（C-05/06已审），再接备份测试。涉及UPD-01/02未冻结版本格式时，先交测试/复现/提案，待契约冻结再实现相关格式断言；REL-01未冻结前用户手册保持草稿。
5. D第一批对象生命周期获审后，可接CALL-08，与短信第二批分开PR；相同observer入口不同时开两条相互覆盖的修改。
6. 依赖审查可以保留需要修改结论；已提交不等于已通过，CI/merge不自动增加成员验收步骤。

## T3文件所有权与交接（历史包；最新追加路径以T6为准）

| 范围 | 离线负责人 | 可写边界 | 主线保留 |
|---|---|---|---|
| 统计库 | A | 原usage-stats范围 | Core注册、Owner授权、真实NKN/设备 |
| NKN引擎/构建与清单 | A | 新tools/nkn-engine、nkn-build、dependencies及本人文档 | 真实公网连接、VPN/路由/Root进程、主线依赖升级和签名；Go复用不能冒充Rust |
| 网络计量 | B | 原network-agent/network.js范围 | 真实NM/nft/SIM、数据开关与设备测量 |
| 测试中心 | B | 新test-center.js、独立组件测试/合成页和接线文档 | app.js、静态路由、页面模板/全局样式、真实声音和发送 |
| 负载与采样 | B | 新tools/performance、docs/performance | 实机阈值、长测和旧新系统同功能比较 |
| NAS兼容/历史 | C | 原compat/Go只读桥范围 | 真实NAS页面、控制、声音和端到端 |
| 备份/更新矩阵 | C | recovery/update独立测试、测试区域最小接入及新隔离脚本 | storage/Core/forwarder生产逻辑、Root安装、签名、设备与真实掉电 |
| 用户手册 | C | 新docs/manuals草稿 | 真实支持矩阵、最终发行包与步骤批准 |
| 短信/终止通话观测 | D | 原收件/observer最小范围及storage calls观测/记录映射、独立测试 | Core派发/鉴权、通话控制、媒体、wire/schema、网页和真实设备 |

成员不得跨表修改他人实现。公共接口或schema确需变化，先在自己的交付报告提供最小反例、具体接口/迁移diff与兼容影响，由主线协调；不得靠测试专用替代实现、清库、合并同号码记录、放宽权限或自动重试获得绿色结果。

追加范围对原任务书禁止条款的例外只限明确列出的路径和动作；没有概括开放仓库。蓝牙密码/身份协议仍待主线冻结，不在本次追加包中擅自实现；硬件BLE、普通流量、PMIC/关机继续保持原状态。

## 分支与PR

沿用[同仓库协作规则](README.md)：本人账号、本人codex分支、PR到main，负责人审核合并；不fork、不直接推送main、不改别人分支。原分支存在且有未提交工作时先检查，不能重建/强制重置。

建议新包独立分支（从最新origin/main，或经说明的已审依赖）：

| 包 | 建议分支 |
|---|---|
| A NKN边界/依赖 | codex/member-a-nkn-contracts |
| A 引擎/ARM构建 | codex/member-a-nkn-engine |
| B 测试中心 | codex/member-b-test-center |
| B 性能工具 | codex/member-b-performance-tools |
| C 恢复矩阵 | codex/member-c-recovery-matrix |
| C 更新矩阵/手册 | codex/member-c-update-failures |
| D 通话重观察 | codex/member-d-call-reobserve |

使用[交付模板](PR-DELIVERY-TEMPLATE.md)，原本人delivery报告追加日期/批次，不覆盖前批证据。逐PR写起始SHA、完整HEAD、对应步骤/动作的离线子范围、真实命令/退出码、复现/修正前后、许可来源/AI辅助、未运行项和主线接线点。新PR不能沿用旧HEAD审核。工时只有本人自报，没有就是未知。

## 验收与面板

成员不改roster、reviews、全局进度、权限设置和签名发布；主线按精确HEAD审查，合并后刷新真实Git贡献。追加计划不会预写成功、代码行数、提交数或工时。任何离线子包通过也不自动完成对应整个项目动作；真实设备、提供商、用户听音/拔插及最终集成各自保留。

本次只更新任务书、分工和公开面板，没有给成员发送站外消息、运行他们的开发任务或操作设备。用户可直接将自己任务书的GitHub链接发给对应成员，其Codex从该任务书继续即可。

<a id="t4-offline-packages"></a>

## T4 · 当日第二次正式追加（按账号领取）

本节为用户本轮批准的4个包，覆盖SEC-01、SEC-04、WEB-02、WEB-06、UPD-07、LED-07、SEC-06的离线子范围。成员先交当前批次小PR；同一成员一次只推进一个修改包。没有验收的接口依赖先提交只读盘点/提案，批准后才接代码。新包不要求等待其他所有成员完成，也不同时修改同一公共入口。

共同交付规则：同仓库本人`codex/`分支→PR到main；记录起始SHA、完整HEAD、真实命令/退出码、每个步骤的证据、失败/未运行项和主线接线点。仅使用公开源码及合成数据，不访问设备、系统D-Bus、真实蓝牙/NKN/运营商或生产服务。不得扫描`.private/`、`.local/`、`.git/`、真实备份或用户目录。发现超出本包的生产问题，提交最小复现/修正提案，由主线修改；不擅自扩权、变更schema或新增依赖。下列文件权限是原任务书的具体追加例外，其他禁止项仍有效。

### fungi1220 · A-17～20 · 权限清单与公开产物扫描

建议分支：`codex/member-a-security-inventory`。依赖：先交当前统计包；A-11依赖盘点获审后开展本包，复用既有清单，不造第二套版本来源。新增GATT未实现入口只列为计划/未知。

| 步骤 | 要执行的动作 | 验收条件 |
|---|---|---|
| A-17 / SEC-01 | 按固定SHA只读盘点HTTP、IPC、媒体、文件写入及固定Root动作；逐入口记录源码位置、调用者、scope、身份/线路绑定、期限、输入上限和撤销路径 | 已实现/计划/未知分开；每条结论有实际符号/位置。Core、Root、GATT授权设计只提出缺口，不修改 |
| A-18 / SEC-04 | 新增只读扫描工具，默认只输出计划；显式扫描Git已跟踪的公开文本白名单或隔离合成发行目录；限制文件/总量/时间，拒绝越界路径、链接及特殊文件 | 使用有限规则识别私钥块、Wi-Fi口令字段、令牌和疑似个人通信字段；输出规则ID/相对位置/统计，不回显命中原文或值；无上传、联网、自动修复/删除 |
| A-19 / SEC-04 | 正例使用明显标记的合成秘密，反例覆盖公开公钥/摘要/示例字段；覆盖超限、无效编码、未知二进制、重复路径和读取失败 | 错误/跳过不显示“全部干净”；退出状态区分通过/发现/未完成；支持按规则说明误报，不能直接全局忽略凭据类别。扫描通过不等于已证明制品无任何秘密 |
| A-20 / 交付 | 提交权限表、扫描脚本/测试、合成报告与CI接入建议；执行真实脚本和测试，附公开输入清单与输出脱敏证明 | 不修改Actions或生产打包器；主线决定正式发行白名单、真实私有制品审查和修复。SEC-01/04全局仍待主线审核 |

可写：新增`tools/security-inventory/`、`docs/security/interface-inventory.md`、`docs/security/public-artifact-scan.md`及本人交付报告；可只读复用`tools/dependencies/`清单。禁止改Core/policy/wire、Root单元、依赖版本、签名器、生产配置和全局面板。扫描器检查的是本地公开构建资料，不探测任何联网服务。

### wnfhsj · B-17～20 · 回执与配置状态组件

建议分支：`codex/member-b-operation-receipts`。依赖：B-09～12测试中心组件契约获审后开展，复用已有业务模块；WEB-06的主线前置FWD-04已完成。若实际操作DTO尚不一致，先交字段/适配提案，不能自行统一业务状态机。

| 步骤 | 要执行的动作 | 验收条件 |
|---|---|---|
| B-17 / WEB-02 | 对照phone、sms-management、forwarding、ap-wifi、session的真实结果，列出prepare/提交/确认/Unknown/归档矩阵；区分登录过期、权限拒绝与设备未确认 | 保留原操作ID及提交标记；查询、刷新、渲染和登录均不会产生提交。现有IndexedDB防重语义保持 |
| B-18 / WEB-02 | 新增独立`operation-receipt.js`显示组件，输入为有限快照与显式callbacks；区分仅查询、已证实可归档、等待确认和操作完成 | 组件只渲染文本/发送显式用户动作回调；无设备访问/网络轮询/私有存储。Unknown不能自动归档、重拨、重发或改幂等键；主线业务模块继续决定允许动作 |
| B-19 / WEB-06 | 新增独立`configuration-state.js`，展示保存意图、设备已保存、服务启动/停止确认与陈旧/未知状态；用真实组件覆盖超时、退出、旧代次迟到回复、乱序及重新登录 | “保存成功”不冒充进程运行；无有效新鲜证据保持未知。凭据不进入DOM/报告；与B-10共享的组件接线必须串行，不能各自覆盖测试中心 |
| B-20 / 交付 | Node实际模块测试＋真实浏览器本地合成页1440/390/320、键盘和长错误提示；提交最小接线说明 | 测试必须加载交付的组件；模拟仅限外部callbacks。主线装配app.js/HTML/静态路由并进行真实设备确认；WEB-02/06不由成员标完成 |

可写：新增`crates/edge/src/web/operation-receipt.js`、`configuration-state.js`、`tools/test-web-operation-receipt.mjs`、`tools/test-web-configuration-state.mjs`、`tools/fixtures/operation-receipts/`、`docs/web/operation-receipt-integration.md`及本人报告。既有业务模块、session/IndexedDB、app.js、HTML、全局CSS、Rust路由只读，接线以文档提案交主线。需要import新组件的test-center.js仅在前一包合入后最小修改且明确列出。

### yh1030 · C-17～20 · 内核/DTB/模块/固件一致性

建议分支：`codex/member-c-kernel-artifact-audit`。依赖：先交当前C-07/08历史导入批次；备份/更新故障包与本包串行。UPD-07无需等待UPD-01完成，检查器只读，实际生产制品缺失即保留未核验。

| 步骤 | 要执行的动作 | 验收条件 |
|---|---|---|
| C-17 / UPD-07 | 对照`firmware/base-system.lock.json`、`board.toml`、内核公开锁定资料及既有verify-kernel-source-tree/verify-audio-boot-recipe脚本，建立版本和来源清单 | 复用已有锁定事实，不另造相冲突的manifest；分别列kernel release/来源提交、DTB板型、模块vermagic/依赖、固件许可/摘要，缺失值不猜测 |
| C-18 / UPD-07 | 新增离线只读一致性检查器，默认只列计划；只读取显式传入的公开/合成manifest及隔离制品目录，不执行被检查的程序或脚本 | 摘要、架构、板型、release、必需文件和声明的模块依赖逐项核对；限量/路径/链接/重复条目/未知版本失败明确；报告仅含允许公开的制品事实 |
| C-19 / UPD-07 | 合成匹配和混版样本，覆盖错板DTB、缺模块/固件、摘要变化、冲突release/vermagic、损坏ELF/元数据及清单不完整；复用已有校验模块 | 断言真实检查器的通过/拒绝/未核验结果；不能只比较手写JSON，也不能用任意合成哈希声称681原装模块或真实boot已匹配 |
| C-20 / 交付 | 提交检查器、合成样本、输入格式、执行结果及主线补验清单，说明公开锁定事实与真实制品证据的区别 | 不写镜像/分区、修改boot/DTB/驱动/NV，不加载模块、不签名；主线取得实际制品后再完成重建、签名及实机验收 |

可写：新增`tools/kernel-artifact-audit/`、`docs/kernel-artifact-audit/`和本人报告；既有锁文件、固件、内核重建器、两个verify脚本、UPD/Root安装生产逻辑全部只读。检查器不得读取`/dev`、自动下载闭源/私人固件，或跟随主机用户目录中的维护材料。

### RedamancyGTW · D-13～16 · 灯控与通信输入失败矩阵

建议分支：`codex/member-d-led-input-boundaries`。依赖：当前短信第一批及D-09～12通话身份边界获审后再接；同一observer/Store测试接入点不并行改动。范围仅LED-07与SEC-06中的短信/通话及灯控输入，HTTP/IPC/媒体/BLE其他解析器仍归主线。

| 步骤 | 要执行的动作 | 验收条件 |
|---|---|---|
| D-13 / LED-07 | 对照真实status-led策略、Core状态输入、storage indicator映射和系统单元，列固定路径/UID/租约/标记生命周期及SIM隔离的源码证据 | 不修改用户已确认的颜色、优先级、跑马灯次序与低频闪烁预算；实际sysfs/UID/硬件仍列主线待验 |
| D-14 / LED-07 | 直接调用真实策略/已有Store接口，使用临时文件与可控时间测试旧代次、过期/超远期租约、损坏输入、切换配置/升级/未读状态；有现成文件适配器才复用假sysfs目录 | 过期/旧SIM不能作为当前配置或新未读生效；不读取真实sysfs/设备节点，不另写一套灯控实现替代被测代码。不支持目录注入时交缺口，不开放任意sysfs路径 |
| D-15 / SEC-06部分 | 在真实公开解析/存储入口上运行有限、可重放的合成边界语料：UTF-8、空/超长/二进制字段、非法枚举/数字、截断及重复/乱序事件；固定种子、次数与资源预算 | 无新依赖的确定性测试优先；不新增PDU解析器、生产HTTP/IPC入口或schema。拒绝不推进游标、不产生发送/拨号副作用。无法到达的内部入口先交最小测试接入提案 |
| D-16 / 交付 | 跑`cargo test -p rustsimhub-status-led --locked`、storage/受影响Core针对性测试、严格Clippy/fmt，记录最小失败输入、次数和环境；提交修复提案 | 本T4只补测试/审计和最小cfg(test)接入，不改灯控策略、鉴权、限额、控制/媒体或生产解析语义。发现真实缺陷由主线另行修正；真实灯效和完整SEC-06仍未验收 |

可写：新增`crates/status-led/tests/offline_policy.rs`、`crates/storage/src/offline_input_boundary_tests.rs`、`tools/input-corpus/sms-calls/`、`docs/testing/led-input-boundaries.md`及本人报告；storage lib.rs仅添加该cfg(test)模块。需要status-led私有函数时先提最小测试hook，不扩大公开API；Core/observer/系统单元本包只读，原D任务的最小生产修正不能混入本T4 PR。

### T4主线接手与未转交项

1. 主线按完整HEAD复核源码、用例实际执行和原范围未回归，必要时独立运行；每个成员步骤可单独接受，C现有6步不会重记或撤销。
2. 公开资料扫描、权限盘点及制品检查的报告需要负责人判断；离线阴性结果不等于生产发行无秘密/无漏洞。正式Actions接线、签名、实际Root权限与数据锁、设备/NAS/运营商/蓝牙全部保留主线。
3. B的组件接线由主线统一装配，不增加第二套会话或提交状态机。D测试不修改主线正在做的升级准入模块，C的制品检查也只读核验来源。
4. T4时蓝牙网页/PWA/Mock BlueZ尚未追加；同日用户后续授权的T5见下文。T5仅开放冻结的离线组件/传输边界，真实密码握手、恢复授权和设备启用仍归主线。T4各包无需等待蓝牙。
5. 任务书/名册/审核数据与面板同步，不新增项目动作、不改估时权重或完成状态，不伪造贡献/自报工时。本次没有给成员发站外消息；用户可直接转发本文件链接。

本轮分工维护验证：18项既有看板单测、Node计算/卡片/转义检查、HTML/Markdown生成一致性和`git diff --check`通过；另核对16个新增步骤逐项存在、名册76步/已审6步/35个涉及动作、四份任务书的T4锚点，以及原动作内容/状态/权重/估时、既有审核与主线Git代码归属均未改变。这些是分工与面板验证，不是新增功能验收。GitHub功能分支/PR及Pages发布按实际回读区分，推送不等于已更新在线main面板。

<a id="t5-frozen-interfaces"></a>

## T5 · 接口固定后继续转交8项的离线部分

用户已授权“定好接口后，依然写到刚才那个md文档里”。本节就是正式接手规格；常量和合成向量分别在[接口清单](interfaces/t5-offline-v1.json)、[跨模块输入向量](interfaces/t5-vectors-v1.json)。固定接口ID为`t5-offline-adapters-v1`，源码对照基线`3eb42bca52768d2f08c81cd8eb860e4fef681d59`。接口清单是组件边界，不是正在运行的HTTP/Core ABI；不得将JSON字段直接映射成任意AuthCommand。

本次固定**浏览器—恢复客户端、GATT传输—安全命令端口、NKN引擎—状态展示、既有AP事务—故障后端**四个接口。主线仍须完成BLE1密码握手/跨语言加密互操作、BLE2恢复授权与强类型IPC、生产BlueZ/Core接线和硬件验收；这些尚未实现，`production_ready=false`、`crypto_wire_frozen=false`保持。**没有安全后端时真实命令必须拒绝，禁止回退明文、默认允许、Mock认证或第二Owner。** 这让成员先做确定的离线部分，不将完整BLE协议伪称已交付。原项目依赖不改；离线子包可以按本契约先行，整个动作完成仍须满足原前置及实机证据。

### I. 浏览器恢复客户端 · RecoveryClientV1

```ts
type RequestId = string; // 小写规范UUID；每笔写入独立生成并先持久化
type TrialId = string;   // 64个小写hex；复用直接Wi-Fi引擎ID，不能当AP流程的UUID
type DevicePin = string; // 64个小写hex的设备Ed25519公钥；从原可信管理渠道核对
interface RecoveryClientV1 {
  connectFromGesture(input: { expectedPin: DevicePin; signal: AbortSignal }): Promise<void>;
  readStatus(input: { signal: AbortSignal }): Promise<RecoveryStatusV1>;
  beginWifi(input: { requestId: RequestId; ssid: string; passphrase: string; signal: AbortSignal }): Promise<OperationV1>;
  queryOperation(input: { targetRequestId: RequestId; signal: AbortSignal }): Promise<LookupV1>;
  confirmWifi(input: { requestId: RequestId; trialId: TrialId; signal: AbortSignal }): Promise<OperationV1>;
  cancelWifi(input: { requestId: RequestId; trialId: TrialId; signal: AbortSignal }): Promise<OperationV1>;
  disconnect(): Promise<void>; // 只释放本地会话；不声称设备已回滚/操作已取消
}
```

- `connectFromGesture`必须在点击调用栈内直接发起浏览器原生选择；不能先等待登录/存储异步工作而丢失用户激活。核验pin、已预注册的独立恢复身份及grant generation由**主线认证实现**完成。广告名、MAC、GATT已连接、前端传`verified=true/channel=ble`都不是授权。没有提前登记不能匿名认领或重置Owner。
- `RecoveryStatusV1`严格字段为`{contract:1, source:'core'|'mock', wifi:'connected'|'disconnected'|'unknown', busy:boolean, recovery_available:boolean}`。仅认证后可读；不包含SSID/地址/卡信息/业务记录。`source:'mock'`只允许测试页面，必须标“离线模拟”。
- `OperationV1`严格字段及枚举以清单为准：`contract,source,request_id,action,trial_id,state,authoritative,settings_saved,rollback_verified,expires_in_ms`。`action`为`begin_wifi|confirm_wifi|cancel_wifi`；state沿用`starting|connecting|ready|saving|saved|rolled_back|unknown`。`trial_id`可为null（未拿到ID），后三个证据/期限字段可为null。`saved`必须`authoritative=true,settings_saved=true`；`rolled_back`必须`authoritative=true,settings_saved=false,rollback_verified=true`；其他状态不能显示保存/回滚已成功。`ready`尚未保存且必须由当前有效授权明确确认。`unknown`不声明终结。
- `LookupV1`严格为`{contract:1,source,target_request_id,lookup:'found'|'not_found'|'unknown',operation:OperationV1|null}`。found时必须返回匹配原请求ID的记录；其他情况operation为null。**not_found也不是未执行证明**，不能据此自动重提写入或清除不确定记录。
- 未知字段、版本、状态和类型拒绝；错误仅返回`{contract:1,code}`，code限接口清单。不得返回远端错误正文、口令、原始帧或任意恢复URL。该接口不是现有Core命令的重命名：请求ID→原事务的持久映射、grant绑定、恢复后查询由主线补齐，当前缺失时返回`unavailable/unknown`，不能伪造查到。
- SSID按UTF-8为1～32字节且无Unicode控制字符；密码为8～63可打印ASCII或64hex。与当前`wifi_credentials_valid`及`ap-wifi.js::credentialsInput`一致。只在内存保留本次输入；JS清空引用不能声称安全擦除内存。密码不进入IndexedDB/URL/日志/缓存；断线/退出及时清空，重开需用户重新输入。
- 本地持久记录严格为`{contract:1,device_pin,revision,requests:[{request_id,action,trial_id,submit_started}]}`，上限16笔，revision为非负安全整数；无SSID、口令、会话密钥或完整响应。Store必须`save(next,expectedRevision)`进行单事务CAS（初次expectedRevision=null）；先记录submit_started再调用写方法，保存失败禁止发送。跨标签/刷新只查询原ID；ID记录不能在未知时清空或重建。到16笔保留记录并阻止新写，已证实终态后显式归档。
- Core试连40秒、NM checkpoint 45秒原预算保持；查询3秒截止，前台最多每秒1次、每次最多30次，页面隐藏暂停。写入超时/Abort/断连时视为结果可能未知，不重试；停止本地等待不会撤销已提交操作。授权在begin/confirm/save及主线后台边界重新检查，Core唯一网络锁、电话空闲和普通数据关闭规则不变。新的连接不延长已有事务。
- 信任存储接口由主线提供：`loadBinding(expectedPin)`只返回设备pin和非导出CryptoKey句柄/代次，缺失或撤销拒绝；B不得自行实现批准恢复身份或把Owner私钥复制到Pages origin。密码协议/随机数/AEAD nonce的真实格式不在T5开放实现范围。

### II. GATT传输与BlueZ生命周期 · GattBackendV1 / SecurePortV1

固定Service UUID为`002c85a2-c6d6-411c-b266-9e99fb4cdd0a`。四个characteristic UUID/flags在清单中：DeviceInfo Read；AuthExchange Write/Notify；Command Write；Result Read/Notify。仅允许Write with response，拒绝without-response/reliable/非零offset/prepare-authorize写；不开放任意特征值或通用配置命令。DeviceInfo仅提供协议版本、产品名、需要预授权标记和消息上限，最多256字节，不含设备公钥/地址/SSID/SIM资料；它不承担设备pin核验。

固定对象根`/org/rustsimhub/recovery/v1`，下设service0及四个characteristic、advertisement0；只管理自己的ObjectManager子树。广播只声明peripheral和固定服务UUID，名称可省略，不自动Power/Pair/Trusted/更改适配器。硬件/权限不足准确报unavailable。

冻结的后端调用名称（Rust实现用对应类型、`impl Future<Output=Result<...>> + Send`，不以任意字符串透传）：

```text
GattBackendV1.registerApplication(epoch, fixed_manifest) -> Result<()>
GattBackendV1.registerAdvertisement(epoch, fixed_advertisement) -> Result<()>
GattBackendV1.unregisterAdvertisement(epoch) -> Result<()>
GattBackendV1.unregisterApplication(epoch) -> Result<()>
GattBackendV1.notify(epoch, current_peer, fixed_characteristic, frame_bytes) -> Result<()>
SecurePortV1.receive(epoch, current_peer, Auth|Command, complete_bytes) -> Result<bounded_reply>
SecurePortV1.disconnected(epoch, current_peer) -> ()
```

- backend由构造器注入；Mock仅模拟D-Bus边界，必须测试实际注册/清理状态机。`epoch`为每次启动独立的非复用标识，peer来自BlueZ实际回调及其唯一D-Bus owner，不信任payload自报peer/path。旧epoch/旧BlueZ owner/第二peer回调拒绝。生命周期`disabled→registering→advertising→connected→stopping→disabled`，无法证明清理时为unknown。
- 先注册应用再广播；广播失败补偿本次应用注册。停止先撤广播、关闭本次会话/队列，再撤应用；Release与显式unregister幂等协调，已Release不重复撤同一广播。每个D-Bus调用5秒截止；迟到注册成功仍须本epoch补偿清理，不能只丢弃回调留下资源。无法证明清理保持unknown，不自动重新启用。BlueZ owner更换使旧所有权/会话失效，重开需显式动作。不存在全局“删除所有广播/设备”动作。
- 单会话、单重组中消息、最多4个待发逻辑结果；拥塞拒绝新帧/关闭受影响会话并保留Core未知操作，不丢成功回执后伪称已同步。每消息最多2048字节/256片，重组从第一片起15秒硬截止，不因新片延长；分片认证成功前也执行预算。
- 12字节little-endian头：`magic[2]='RB', version:u8=1, kind:u8(1auth/2command/3result), message_id:u16, index:u16, count:u16, total_bytes:u16`。索引从0连续递增，message_id每方向从1递增且同一连接不复用，溢出须重新建立会话；重复、跳片、kind/总长度变化均终止该重组，禁止无限补片。它只是分片关联号，**不承担密码学nonce或防重放授权**。
- 初始attribute预算20字节；后端有可靠协商证据才用`min(ATT_MTU-3,244)`，浏览器没有公开MTU时继续用20。每片payload为预算减12，除最后片外填满；`count=ceil(total/payload)`、total为1～2048、count≤256。首片固定该消息预算，避免跨协商混片。向量覆盖单片及20字节预算下的两片，不是加密向量。
- AuthExchange承载主线安全后端的opaque握手内容，Command只能提交经过该后端认证解密/重放检查的白名单操作；Result仅向本次已认证会话发其加密结果，未认证不读取先前会话结果。D只处理opaque字节和生命周期，**不选择/实现ECDH、AEAD或伪造VerifiedSession**。未注入主线SecurePort时命令一律拒绝；测试假端口仅存在测试目标，不能进入生产默认构造器/可配置运行开关。

### III. NKN状态展示 · NknStatusProviderV1

`readStatus({signal:AbortSignal}):Promise<NknStatusV1>`是唯一读取接口；不包含start/stop/connect或RPC交易。主线负责鉴权读取和把实际引擎计量映射为DTO，A可在自己的引擎范围实现输出适配提案，不修改Core。单次响应≤4096字节/3秒，读取和页面渲染不能启动引擎、测速或连接公网。

严格DTO字段为`contract:1,source,generation,sequence,age_ms,enabled,service,path,dht,rx_bytes,tx_bytes,rtt_ms,peer_count,last_error`；枚举以清单为准。generation/sequence为0～2^32-1，重启/重新启用必须换generation；同generation较小/相同sequence不覆盖新样本，旧generation的迟到异步结果由当前请求上下文拒绝，不能按数字大小猜新旧。age_ms为非负安全整数或null，超过5000ms或未知时间按陈旧/未知显示，墙钟回拨不能让它变新鲜。

rx/tx为规范十进制u64字符串或null（`0`可以、`01`不可以），范围0～18446744073709551615；前端用BigInt/安全字符串算术，不先转Number。rtt_ms为0～60000有限数或null，peer_count为0～100000整数或null；未知不填0。`source=mock`必须显示离线模拟，`unavailable`各动态字段未知；DHT连接、direct/relay、引擎running分别展示，不互相推断。peer_count标“连接节点数”，不能作为用户/安装统计。实际引擎没有提供的字段保持unknown/null，禁止模拟成功冒充真实引擎接线。组件日志/复制诊断不包含NKN地址、密钥、peer地址或业务正文。

### IV. AP故障测试复用 · 已有生产trait，不新增并行配网器

C使用`crates/setup-ap/src/station_trial.rs`的`StationNetwork/StationProvider/TrialGuard`、`transaction.rs`的`Gate/Guard`及`station_commit.rs`的`CommitNetwork/CommitProvider/CommitAudit`，直接调用两个实际`run`。`begin/usable/restore_ap`和`persist/finish`分别注入错误/延迟，使用watch取消；不访问真实NM/D-Bus。验证结果只接受实际`TrialOutcome::{UsableAndRestored,FailedAndRestored,Unknown}`与`CommitOutcome::{Saved,RestoredAp,Unknown}`。

固定10类case ID见清单；每case记录gate/audit及begin/persist/restore/finish调用次数和先后。未确认不得persist；部分保存后恢复无法证实必须Unknown；SSID/DHCP失败不能标互联网通过；取消/调用方退出须执行本次清理，旧代次完成不覆盖新事务。沿用生产window≤40秒及现有守卫，不拉长截止，不创建另一个状态机。不能由Mock产生真实Outcome替代被测函数返回。

### V. PWA与生产接线边界

固定路径和SW scope均为`/RustSimHub/configurator/`；以自身相对URL构建，根看板/发布器不变。静态允许清单≤32个文件、单文件≤512KiB、总≤2MiB，只缓存同源清单内静态GET；不缓存POST、设备响应、绑定库、IDB内容、凭据或mock后端。不接受任意URL/路径、postMessage授予能力或跨origin设备请求。离线功能要求此前成功缓存，首次离线不声称可用。

B提供静态资源/相对路径/版本化缓存及有限更新提示，主线负责统一Pages打包/真实发布与生产后端安装。实际UI会话/未确认操作存在时不强制skipWaiting/刷新；更新需明确操作并保留旧查询记录。Mock在独立测试页/构建，不允许生产query参数、localStorage或隐藏按钮切换成Mock。生产安全provider缺失时只显示尚未开放，禁止把UI状态当作认证。

### VI. T5正式分工与文件所有权

每人继续先完成当前小PR；接口已固定可只读预研，修改包串行。新增20个成员步骤：A-21～24、B-21～28、C-21～24、D-17～20，累计24/28/24/20共96步。涉及项目动作43项、未转交112项；155项未闭环、14/169和加权7.3%保持，C已审6步保留。新增粗估A 2～4、B 6～12、C 2～4、D 5～10小时，仅成员计划，不改项目冻结估时或伪造实耗。

| 人员 / 建议分支 | 步骤与动作 | 验收及主线保留 |
|---|---|---|
| wnfhsj88 / `codex/member-a-nkn-status` | A-21：按III盘点真实引擎可提供字段；A-22：实现只读DTO/未知字段映射；A-23：独立状态展示组件及u64/陈旧/乱序/取消测试；A-24：交实际组件、本地浏览器与主线接线报告 | NKN-07离线部分。等待A-13～16引擎接口获审后修改引擎输出；无法提供的字段不造值。主线负责Core读取鉴权、实际网络及控制开关 |
| wnfhsj / `codex/member-b-ble-configurator` | B-21：实现注入式RecoveryClientV1控制器和点击选择；B-22：pin/未预绑定/撤销/缺provider页面；B-23：有限试连与明确保存/取消；B-24：真实CAS记录、丢回复及刷新只查询；B-25：静态PWA允许清单；B-26：离线/缓存升级与未确认操作保护；B-27：实际模块＋模拟端口/分片向量及1440/390/320/键盘测试；B-28：交四项映射和生产接线缺口 | BLE4-01～04离线部分。等待B-17～20回执接口获审后接本包；使用主线注入端口，不实现密码协议或批准恢复身份。生产认证/IDB绑定主线提供；未提供时真实路径拒绝 |
| yh1030 / `codex/member-c-ap-failure-matrix` | C-21：对照IV列已有覆盖和10类case；C-22：真实station_trial::run错误/取消/restore矩阵；C-23：真实station_commit::run部分保存/audit/撤销/迟到结果矩阵；C-24：实际测试、调用序列、失败最小样本和设备补验交付 | AP-04离线部分。依赖实际公开trait已存在，可在当前历史PR获审后与恢复/制品包串行。Root实际失败/SSID/DHCP/物理回退仍归主线；原AP-03完整实机前置保持 |
| RedamancyGTW / `codex/member-d-ble-gatt-adapter` | D-17：按II建立固定对象/UUID、帧/预算类型与端口；D-18：实际注册/撤销/Release/epoch状态机；D-19：借助Mock BlueZ与测试SecurePort验证广播部分失败、第二peer、乱序/洪泛/断连/重启/晚到注册；D-20：真实库/帧向量/资源上限测试和主线接线交付 | BLE3-01/04离线部分。先交当前D包；只实现可复用库与注入后端，缺安全端口默认拒绝。真实BlueZ绑定、Core IPC、密码协议、服务UID/Root安装与BLE3-02/03保持主线 |

**A可写**：新增`crates/edge/src/web/nkn-status.js`、`tools/test-web-nkn-status.mjs`、`tools/fixtures/nkn-status/`、`docs/nkn/status-integration.md`及本人报告；原本人NKN引擎范围可加只读状态适配，不改Core/页面装配。此处为原“不得改Edge”的具体例外，只开放该独立文件。

**B可写**：新增`configurator/src/`的UI/注入适配接口、`configurator/assets/`、`configurator/index.html`、`manifest.webmanifest`、`sw.js`、`configurator/tests/`、`tools/test-ble-configurator.mjs`、`docs/ble/configurator-integration.md`及本人报告。保留`configurator/src/production-backend.js`、密码/恢复身份绑定实现、Core/Root/wire、现有日常网页、Actions及Pages打包器给主线；不可另建覆盖根站点的发布流。PWA只做新目录资源，不改进度看板。

**C可写**：新增`crates/setup-ap/src/station_trial/tests/t5_matrix.rs`、`crates/setup-ap/src/station_commit/tests/t5_matrix.rs`和`docs/testing/ap-failure-matrix.md`及本人报告；两个既有tests.rs只追加`mod t5_matrix;`测试接入。若Rust模块路径需要显式path属性，仍只加cfg(test)接入，不移动旧测试。生产trait/执行器/NM/期限/Core及其他成员代码只读，实际缺陷交最小提案由主线修正。

**D可写**：新增`crates/ble/`的有限GATT适配库（非默认启动守护进程）、其测试、`docs/ble/gatt-adapter-integration.md`和本人报告；顶层Cargo.toml/Cargo.lock仅允许新增该成员包所需的最小条目，复用已锁定serde/tokio等依赖，不升级或删包。与A的workspace修改按小PR合入顺序正常解决，禁止覆盖锁文件。不得改Auth/policy/wire/Core/蓝牙恢复密钥存储、systemd/设备访问权限、启动/签名/Pages或主线接口文件。新生产backend必须由主线明确安装并注入，Mock不能作为运行时默认。

主线独占本节接口清单与向量，成员不得自行改版本、预算、错误枚举、UUID或信任边界；有冲突先交具体输入/错误/兼容提案。T5在这些明确路径上追加原任务书例外，其他T3/T4限制继续有效；不领取他人包、不操作设备、不发真实网络业务、不改全局进度或将离线结果标实机通过。

### VII. 接口依据与冻结边界

当前源码事实：`crates/wire/src/auth.rs::wifi_credentials_valid`、`crates/edge/src/web/ap-wifi.js::credentialsInput`、直接Core的`network_trial/provision.rs`及Root AP的`setup-ap/src/station_trial.rs/station_commit.rs`已逐项对照；直接Wi-Fi的64hex trial ID与AP的UUID明确分开。GATT及恢复客户端是新接口，不声称现有Core已能接收。

BlueZ对象树/注册、特征值方法及广播生命周期依据上游[GattManager](https://github.com/bluez/bluez/blob/master/doc/org.bluez.GattManager.rst)、[GattCharacteristic](https://github.com/bluez/bluez/blob/master/doc/org.bluez.GattCharacteristic.rst)、[LEAdvertisingManager](https://github.com/bluez/bluez/blob/master/doc/org.bluez.LEAdvertisingManager.rst)；浏览器需可信上下文和用户点击授权，依据[MDN Web Bluetooth](https://developer.mozilla.org/en-US/docs/Web/API/Web_Bluetooth_API)。UUID、12字节头、上限和组件方法是本项目本次冻结的工程选择，不是BlueZ标准协议，也不是安全握手规范。

只有接口/任务已固定；现有BLE1加密、BLE2 Core授权、BLE3生产系统服务、BLE5真实Pages与BLE6硬件仍保持待实现/待验收。不得因本次分工将这8个完整动作或原项目百分比标完成。

<a id="t6-maximum-offline-delegation"></a>

## T6 · 最大化转交：成员负责实现，主线集中审核和实际交付

用户进一步要求“尽量把能分出去的任务都分出去，咱们只做实在分不出去的”。本节是最新所有权规则，**在明确列出的文件和阶段上追加/替代T3～T5的代码归属限制；安全不变量、失败关闭、真实环境授权和原验收标准均不放宽。** 密码/授权/Root代码的重要性不是永远留给主线编写的理由：成员可以交离线实现和故障测试，主线负责接口/方案审批、精确HEAD审查及真实签名部署。

本次T5的8项加T6的47项，合计再转交55项的离线部分；全项目155项未闭环动作中，**90项有明确成员离线负责人，65项保留主线专属执行**。90项仍需主线审核、集成及其中实际环境验收，不是任务已完成或主线工作量只剩65次操作。41个需要真人的验收点保持，不因离线代码转交而取消。逐动作所有权和主线保留原因见[完整映射](offline-ownership.json)及文末表；原169分母、14项完成和估时权重不变。

### 1. 四人最终责任与执行批次

| 人员 | 当前累计步骤 / 离线已审 | T6追加实现责任 | 全部分配粗估 |
|---|---|---|---|
| fungi1220 | A-01～30 / 0 | NKN进程/路由边界与成员证明、BLE安全会话库、邮件通道、安全矩阵/静态权限审查 | 39～88工程小时 |
| wnfhsj | B-01～32 / 0 | 全部网页组件装配和窄屏/无障碍、统计展示、PWA发布材料、说明/性能报告工具 | 28～63工程小时 |
| yh1030 | C-01～30 / 6 | 更新/干净系统/健康判定离线实现、内核构建材料、NAS控制/声音客户端、恢复/发布证据与测试中继模板 | 35～80工程小时 |
| RedamancyGTW | D-01～26 / 0 | Core BLE恢复授权/IPC/Wi-Fi接线、有限服务模板、远程API与TURN媒体、跨模块失败测试 | 33～75工程小时 |

T5先增加20步至96步，T6再增加22步至**118步**；C的6步审核原样保留，工时仅粗略计划，不是剩余工时/实际投入/交付承诺，也不重复累加到项目台账。新增T6粗估A 20～45、B 10～22、C 18～40、D 16～36小时；遇到新协议/依赖/格式提案时再按证据校准。

**执行规则**：每人只推进一个修改包；先交手上当前PR。新包第一份小PR只固定接口/来源/失败样例，主线审核后再实现其余代码；等待时可只读预研下一包。涉及Auth scope/schema、密码套件、签名材料写入表、Root账户/路由或媒体线路格式的变更必须独立列出迁移/兼容提案，不允许混在展示或文档PR中。此规则是工程分阶段，不要求再次征求用户分工许可，也不允许成员直接合入main。

### 2. 冻结的新增端口和主线审批点

T5已有方法、DTO、错误、UUID和预算继续适用；本地记录按已核对的device_pin独立命名空间存储，CAS不能跨设备共用一个槽。下面是T6**源码内部接口**，不是允许任意客户端调用的新HTTP/IPC。对具体端口的序列化、错误枚举与持久格式，负责人按成员接口PR冻结后才准许接生产入口；缺少该实现时一律unavailable/拒绝，不用Mock成功兜底。

| 端口 / 实现人 | 输入与输出 / 必须保留的语义 |
|---|---|
| `NknRootPortV1` / A | `status()`、`start(fixedPolicyHandle,leaseProof)`、`stop(leaseProof)`；只处理Root预登记设备服务策略句柄，不能传任意shell/CIDR/网卡/路由/出口。主线Root检查SO_PEERCRED及有效租约；引擎、转发、统计三个开关独立，默认不改默认路由、不转发LAN、不启普通APN |
| `SecureSessionPortV1` / A | `startHandshake(context)`、`ingestHandshake(boundedBytes)`、`seal(direction,sequence,payload)`、`open(direction,sequence,ciphertext)`、`close()`；context绑定设备pin、独立恢复公钥/代次和会话上下文。只用经审核的标准协议/库和固定向量，不能自创密码原语。加密套件/握手转录/nonce域在接口PR审批前仍未冻结；T5分片头不是nonce。Rust与浏览器必须有真实互操作和失败向量，密码状态不进入日志/公开DTO |
| `RecoveryAuthorizerV1` / D | `grant(recentOwner,publicKey,constraints)`、`revoke(recentOwner,key,generation)`、`authorize(peerProof,sessionProof,requestContext)`；返回Core内部不可由payload构造的能力上下文。不能以bool/channel字符串/蓝牙bonding代替；同Core、同唯一Owner，begin/confirm/save复核代次/撤销。现有policy只有10个scope，新增能力/位/存储迁移先审，禁止直接重编号、改ALL_SCOPES或给Client/Nas伪装Owner |
| `RemoteCallMediaPortV1` / D、C消费 | 复用现有wire的`PrepareCall/SubmitCall/CallOperation/CancelPreparedCall/RenewCall/IssueMedia/CheckMediaLease`和domain的CallAction/CallLease；不新建呼叫库或幂等体系。远程/NKN/Origin/IP只是传输，不授予业务权限；每次调用核验正常Core身份、线路、操作ID和租约。TURN凭据短期且绑定实际会话，不能以通用代理代替受限媒体路径 |
| `NasControlClientV1` / C | `prepare(action,lineEpoch,idempotencyKey,callId,payload)`、`submit(operationId,lease)`、`query(operationId)`、`renew(lease)`；由注入的正常已授权控制客户端会话调用上一端口。旧NAS只读LegacySession继续只读，不能把旧NAS签名/身份自动升级为控制权限，不能复制Owner私钥。结果不明只查原ID，不自动重拨/挂断/改变唯一控制租约 |
| `NasAudioPortV1` / C、D提供授权 | `open(currentPermit)`、`pushFrame()`、`pullFrame()`、`close()`；复用实际media Authority/Context/Permit/Ticket以及protocol解析。当前PCM是16kHz、单样本i16、320样本/20ms、640字节/帧；年龄60ms、队列4帧沿用源码，不能用缓存/增租约掩盖断音。旧NAS不同格式必须显式转换/拒绝并有向量，不能静默按另一采样率播放；生产设备、音频路由和听音仍主线 |
| `EmailSenderV1` / A | 复用forwarder的Connector/Fence/Claim阶段边界与`DeliveryOutcome::{NotSubmitted,Confirmed,Unknown}`，新增SMTP/TLS有限实现和独立私有配置。服务接受确认后的邮件结果只能表示服务器接受，不等于手机收到；DATA可能已发后掉线/超时保留Unknown，不重试。出口固定、证书检查、凭据/正文预算及OFF无进程语义不变；新通道的配置/数据格式/依赖先交兼容提案，不触发旧outbox自动迁移 |
| `ReleaseMaterialPlanV1` / C | `inspect(readOnlyInputs)`、`validate(exactManifest,evidence)`、`plan(fixedTargets)`、`assemble(privateOutput)`；输出精确来源/架构/摘要/权限/Root契约/数据版本和缺口。写入表由主线签名材料约束，禁止任意路径/服务/命令。默认只读计划，实际组装仅指定隔离输出；候选健康评估与其精确source/version/Root/data证据绑定，无证据不能confirm。生产签名密钥和实际Root安装/choose/confirm始终由主线执行 |
| `RelayTemplateV1` / C | `validate(explicitTemplate)`、`renderPrivate(outputDir)`、`redactedSummary()`；默认只列计划，模板里只用合成主机/占位凭据，正式证书/账号不进Git。端口/预算/凭据生命周期/停止与清理明确；不登录服务器、开端口、发公网请求或自动部署。主线提供并验证实际服务器/DNS/证书后才可实测 |
| `EvidenceIndexV1` / B、C | 消费固定SHA、具体测试命令/环境、样本与通过/失败/未运行/实机待验状态；生成支持表/差距表/手册和性能报告。单个CI绿勾、标题或人填`passed=true`不能推导整机通过；缺来源、错SHA、缺关键证据必须保留缺口。主线签认支持矩阵/预算/正式发行 |

所有新库使用Rust（现有Go NKN/NAS来源部分保留Go及许可）；网页原生JS/CSS，构建/审计工具Python/既有Node。新增第三方依赖先锁版本/来源/许可并提案，不批量更新锁文件。核心密码协议和数据迁移的“接口审批未完成”明确阻止生产接线，不能把设计状态写成运行支持。

### 3. wnfhsj88（接手原成员A） · A-25～30 · 引擎、安全会话与邮件

| 步骤 | 对应动作与具体工作 | 接手/验收 |
|---|---|---|
| A-25 | NKN-03/04：基于已有最小引擎，完成固定Root策略端口、低权监督、开关/资源所有权及退出/重连预算库；注入Mock Root/引擎，覆盖部分启动、停止超时、旧epoch迟到、引用释放 | 真正运行生产监督代码，不用mock状态机自证；真实路由/Root激活和实际cgroup由主线核验 |
| A-26 | NKN-05/06、REMOTE-01：明确发现/信令/媒体分层，复用成员证明，有限设备服务路由与撤销/重放/默认拒绝；先交字段/签名域/状态与允许服务的接口PR | 不产生第二Owner或任意LAN出口。Core权限桥由D实现；A交真实协议/路由策略库及模拟端口测试，主线批准后接线 |
| A-27 | BLE1-01～04：T5帧预算保持，提交标准安全会话/依赖/密钥域/设备pin及transcript提案；获审后实现Rust与浏览器加解密/互操作、错误pin/撤销/重放/旧会话向量 | 明确安全套件尚未获审时不能写成已冻结；不用假签名、固定钥匙或自造加密算法。生产绑定由D在真实Core grant下使用，不是前端自报认证 |
| A-28 | SEC-02/03/07：扩展既有权限清单至Auth/realm/SIM/重放/撤销失败矩阵及Root单元静态检查；复核实际UID/能力/地址族/路径/polkit/credential约束，给出逐SHA缺陷和最小复现 | 可用真实Core开发夹具/临时IPC测试拒绝路径；主线负责受限Root实测及最终安全结论。生产缺陷按文件负责人修复，不跨包抢改 |
| A-29 | FWD-09：按EmailSenderV1实现SMTP/TLS、有限配置及真实本机TLS邮件服务夹具，覆盖拒绝、认证失败、DATA前/后断线、超时和关闭 | 不发真实邮件、不放松Fence/固定出口/证书检查；配置/queue版本/新依赖的兼容PR先获审。发布支持仍等获准邮箱实测 |
| A-30 | NKN-10/11：提交可复现引擎/会话/Root策略材料、来源许可、有限开关/远程流程文档及故障回选接口说明，完成本包真实回归和交接 | C组装候选，主线持钥签名/安装/外网验收；不能另设无签名更新器或用Go代码冒充Rust |

可写：原本人路径及新`crates/nkn-agent/`、`crates/recovery-session/`、`configurator/src/crypto/`、`tools/test-web-recovery-session.mjs`、`tools/security-matrix/`、`firmware/system/nkn/`、`crates/forwarder/src/email/`及相应独立测试/文档。forwarder生产配置/发送器枚举只在兼容接口PR获审后最小接入；outbox/schema/Core派发保持其负责人审批。Core安全测试优先经现有dev-fixtures外部驱动，确需私有hook先提最小提案。A只写自己的密码库和前端crypto子目录，不写D的binding/grant/production-backend实现。

### 4. wnfhsj · B-29～32 · 网页装配、PWA材料和报告

| 步骤 | 对应动作与具体工作 | 接手/验收 |
|---|---|---|
| B-29 | WEB-03/07、STAT-09：接独立组件到现有日常网页，完成手机/长中文/回收站/键盘与无障碍；增加窗口/更新时间/未知/关闭语义的统计展示 | 状态来自A的实际DTO/注入接口，按钮调用已授权现有业务控制器；不另造登录、Owner、提交队列或用渲染触发请求。真实Android/第二端及关闭无PID由主线验收 |
| B-30 | AP-12、REMOTE-07：为配网/远程整理可复制脱敏诊断、地址/pin/证书交付指引；接入明确的状态/禁止原因及只查询操作核对 | 不把HTTPS浏览器提示等同业务离线，不返回任意跳转/控制命令；手册只引用已验证支持，生产事实由主线提供 |
| B-31 | BLE5-01/03：实现/configurator/相对构建、严格静态产物清单、根看板共存及离线/更新说明；提交统一Pages流程的最小改动和合成产物泄露反例 | 不新建覆盖根站点的第二发布流；不上传整个仓库/私有目录、Mock或密钥。主线审核合并后执行真实部署与HTTPS字节复核，BLE5-02仍主线 |
| B-32 | STAB-06：将本人性能工具输出转换为有来源/样本/阈值提案的比较和发布报告；覆盖缺样本、异常、旧SHA、单位错误和短样本不外推 | 真实棒子样本/旧新等功能测量由主线采集；未获批准预算和缺测量不填达标。提交实际浏览器/Node回归、接线差异及未验收项 |

新增允许：`crates/edge/src/web/app.js`、`index.html`、`style.css`中最小组件装配/布局，以及独立`usage-stats.js`、诊断展示组件；本人既有JS测试/夹具、`tools/progress/`以外的`tools/site-build/`、`.github/workflows/progress-pages.yml`仅静态allowlist/构建接线、`docs/web/`/`docs/performance/`/`docs/ble/`中本人对应文档。禁止改session.js的认证、业务模块的提交/幂等/租约、Rust鉴权和设备API语义；需要改变业务行为时给D/主线最小复现。`configurator/src/crypto/`归A，`src/security/`及`production-backend.js`归D，B不越界。公开进度JSON/名册/审核/贡献计算仍主线维护，不允许发布私有证据。

### 5. yh1030 · C-25～30 · 系统材料、NAS与发行证据

| 步骤 | 对应动作与具体工作 | 接手/验收 |
|---|---|---|
| C-25 | UPD-02/03：消费主线已收口UPD-01契约，做固定Root材料计划/验证、干净rootfs组合与可复现产物检查；合成密钥/隔离输出覆盖错误版本、任意路径、错hash/权限和中断 | **UPD-01当前U4未提交区域禁止抢改**，等主线提交接口后才接该实现。不能用设备当前Owner/短信/Wi-Fi口令制镜像，不能生产签名/安装/choose |
| C-26 | UPD-05：在真实更新器基础上实现精确候选健康评估库/证据验证与接口接线提案，覆盖进程存活但Core/Root/data不匹配、旧来源、伪造/过期回执 | 不把输入JSON自报healthy当确认授权；实际confirm调用和Root可信证据来源由主线接纳。保留fallback与数据兼容，不能靠清floor/预算通过 |
| C-27 | UPD-08：基于本人UPD-07清单完善匹配的内核/DTB/模块/外部initramfs离线构建材料及重建/静态校验工具 | 无设备只交构建/格式/来源证据；不执行真实启动/分区写入。实际boot和恢复路径仍主线验证 |
| C-28 | NAS-03/04：用独立正常控制客户端会话实现NasControlClientV1和NasAudioPortV1，保留旧NAS只读桥；核对已知旧源码版本，模拟真实Core服务验证操作/租约与音频转换 | LegacyRead/Nas身份不得自动获得控制权限；不复制Owner私钥、不直接拨号或接PCM硬件。无法获得固定旧页面源码时明示缺口，不伪造兼容；真实NAS/两端听音由主线验收 |
| C-29 | FWD-22、UPD-10、REC-05、REL-01/04、BLE0-01/02/04、BLE6-04：给出固定来源/支持事实/缺口的证据索引与校验器、公开能力矩阵、只读硬件计划及安装/回退/使用文档 | 复用已分配REL-05手册，不重复造文档。缺真实HCI/RF/恢复/OTA证据保持未知或不支持；校验器拒绝错SHA/漏项，不能代替负责人最终签认REL-06 |
| C-30 | REMOTE-02：提供测试中继/TURN/DNS/证书的有限私有模板、预算/停机/清理计划与离线校验；合并本包失败矩阵和来源许可 | 不连接实际服务器，不在仓库放账号/证书私钥或自动开端口。真实资源由主线配置与验收，实际部署不算成员已做 |

新增允许：`tools/release-materials/`、`tools/relay-template/`、`tools/release-evidence/`、`tools/rootfs/`中本人匹配构建子目录、`docs/release/`/`docs/remote/`；`crates/update/`中独立root材料/health评估模块及测试（UPD-01收口后，以接口PR列出的模块为准），新`tools/nas-adapter/control/`/`audio/`模块及测试和最小客户端接入。旧NAS只读实现的权限语义不动。不可改主线U4现场、更新器签名/nonce/floor/Store生产逻辑、真实刷写脚本或设备秘密；需要接入点必须小PR明确列出。A/D写Root材料，C只消费/校验/组装，互不改对方源文件。

### 6. RedamancyGTW · D-21～26 · Core恢复、远程权限与媒体

| 步骤 | 对应动作与具体工作 | 接手/验收 |
|---|---|---|
| D-21 | BLE2-01：按RecoveryAuthorizerV1提独立恢复能力/批准撤销/存储迁移提案，获审后在现有Auth/Core唯一权威实现；浏览器绑定非导出恢复密钥 | 不使用匿名首次认领窗口、不制造Owner，不改旧scope bit含义。新版本/损坏/旧realm/grant撤销/重启迁移和无权限全部有真实模块测试 |
| D-22 | BLE2-02、BLE3-02：完成固定Unix IPC/真实SO_PEERCRED来源与会话绑定，接A安全会话和本人Gatt适配库，构造Core内部不可由payload伪造的可信能力 | 测试真实临时socket/不同UID可执行部分，权限不足的Root用例标未运行。禁止前端channel=ble、固定假uid或Mock安全port进入生产路径 |
| D-23 | BLE2-03/04：把合法恢复上下文接现有Wi-Fi开始/查询/确认/取消；保留USB原门禁、唯一网络锁、电话协调、WWAN/IMS/默认APN边界 | 无USB未认证拒绝、合法恢复通过、保存前后撤权/断连/重放/并发网页路径覆盖；Unknown保留，不新建共享业务库或第二网络事务 |
| D-24 | BLE3-03：交付有限低权服务/账户/目录/设备/地址族/预算模板和关闭清理；与Root安装器/C材料组装协调 | 账户角色/实际UID和许可表先审，不自行占用既有UID。无BLE时Core仍运行，关闭清理广播/会话，无常驻任意Root；真实NSS/cgroup/BlueZ权限由主线安装复核 |
| D-25 | REMOTE-03/05：实现现有身份/媒体预授权在远程通道的绑定、撤销及受限TURN/ICE/DTLS/Opus适配，保留票据/peer/call/line/租约关系 | 本地合成对端/私有测试TLS环境做强制中继与失败测试，不连接真实外网服务器。NAS/C仅消费已审核的正常接口，不允许NKN隧道绕过权限 |
| D-26 | 扩展本人SEC-06输入矩阵至实际新增Core/媒体/BLE解析及跨模块故障：并发控制、过期/撤销、退出、回调迟到、分片/队列/超时 | 测试真实生产模块，模拟仅外部后端；不拉长产品租约/队列或删拒绝断言。提交最小复现/修正、真实命令及尚未运行的设备/TURN/两端听音清单 |

新增允许：在先审接口/迁移PR之后，修改`crates/auth/`、`crates/policy/`、`crates/wire/`与`crates/core/`中独立BLE授权/可信来源/远程媒体接线；`crates/media/`的对应授权/远程适配与测试；`crates/ble/`、`firmware/system/ble/`、`configurator/src/security/`、`configurator/src/production-backend.js`。正常通话/短信授权、身份、旧协议和USB路径保持，需改变时单独列兼容提案/拒绝路径，不暗改。不要改A密码原语/会话库、C updater/旧NAS客户端、B UI控制器；公共入口同一时间一个PR，其他包先交接口/最小提案。Root安装及生产启用仍主线。

### 7. 主线真正保留的工作

1. 当前UPD-01/U4未提交现场由主线收口并提交供C消费，避免将半成品与新成员修改混在同一工作树；这是当前的暂时例外，不把后续普通更新代码永远留主线。
2. 当前设备/NAS/服务器的私有资料、生产密钥与签名、具体恢复快照和分区目标；真实系统安装/回读、受限Root运行、路由/运营商/外网/音频/蓝牙及长测。成员只拿公开/合成/脱敏输入，不需要共享主线凭据。
3. 接口与迁移/依赖/安全方案审批、精确HEAD代码审查、正常PR合并、统一集成和真实发布；最终支持声明及100%验收签认。没有设备不等于不能写Core/Root适配代码，代码交给成员，实际权限与执行验证仍由主线掌握。
4. 65项保留动作大多是实际环境关卡，不再重复安排成员重做已有离线测试。若实测暴露代码缺陷，按上面的模块负责人退回成员修正，主线保留诊断/复现/复核，不默认把全部修复抢回自己写。
5. 本轮没有给成员发站外消息、修改GitHub权限、访问设备或执行新网络业务。项目范围/验收/权重不变，不通过“已分配”或生成报告增加完成率。

### 8. T6主线专属动作明细

下面按当前台账自动核对；其中一些动作消耗用户、真实卡/对端或生产环境，另一些是主线必须签认的最后关卡。表不取消90项成员实现的主线审查/集成。

| 模块 | 主线专属动作 | 保留原因 |
|---|---|---|
| FWD | FWD-06, FWD-08, FWD-13, FWD-14, FWD-16, FWD-17, FWD-18, FWD-19, FWD-20, FWD-21 | 真实设备/真人/卡/提供商/外网执行，具体前置见台账及逐动作映射 |
| AP | AP-03, AP-07, AP-08, AP-09, AP-10, AP-11 | 真实设备/真人/卡/提供商/外网执行，具体前置见台账及逐动作映射 |
| SMS | SMS-01, SMS-07, SMS-08, SMS-09 | 真实设备/真人/卡/提供商/外网执行，具体前置见台账及逐动作映射 |
| CALL | CALL-01, CALL-02, CALL-03, CALL-04, CALL-05, CALL-06, CALL-07, CALL-09, CALL-10 | 真实设备/真人/卡/提供商/外网执行，具体前置见台账及逐动作映射 |
| NET | NET-04, NET-05, NET-06 | 真实设备/真人/卡/提供商/外网执行，具体前置见台账及逐动作映射 |
| REMOTE | REMOTE-04, REMOTE-06 | 真实设备/真人/卡/提供商/外网执行，具体前置见台账及逐动作映射 |
| NAS | NAS-02, NAS-07, NAS-08 | 真实设备/真人/卡/提供商/外网执行，具体前置见台账及逐动作映射 |
| UPD | UPD-01, UPD-04, UPD-09 | UPD-01现场收口及真实回选/刷写；其余更新代码已交C |
| REC | REC-01, REC-02, REC-03 | 真实私有备份/快照/写入目标及现场演练；通用恢复交C |
| LED | LED-01, LED-02, LED-03, LED-04, LED-05, LED-06 | 真实设备/真人/卡/提供商/外网执行，具体前置见台账及逐动作映射 |
| WEB | WEB-04 | 真实设备/真人/卡/提供商/外网执行，具体前置见台账及逐动作映射 |
| STAB | STAB-02, STAB-03, STAB-04, STAB-05 | 真实设备/真人/卡/提供商/外网执行，具体前置见台账及逐动作映射 |
| REL | REL-02, REL-03, REL-06 | 第二设备安装/恢复及最终负责人签认；证据工具交C |
| NKN | NKN-08, NKN-09 | 真实设备/真人/卡/提供商/外网执行，具体前置见台账及逐动作映射 |
| STAT | STAT-08 | 真实设备/真人/卡/提供商/外网执行，具体前置见台账及逐动作映射 |
| BLE0 | BLE0-03 | 真实设备/真人/卡/提供商/外网执行，具体前置见台账及逐动作映射 |
| BLE5 | BLE5-02 | 实际统一Pages部署与线上核验；构建实现交B |
| BLE6 | BLE6-01, BLE6-02, BLE6-03 | 真实设备/真人/卡/提供商/外网执行，具体前置见台账及逐动作映射 |

最新主线统计截面为`6f228fd3dcb9ce6da54d7910f4b044833e71a91a`。用户本轮已合并PR#8/#9，贡献按该真实主线刷新；#9本轮未做代码验收，不将C-07/08自动计入已审，C已审仍6步。Core/wire/setup-ap/media对照基线至该主线未变，T5源码语义对照保持。T5/T6分工PR合入main后，成员从最新main开本人分支；此前只读预研，避免按缺少接口文件的旧版本接线。

T5/T6本轮验证：18项既有看板单测、Node面板/生成一致性通过；118步/90项离线归属/65项逐项主线原因/41真人前置和全部155项唯一映射通过，原动作/状态/权重/估时/已审记录保持。9组黄金Wi-Fi输入已对照实际生产JS校验器；UUID唯一性、分片向量自洽、严格示例字段和u64精度核对通过；这些不是GATT/加密/NKN实际互操作或硬件通过。git diff --check通过。生产实现、协议套件审批及实机验收仍按对应任务交付。
