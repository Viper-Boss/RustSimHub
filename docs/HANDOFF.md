# RustSimHub 当前接手入口

2026-10-24 P4H后续：源码54bf31f的精确CI40050411713已全部成功，含新增真实AP材料CLI隔离步骤、309项Rust/网页/系统夹具及Native任务；Root导入和私有导出通过。源码与接手索引已保存，仍未安装/访问设备，当前不需用户操作，整体79.5%。

2026-10-24 [P4H配网材料/证书](VALIDATION-SETUP-AP-MATERIAL-P4H.md)源码完成：独立固定AP Edge准备、Root绑定导入/每设备随机材料、完整pending不覆盖发布和可信私有导出（含真实设备公钥），独立服务候选在overlay之外。309项Rust、严格Clippy、生产检查、两程序/库ARM64和真实Root/普通用户两项CLI临时目录联调通过。未部署/未访问棒子，不启动服务或热点，不切网/拨号/发短信/开数据或改信任库；仍缺真实绑定/首次窗口、AP到STA及网页/联合安装。上次确认业务185ec6e/43、整体79.5%、10010仍44，当前无需用户操作。

2026-10-24 P4G后续：执行程序源码38be15d的精确CI40048512048已全部成功（工作区/网页/系统夹具及Native音频两任务），本地301项Rust/严格Clippy/ARM程序和库保持。源码与私有接手索引已保存；没有部署或设备访问，当前无需用户操作，整体仍79.5%。

2026-10-24 [P4G Root配网执行程序](VALIDATION-SETUP-AP-EXECUTOR-P4G.md)源码完成：真实保护文件租约连接固定NM/nft事务，消费后唯一激活、独立过期/停止取消、真实TLS/SAN/精确Origin和设备身份检查，实际AP HTTPS可达后才Ready；Unknown保留，持久Closed可补发而不重放。301项Rust、严格Clippy、生产检查及执行程序/库ARM64通过。未安装/访问棒子，仍缺材料证书安装、首次窗口、AP到STA和网页；当前不需用户操作，上次确认业务185ec6e/43、整体79.5%、10010仍44。P4F精确源码CI40046938582已成功。

2026-10-24 [P4F配网控制面](VALIDATION-SETUP-AP-CONTROL-P4F.md)源码完成：真实Owner/network.manage、Core网络互斥、严格通话空闲读取、Core/Root限长保护文件协议与持久防重；旧Start恢复仅取消，Unknown保留锁，精确Root回退证明后才释放。289项Rust、严格Clippy、生产检查、Core/库ARM64及12组网页通过。默认模式禁用，新显式模式仍需Root执行进程、真实材料/证书、首次窗口和AP到STA/网页；本轮不访问/交付棒子，不开热点、不切网/拨号/发短信/开数据，当前无需用户操作。上次确认业务185ec6e/43、整体79.5%、10010仍44。

2026-10-24 [P4E热点事务层](VALIDATION-SETUP-AP-TRANSACTION-P4E.md)源码完成：新增setup-ap库的每设备随机材料、固定WPA2临时AP、有限NM checkpoint、单次激活/精确回退和Root固定隔离；276项Rust、严格Clippy、生产检查、ARM64库及精确源码CI40042735327通过，NAS私有网络命名空间加载/排他创建/删除规则通过，无宿主防火墙或棒子无线修改，未做报文实测。尚未接Core真实授权/首次认领、证书Origin、AP到STA试用、网页或启动服务，不部署/不开热点。上次确认业务185ec6e/43保持，10010仍44、整体79.5%；用户休息期间继续独立工作，无当前用户操作。

2026-10-24 [P4D网页Wi-Fi选网](VALIDATION-WIFI-CATALOGUE-P4D.md)已交付185ec6e/confirmed43：508份固定Git文件、267项Rust、八ARM、同源Native33、12组网页及CI40022101743通过。Owner缓存选网、同区登录和隐藏SSID手输已上线，选中只填名称；40秒试连接规则保持，查询失败保留trial_id。原信任签名/设备回读、一致性备份、九份严格HTTPS资源/身份和新接口无效凭证拒绝通过；原Owner/67短信/Unknown、Root音频/单元/网络及普通数据关闭保持，同boot无恢复预算消耗，无新电话/SMS、无重启/刷分区/扫描/切网。真实Owner缓存读取、网页和切网未验收，整体79.5%、10010仍44；Windows临时证书确认未受信任。当前不需用户配合，用户休息期间继续独立配网/NAS功能准备，不安排物理/听音操作。最新私有索引wifi-catalogue-checkpoint-20261024-first.json。

2026-10-24 [P4D网页Wi-Fi选网](VALIDATION-WIFI-CATALOGUE-P4D.md)源码完成：Owner最近登录下读取设备缓存，显示SSID/信号/频率/加密，显式选择只填名称；配网移入网络配置区并有同区重新登录，隐藏网络手输和原40秒确认保持。267项Rust、严格Clippy、生产检查及12组网页通过；查询失败保留trial_id，不自动扫描、连接或确认。尚未固定ARM构建/交付/实机验收，现场仍c91d7cd/confirmed40及67短信。无设备访问/新电话/SMS发送，普通数据关闭、关机搁置，整体79.5%、10010仍44；用户可休息，当前不需配合。下一步固定同源构建和正常签名应用升级。

2026-10-24 [P3N签名会话续期](VALIDATION-CALL-SESSION-P3N.md)已交付c91d7cd/confirmed40：262项Rust、八ARM、同源Native33、十一组网页和CI40016691893通过。原信任签名、备份回读、受限进程、九份严格HTTPS资源/身份及新接口无效凭证拒绝通过；原Owner/67短信/Unknown、Root音频886/单元0a/网络及普通数据关闭保持，同boot无恢复预算消耗，无新电话/SMS发送、无重启/刷分区。短期凭证轮换不延长旧期限或敏感管理登录时间，声音/预连接连续性已离线验证，真实长通话尚未验收。整体79.5%、10010仍44。另只读确认NM声明AP能力，未扫描/开热点/切网，准备后续独立配网；用户休息期间不安排物理/听音操作。最新私有索引session-continuation-checkpoint-20261024-first.json。

2026-10-24 [P3N通话会话续期](VALIDATION-CALL-SESSION-P3N.md)源码完成：原会话和浏览器密钥共同证明身份后轮换短期凭证，旧期限与敏感管理登录时间保持；网页只在日常通话中续期，活动声音/预连接保持原连接。262项Rust、严格Clippy和十一组网页通过，含真实Auth/IPC、fake D-Bus媒体绑定和WebCrypto接线。尚未ARM构建/交付/真实长通话验收；现场4350895/confirmed37及原67短信保持，整体79.5%、10010仍44。无新电话/SMS、无设备访问，普通数据关闭、关机搁置，当前无需用户配合。下一步固定同源构建及正常签名应用升级。

2026-10-24 [P3M日常电话](VALIDATION-DAILY-PHONE-P3M.md)已交付4350895/confirmed37：256项Rust、八ARM、同源Native33、十组网页和CI40008196169通过；原信任签名、备份回读、受限进程及六份严格HTTPS资源/身份通过。首页具备拨号、实时来电/号码、接听/拒接、挂断、按键和静音；新日常页面的真实听音/拒接/DTMF尚未验收。原Owner/67短信/Unknown、Root音频886/单元0a/网络及普通数据关闭保持，同boot无恢复预算消耗、无新电话/SMS发送、无重启/刷分区；factory16和原34组保留。整体79.5%、10010仍44，当前不需用户配合，继续日常使用/网络/NAS缺口。最新私有索引daily-phone-checkpoint-20261024-first.json。

2026-10-24 [P3M日常电话](VALIDATION-DAILY-PHONE-P3M.md)源码完成：首页实时来电/号码、拨号、接听/拒接、挂断、按键和静音，沿用持久提交标记与AudioLink，增加确认租约续期和迟到动作隔离。Core补精确响铃拒接的一次临时控制，已接通/其他控制仍要求租约，schema6保持。256项Rust、严格Clippy及十组网页通过，尚未ARM构建/部署/真实电话验收；现场仍5d74f25/confirmed34和原67短信，整体79.5%、10010仍44。用户可继续休息，不安排现场/听音/短信发送，普通数据关闭、关机搁置。下一步固定同源构建与正常签名应用交付。

2026-10-24 [P2G日常短信](VALIDATION-DAILY-SMS-P2G.md)已交付5d74f25/confirmed34：254项Rust、八个ARM、同源Native33项、九组网页及CI40001841916通过；原信任签名、备份回读、受限进程及五份严格HTTPS资源/身份通过。原Owner/67短信/Unknown、Root音频886/单元0a和普通数据关闭保持，同boot无恢复预算消耗、无新电话/SMS发送、无重启/刷分区。首页可读短信和确认一次发送入口已上线，但真实网页发送与用户视觉验收仍未做，不增加里程碑分数，整体79.5%、10010仍44。当前不需用户配合；下一步日常电话入口，真实短信/听音/拔插等用户醒后安排。最新私有索引daily-sms-checkpoint-20261024-first.json。

2026-10-24 按用户“最快交付、功能优先”推进[P2G日常短信](VALIDATION-DAILY-SMS-P2G.md)：首页收件箱/正文/确认一次发送、可查询回执和回收站已实现；有界摘要不改schema6，持久提交标记/Unknown及注销竞争保持。九组网页、两项存储定向测试、完整254项Rust测试及全工作区严格Clippy通过；尚未ARM构建/部署或真实发送。Claude未接任务，短信前端由主线负责。用户可以先睡，当前不需要现场操作；后续真实短信/听音/拔插另行安排。现场仍业务935ea9d/confirmed31、原67短信，普通数据关闭、关机搁置，整体79.5%、10010仍44。

2026-10-24 用户咨询Claude离线协作，已整理[可直接交付的任务包](CLAUDE-OFFLINE-HANDOFF.md)：建议先日常短信、再电话页面，配网交互/NAS契约适配作为后续；明确独立目录/分支、现有API与鉴权/防重边界、八组网页回归和patch交付。尚未收到或验收Claude代码，未操作设备，整体仍79.5%。采用任务A时主线应避开其前端入口，处理设备/后端及最终集成。

2026-10-24 [P6AH音频启动](VALIDATION-AUDIO-READINESS-P6AH.md)第二次普通拔插通过：精确控制设备Requires/After修正后首次启动成功，音频23.86秒七路0→7、ready_ms=0，网页约25.07秒启动；无app-recover日志且新boot恢复标记不存在。NAS USB IPv4仍未激活，保存Wi-Fi/严格HTTPS可用，原Owner/67短信/TLS/Unknown及confirmed31保持，没有新增重复候选或电话。业务935ea9d、Root音频88692b4、启动单元0a823e2；33 Native/155系统（1跳过）及完整CI39989478746通过。前一次失败及恢复后健康分别保留；不重复给冷启动修复加分，整体79.5%、10010仍44，普通数据关闭、关机搁置。当前无待用户操作。

2026-10-24 P6AH：88692b4/Root音频已交付并经普通拔插；首次直接启动仍失败，PCM出现后控制设备许可尚未正确建立，随后既有恢复启动成功。严格first失败保留，second仅恢复后健康：原67短信/Owner/TLS/epoch31保持，普通数据关闭。真实systemd合成设备复现“缺节点时建许可→后来仍EPERM”，有序依赖后ENXIO（无实际硬件）；源码补Requires/After精确控制设备、JobTimeout60，尚待交付/第二次冷启动。恢复标记未重置，整体79.5%，关机搁置。

2026-10-24 [P6AH音频启动等待](VALIDATION-AUDIO-READINESS-P6AH.md)源码已补：只对固定PCM状态NotFound作20秒单调截止等待，占用/无效/其他错误立即拒绝，全部明确closed后才检查profile/初始化；oneshot超时30秒、原权限和一次恢复预算保持。33项Native/严格Clippy及154项Root系统夹具（1跳过）通过，尚未固定ARM构建/部署或新冷启动；业务仍935ea9d/confirmed31。整体79.5%，关机继续搁置。

2026-10-24 [P3L一键来电](VALIDATION-ONE-CLICK-INCOMING-P3L.md)真实验收通过：用户反馈两端正常、接音1.9秒；后台第二通新来电ringing-in→ACTIVE→27秒后terminated，精确目标各一次accept/hangup、双向收发播放均1200多帧、结束后六个PCM关闭。第一通未接来电使首份观测提前结束，未冒充语音证据；后续独立日志/数据库核验通过。原Owner/67条短信/Unknown保持，临时证书已移除，10010累计仍44、普通数据关闭、关机搁置。P3加1点至15.5，整体79.5%；有19帧丢弃/1次可恢复xrun，远程/长期/竞争仍缺。接下来修开机早期音频设备状态出现较晚的有界等待。

2026-10-23 [P3L一键来电](VALIDATION-ONE-CLICK-INCOMING-P3L.md)已部署并确认935ea9d/epoch31：252项Rust、八ARM/Native26、八组网页和CI39980085479通过；原Owner/身份/67短信及Unknown保持。用户现在只需首页一键等待、再用另一手机拨入；双向讲话尚待反馈，当前观测/本轮临时证书待清理见私有checkpoint。开机早期媒体依赖/Edge预检查曾失败，自动整组恢复后可用；recovery.used早于本轮升级，本轮无恢复且未重置标记，第二份独立confirm通过、首份拒绝保留。P6AG是最终自恢复可用，首次无故障启动仍缺；整体78.5%、10010累计44，普通数据关闭，关机搁置。

2026-10-23 [P3L一键来电](VALIDATION-ONE-CLICK-INCOMING-P3L.md)正在实现：用户明确要求简化手动双向流程，首页新增自动等待新来电/单次接听/接音/27秒结束，沿用原30秒租约；Core增加有界实时摘要，不用历史reconciling选呼叫。新增Node失败路径、实际app接线、Core真实fake D-Bus测试及Clippy通过；尚未完整固定构建/签名交付或真实双向验收。手动观测零新来电且已结束，临时证书移除；P2F c126ddf构建/CI39974512953/原信任epoch28—30候选已通过但未上机。现场仍epoch25/67短信/10010累计44，整体78.5%，数据关闭/关机搁置。

2026-10-23 [P2F正式短信管理](VALIDATION-SMS-FORMAL-MANAGEMENT-P2F.md)：用户在epoch25完成指定候选的所有者删除→恢复，后台确认对应事件顺序/Owner归属、原67行及旧记录保留，实际SIM消息和新增1条真正来信读回通过；仍缺其他独立客户端补同步和发送/故障矩阵。最近登录失效时网页反馈不清，源码已补同区重新登录入口、每次修改前核心资格复核、检查失败明确未提交和换选/注销竞争保护；七组网页通过，尚未ARM构建/交付该修正。现场仍e0f843b/confirmed25、数据关闭/关机搁置，整体78.5%、累计10010为44。

2026-10-23 [P6AG正式冷启动](VALIDATION-FORMAL-COLDBOOT-P6AG.md)通过：e0f843b/confirmed25在用户普通拔插后自动启动，保存的Wi‑Fi连接且NAS USB维护IPv4尚未激活时严格HTTPS可用；七项音频路由实际0/7→7/7，私有只读profile和Rust PS偏好生效，原身份/唯一Owner/证书/43条短信及Unknown操作保留。新增23条跨启动候选保持歧义，共66行，未自动合并；P2E两栏/中文正文用户截图验收通过。冷启动后单次10010真正ACTIVE，用户完整听音/0.4秒，后台1246帧收发播放、1253接收、20丢弃/1可恢复xrun，正常挂断后PCM全关闭，累计44；临时证书已移除。P6加0.5，总**78.5%**，剩余21.5点；双向说话、正式短信跨客户端管理、故障回退及整机OTA仍缺，普通数据关闭、关机搁置。

2026-10-23 [P2E候选短信核对](VALIDATION-SMS-CANDIDATE-REVIEW-P2E.md)已实现并通过七组网页回归：收件箱标记、并排当前/旧记录、显式所有者回收站读取、换选/注销/撤销/删除后的迟到回复隔离、DTO限额及二进制显示；比较不写短信、不自动合并、不隐式回收站重试。保持schema6，尚未固定构建/升级/实机页面验收，整体78%。用户目前方便后续普通拔插，须在候选及新备份就绪后明确通知，不提前操作。

2026-10-23 [P3K语音启动修正](VALIDATION-AUDIO-STARTUP-P3K.md)收尾通过：正式新系统来电ringing-in及用户正常回铃确认；10010真实ACTIVE、用户完整听音、观察接通后接音0.4秒，后台约25秒媒体及自动挂断/PCM全关闭核实，累计43。Root网络/私有只读profile为0d56647；独立Root音频平台为998d48f，七路实际0/7→7/7，正式应用仍3216745/confirmed22且原Owner、20条短信与原Unknown操作保留。真实新收3条短信逐条modem received/UTF-8正文/号码/时间对照通过；另20条跨启动duplicate_candidate保持，未按内容合并。新代码本机Native26/严格Clippy、系统154（1跳过）、八ARM、真实ARM Native26与完整CI39947802973通过；两个已退出构建容器已清理、缓存/日志/产物保留。两轮临时证书均移除。最终私有索引carrier-voice-acceptance-20261023-third.json；前两份汇总因新收信数量假设和CLI C语言环境中文转码/字段假设拒绝均保留。尚缺新增音频初始化再次冷启动、真实双向说话/来电接听和长期验收；有1次可恢复xrun/24帧丢弃。恢复回归暂不新增固定里程碑点，整体78%，普通数据关闭、关机继续搁置。

2026-10-23 [P3K冷启动语音恢复](VALIDATION-AUDIO-STARTUP-P3K.md)：0d56647完整检查/ARM/Native与Root维护交付完成，备份及用户普通拔插后，私有只读profile与Rust PS偏好实际生效，正式3216745/confirmed22及Owner保持。用户确认来电正常，后台ringing-in→terminated；随后一笔10010真正ACTIVE，累计42，但音频409、PCM未开。只读确认七项路由0/7，已补Rust平台初始化及独立无PCM许可单元，尚在验证/构建，未安装该音频修正。发现原20短信保留外新增20条带duplicate_candidate的重观测，问题开放。NAS既有不自动连接的USB配置已仅激活恢复，默认路由不变，普通数据关闭，关机继续搁置。整体暂78%。

2026-10-23 语音回归补充：旧AirSimHub固定ea019485和旧系统快照确认两个遗漏：联通运行时PS优先服务，以及rmtfs私有profile启动入口。当前用备份的旧Go有限工具临时将2→3并独立读回，但用户拨入仍不通、没有新Call对象；累计10010仍41。现有私有profile四文件摘要/权限通过，现场rmtfs仍是原分区只读模式。已在Root网络组件内补Rust代理协议及两个启动脚本/可选单元，12项网络Rust测试、Clippy和12项隔离脚本测试通过（首次标记umask问题已修正）；尚未ARM构建/交付/启用/冷启动。待完整检查和备份就绪后再请用户普通拔插，不在运行中重启基带。临时旧工具不是最终Rust实现，需移除。整体78%，语音未恢复验收。

2026-10-23 [P6AF正式应用升级](VALIDATION-INITIALIZED-APP-UPGRADE-P6AF.md)完成：固定3216745的245项Rust/八ARM/同源Native21项及CI通过，原信任epoch22已通过正常暂存、整组选中、数据健康核验及精确摘要确认；保留原factory16、唯一Owner、20条短信、原Unknown操作，未重初始化/刷写/重启系统。用户真实重新登录成功。本轮仅一笔10010，累计41，始终dialing、未ACTIVE、PCM未打开，用户无声且另报来电不通；不能计语音通过。终止对象重新观测造成两条额外历史为已知问题。普通数据关闭、关机继续搁置、临时证书已移除。P6增加0.5点，总78%。后续未接通提示仅有源码/离线回归，尚未交付；先查语音网络，不重拨Unknown。下方epoch16与未部署P3J为历史。

2026-10-23 [P6AE正式首次设置](VALIDATION-FORMAL-FIRST-OWNER-P6AE.md)已完成：官方初始化、同一设备身份/严格HTTPS、公有证书私有交付及用户真实Owner认领/登录通过，唯一active Owner核实，Core/Edge/Media正式运行，窗口已消费关闭。P1补0.5点，总77.5%。两笔10010均已结束，第一笔只dialing、第二笔QMI90/IncompatibleState，PCM未打开，不能计通话通过，累计40。已补[P3J](VALIDATION-UNCERTAIN-CALL-CLEANUP-P3J.md)匹配租约异常清理和手动归档，七组网页/Edge26项/Clippy通过但未部署。现场仍9528712/epoch16，已初始化，禁止重跑空状态交付/初始化helper或删setup.used；原卡普通数据关闭、关机继续搁置。67ba519离线完整ARM/Native/原信任epoch19-21及CI已过，不包含后加P3J。临时证书已按授权移除；下一步冻结P3J、正常签名升级路径交付，再定位语音网络问题。下方未初始化记录均为历史。

2026-10-23 [P7G](VALIDATION-WEB-VIEW-LIFECYCLE-P7G.md)：补齐通话/历史列表及详情的迟到回复隔离、分页在途检查和注销清理；被替代的刷新拒绝，避免补同步游标提前提交。实际app回归、旧版预期失败对照、七组网页、Linux Edge26项/Clippy通过。未访问/部署设备，现场仍9528712/epoch16，首次Owner与新证书授权问题待用户回复；关机继续搁置、原卡数据关闭，整体77%。

2026-10-23 [P6AD](VALIDATION-SCHEMA6-DEVICE-P6AD.md)：原发布信任下schema6应用已实际交付，factory现为9528712/epoch16，旧epoch13及文件备份保留；Root网络a239f7f/灯控0c817c1未变。设备包验证、自检和隔离合成音频通过。按用户选择只读恢复此前Wi-Fi并实际连接，自动连接已保存，USB配置保持、普通gsm断开。TLS/媒体配置已准备，正式身份/业务库/installation.complete与Owner窗口仍未创建，业务服务inactive。等用户就绪后进行首次认领；不重跑已消费helper。无新电话/SMS、无重启/刷写，关机继续搁置；整体77%。下方旧“未交付/epoch13”是历史检查点。

现场补充只读复核：`.private/firstboot-prerequisites-20261023-first.json`通过；固定身份SSH/同boot维护，MM/NM/rmtfs/网络守卫/灯控active，正式Core/Edge/Media inactive。installation.complete、正式identity.seed、业务store.sqlite3和升级账本state.json均不存在，PCM关闭、普通gsm断开且无gsm配置。当前factory epoch13声明5..5。未安装或初始化。后续应按空正式业务的v6首次安装准备，不能把隔离v5迁移试验说成已迁移现场；仍需沿用正确发布信任和交付完整基线、Owner/证书流程。整体77%，关机继续搁置。

2026-10-23 [P6AC](VALIDATION-ARM-DATA-MIGRATION-P6AC.md)：NAS真实ARM生产Core已验证v5→v6、Owner/Client签名IPC、删除保持、兼容v6 Core读取新增合成记录/恢复及cursor100→103。直接存储入口的旧v5拒绝和DDL迁移冲突后数据库逐字节保持通过；first错误字符串假设、second镜像输入失败均保留false，third汇总通过，并独立核对三版本生产元数据/ELF/hash及最终库完整性。合成新记录在Core停止时注入，不是modem收信，未证明完整签名组与真实新短信切换闭环。无棒子访问/部署，整体77%，原卡数据关闭、关机仍搁置。下一步核现场首启与维护条件。

2026-10-23：用户要求的[当前剩余未完成清单](REMAINING-2026-10-23.md)已生成，按实现/验收/参与条件列出23点缺口，同时修正旧任务包合计23.5为23。继续完成[P6AB实际ARM升级器验证](VALIDATION-ARM-UPDATE-RUNTIME-P6AB.md)：NAS隔离环境真实签名、boot-select发布/回选、floor保持、确认和账本恢复、篡改拒绝通过。first/second失败保留，third通过；未访问/部署棒子，尚未验证业务数据迁移与候选新短信保留，整体77%，关机仍搁置。

P6AA收尾：精确提交`9528712`的完整 [CI 39901072969](https://github.com/Viper-Boss/RustSimHub/actions/runs/39901072969) success，固定源码244项Linux及八个生产ARM64程序通过。同一归档的Native21项及release构建通过，私有开发签名epoch1/2/3完整应用组已生成；OpenSSL验证每组签名/所有文件、v5拒绝和实际篡改副本拒绝通过。Native使用既有离线ARM镜像Rust1.99，主要程序为本机1.98，区别已记录。制品只是候选，未部署、未运行实际ARM升级器选组或保留新短信的维护迁移。整体77%，关机继续搁置。

2026-10-23 P6AA：修正schema6维护包准入。新包改为6..6，旧updater不能接受；暂存候选前必须核验当前confirmed组，避免单换updater后接受无兼容回退基线的候选。拒绝时不写候选目录、不推进floor；损坏基线拒绝、6组内失败回选与反回退通过。244项Linux、生产检查/Clippy及系统工具141项（1跳过）通过。见 [P6AA](VALIDATION-SCHEMA-ADMISSION-P6AA.md)，索引`.private/schema-admission-20261023-first.json`。未访问/部署设备，尚未验证Native同源签名组和保留新短信的实机维护迁移；关机继续搁置、原卡数据关闭，整体77%。

收尾：功能提交`8785d3c`已同步，完整 [CI 39900140405](https://github.com/Viper-Boss/RustSimHub/actions/runs/39900140405) success；固定提交241项Linux回归及八个生产ARM64程序构建通过，schema6元数据与制品摘要已核对。系统工具141项（1项跳过），七组网页通过。首次归档被严格拒绝已保留，第二份仅普通文件归档通过逐文件Git核对。新增短信本地删除/恢复计P2离线实现0.5点，整体 **77%**；未部署、未代替正式短信/多客户端实机验收。详见P2D。

2026-10-23：按用户要求继续搁置关机问题，完成短信本地删除/回收站/恢复的代码与离线验证。Owner最近5分钟认证、存储generation复核、原子删除事件、列表/详情/未读灯过滤及网页失败/迟到回复隔离已实现；普通Client/NAS不能修改。完整Linux回归241项、七组网页、生产检查及Clippy通过。schema5→6是维护迁移，旧v5应用不能直接回退，尚未安装现场；不宣称新系统短信/完整NAS删除同步实机闭环完成。详见 [P2D](VALIDATION-SMS-TRASH-P2D.md)，私有索引`.private/sms-management-20261023-first.json`。普通数据关闭、无新电话/SMS，当前不需要用户操作；整体暂保持76.5%。

## 最新：收到pon_dump，引用泄漏修正与真实离线构建已核验，未上机

发布复核：实现 `91cb708` 已同步GitHub，完整 [CI 39835048209](https://github.com/Viper-Boss/RustSimHub/actions/runs/39835048209) success。源码hash与实际编译输入一致，CI仅验证模拟依赖控制流及既有工作区；真实模块编译由私有second报告单独证明。Git常规传输及API发布失败保留，最后仅本次命令使用HTTP/1.1推送成功，未改全局配置。DWARF普通/压缩节独立检查均无，仍未上机。

2026-10-23，用户提供完整片段后发现读reg失败分支先取得pdev再直接返回的泄漏，以及所有读取失败仍返回0。审查稿前移失败处理/保留首个错误，省去依据不足的0x0e；GPL来源、实验限制及并发解绑未保证明确保留。实际init的10条模拟依赖分支通过，原片段泄漏/错误成功已复现；两份真实ARM64外部模块编译、vermagic/强依赖、原配置/导出独立核验通过。首次仅最终Git元数据读取被WSL所有权检查拒绝，false保留，second独立复核通过，不改全局Git配置、不重复构建。见 [P6Z](VALIDATION-PON-DUMP-REVIEW-P6Z.md)，唯一成功索引 `.private/pon-dump-review-build-20261023-second.json`。现有匹配vmlinux没有DWARF，未得到faddr2line源码行。没有连接/加载/自动加载/关机/刷写或寄存器实测，电源实机调试仍搁置，软件主线P2C已验证未部署，**整体76.5%**。

## 最新：已核对用户补充分析，电源实机调试仍搁置

2026-10-23，新增 [补充分析核对](POWEROFF-EXTERNAL-REVIEW-2026-10-23.md)。本机固定源码核对PMIC revision分支/写失败返回及SMD全部5处排队，已有参考配置的init-on-free默认未开；这不是本轮运行时检查。原因寄存器需对应实际返回boot，不能用物理恢复boot读值回填；UAF、锁卡死和迟到排队仍未证明。未收到所述pon_dump源码，未构建/加载/关机/改引导。私有报告 `.private/poweroff-external-review-20261023-first.json`；初次默认WSL用户无法读取Root缓存，随后Root只读核对成功，未改原件。软件主线仍P2C已验证未部署，整体76.5%，不因参考审查加点。

## 最新：关机问题搁置，网页补同步修正已完成本地验证

发布复核：实现提交 `cede3d7` 已同步GitHub，完整 [CI 39829439401](https://github.com/Viper-Boss/RustSimHub/actions/runs/39829439401) success，含新增补同步和既有五组网页、Rust/生产配置/Clippy与系统工具；远程精确提交JSON已保存。仍未交付设备，进度不加点。

2026-10-23，按用户新指示转做其他固件任务。新增有界事件追赶（每类最多8页/256条），列表刷新成功后才提交对应游标；失败可原位置重试，退出/刷新重置任务代次和游标。实际 app 接线/迟到回复隔离及六组网页回归、Linux Edge 26项与Clippy、format/diff检查通过。新脚本路由与CI步骤已接入，未部署或连接设备；**整体仍76.5%**，见 [P2C](VALIDATION-WEB-EVENT-SYNC-P2C.md)，私有索引 `.private/web-event-sync-20261023-first.json`。设备组成/流量关闭状态沿用P6Y最终检查，不冒充本轮实测。电源/内核调试按用户要求搁置；另新增 [可转发的关机证据摘要](POWEROFF-SHAREABLE-BRIEF-2026-10-23.md)，不依赖本机路径。下一项可继续R04/R02准备，不重跑关机/安装helper。

## 最新：按用户要求整理关机问题，交由用户研究

2026-10-23，新增 [关机排查与代码接手说明](POWEROFF-INVESTIGATION-HANDOFF-2026-10-23.md)，区分“poweroff 后重新启动”和“返回 boot 的 SMD 内核异常”，列出固定源码、已验证指令映射、私有报告与尚未验证的生命周期时序。当前没有 SMD 补丁或离线复现结果。此次仅整理已有证据，无设备连接、加载、关机、刷写或流量测试；WSL 补充读取被本地访问限制拒绝，未提权重试，不影响既有报告。用户本次要求文档交接，后续硬件调试未开始；整体仍 **76.5%**，不将文档整理计作修复完成。

## 最新：P6Y临时试验结束，已恢复；发现基带恢复路径内核Oops

最终普通插回后的只读检查`.private/pmic-final-health-20261023-second.json`已通过：USB/固定身份SSH正常、联通home、普通gsm断开/无gsm配置、临时PMIC模块/目录/阻止租约/计时器均消失、PCM关闭、正式业务仍未初始化，监测/灯控active。boot列表与原关机日志确认测试后确有新boot，其基带恢复时`qcom_channel_state_worker+0x268/0x31c` NULL指针Oops及后续watchdog是新证据；不能继续把会话超时仅当网络问题。未证明PMIC直接导致该故障，也未验证关机修复。下一步定位固定内核SMD/rpmsg恢复路径，同时继续R02准备；**整体76.5，goal active，目前不需用户动作。** 下段等待状态为过程历史。

2026-10-23，用户明确授权一次临时加载及一次关机。候选在`/run`经摘要/版本核对，电源键阻止租约/卸载定时器建立后，实际加载/绑定/notifier、新输入设备/无异常按下及同boot维护状态通过；随后只关机一次。USB重新枚举，两个只读SSH会话均超时；首次用户操作后NAS识别9008，未打开刷写器或reset，再请求无按键普通拔插。随后恢复情况以上述最终复核为准。**候选不是已验证修复。** 详见 [P6Y](VALIDATION-PMIC-TRIAL-P6Y.md)。

主要证据`.private/pmic-load-and-poweroff-20261023-first.json`、两个`pmic-reconcile`失败报告及`pmic-nas-usb-20261023-first.log`。P6X“缺驱动”事实保持，但实测证明补齐后仍未得到预期停机，不能继续把它当充分因果解释。现场只做过临时RAM模块，无自动加载/模块目录/原始分区写入。恢复后必须重核普通数据关闭、临时模块/租约/计时器、PCM和正式业务门禁；关机前状态不能冒充恢复后状态。禁止重跑已消费加载/关机helper；不为恢复而自动刷旧计划。后续继续目标，需用户处先询问，未暂停或完成goal。

## 最新：关机重启诊断完成，PMIC候选仅离线验证

2026-10-23。实际poweroff日志确认正常停止/卸载后进入关机，随后新boot；当前内核`INPUT_MISC`未开，PMIC pwrkey节点存在但无驱动/模块/notifier。固定上游驱动负责关机时设置PMIC shutdown类型，缺失是高可信原因，尚未做受控因果验证。新增独立实验配置片段，仅两项有效改变；新输出严格编译生成17,344字节ARM模块，版本/别名/24项原导出核对通过，原源码/配置/导出保持。第一、二轮构建环境/输入失败保留，第三轮通过；未放宽检查。见 [P6X](VALIDATION-POWEROFF-DIAGNOSIS-P6X.md)。**整体仍76.5/100，不宣称修复已上机。**

成功索引`.private/poweroff-offline-candidate-20261023-third.json`，制品`.local/poweroff-driver-candidate-20261023-third/`；最终只读复核`poweroff-final-readonly-20261023-first.json`通过：同一boot、原卡数据关闭、无gsm连接配置、PCM关闭、正式业务仍未初始化。没有加载模块/再次关机/刷写，现用系统版本不变。后续单独协调候选交付、输入事件策略及有恢复条件的关机实测；本次诊断收尾，不自动开始部署或R02。不重跑已消费helper。

## 最新：网页自动检查补齐，完整GitHub运行通过

2026-10-22，用户追加一个小任务。此前CI只跑网页灯控，已补齐身份/首次owner、音频生命周期、一键通话及网络/月策略四组既有回归；五组本地均通过，配置提交a679da8的 [完整GitHub运行39723798698](https://github.com/Viper-Boss/RustSimHub/actions/runs/39723798698)已success，包含Rust/生产检查/Clippy和系统工具。细节与命令见 [P7F](VALIDATION-WEB-CI-P7F.md)，私有证据`.private/web-ci-20261022-first.json`及`web-ci-run-20261022-first.json`。**整体仍76.5/100，不为已有功能重复加点。**

本轮没有访问NAS/棒子、重建/部署固件、启动正式业务或使用普通数据，实际硬件组成及流量关闭状态沿用下方R11最终复核。工作流版本和设备制品版本分别记录；当前文档HEAD不能冒充新固件。保存并同步本单项后停止，下一项仍R02及poweroff诊断，不重跑已消费的实机helper。

## 最新：R11移动数据、后台网络模块及月限额实测收尾

2026-10-22。用户换移动卡并明确授权流量测试、持久安装网络监测/守卫；完成真实DNS/严格HTTPS、接口绑定、1MiB下载短样本约2.16Mbps、独立月账本、实际256字节剩余额度阻断和普通gsm断开。管理网页新增IP/DNS/APN、无线/小区参数、速率曲线、月统计/限额/重置日及授权主动测试。**整体76.5/100，P4由7.5增至8.5；R11部分完成，其他里程碑不加点。** 详见 [P4B](VALIDATION-CELLULAR-NETWORK-P4B.md)、[模块接口](network-diagnostics.md)。

当前交付业务/Native为38366a8、原开发签名身份factory epoch13；Root网络程序为a239f7f；Root灯控保持0c817c1。bbb安装组成与既有维护补丁保持，未刷写原始分区或整张新镜像。原2b/epoch7收据与备份只作历史，以本次交付/最终检查索引为最新文件依据；system-candidate原来源不改写。正式owner、installation.complete及app/state.json仍不存在，正式Core/Edge/Media仍inactive；临时隔离身份的真实API测试不等于首次设置已完成。

232项Rust、8个Rust ARM构建、Native21项、最终Root6项和网页/系统回归通过。实际新业务受限自检、合成Opus500帧/DTLS各50帧通过；正式新系统的真实通话/短信/双向音频仍缺。本轮新增电话0/SMS0，累计10010仍38。

用户要求测试后关流量并已换回原联通卡。**最终只读复核home注册、普通gsm断开、没有gsm自动连接profile、原卡月账本0、监测/灯控active及PCM关闭。** 移动卡非零账本1,152,128字节已保存并在重启后保持；限额默认恢复关闭、10GiB/每月1日，与普通数据已关闭分开记录。不得因换回原卡、Wi-Fi故障或维护任务重新激活APN/互联网探测。

正常poweroff后棒子重新枚举并返回新boot；仅恢复NAS指定USB维护地址后验证数据仍关闭、Root服务和账本正常。**关机自动回启动尚待诊断，不能声称成功停机或完整冷启动验收。** 换卡后再次只读复核通过；未自动重跑关机/安装/首次初始化。

最新私有证据：`.private/r11-live-network-20261022-fourth.json`、`r11-business-delivery-20261022-second.json`、`r11-final-check-20261022-first.json`、`r11-return-data-off-20261022-second.json`、`r11-original-card-return-20261022-first.json`均passed=true；两个成功退出的Native构建容器已清理，日志/源码/输出保留。最初失败记录保留，不重跑任何已消费helper。本轮收尾保存，不开始R02；下一项协调正式owner/端点/可信TLS初始化、正式启动与冷启动，并诊断poweroff返回。

## 此前：R01-C已在新系统内交付签名业务及配套文件（历史）

2026-10-22，用户追加一个任务。严格固定公钥维护SSH及动态modem枚举只读基线通过：home注册、无通话、PCMclosed、普通gsm未连接、业务未初始化。备份并回读13份旧文件后，通过原子目录交换交付2b/epoch7 factory和新六程序slot，替换update/edge-setup/migrate、Core单元、tmpfiles及0c Root灯控六文件；公钥保持。**设备上8个程序来源、Core自检、UID984无设备/外网的合成Opus500帧与DTLS双向50帧通过，灯控采样正常，初次设置门禁实际跳过且未创建台账。整体75.5/100。** 见 [P6W](VALIDATION-DEVICE-DELIVERY-P6W.md)。

唯一交付索引 `.private/r01c-delivery-20261022-first.json` passed=true；门禁/收据 `.private/r01c-final-gate-20261022-first.json` passed=true。PC备份 `.private/r01c-backup-20261022-first/`；设备旧factory位于 `/usr/share/rustsimhub/.factory-app-r01c-2b3c49c-first`（Root0700），收据 `/usr/share/rustsimhub/maintenance-receipts/r01c-20261022.json`（Root0600）。新manifest `cee2f68e30eafbe61f0871982398c14060ffee52cf6c9fbb8920fef877eba256`；epoch7是factory版本，state.json仍不存在，不能当作floor已推进。实际灯控现为0c来源，不能继续按旧2b二进制台账报告。

**当前系统是bbb安装组成+既有维护补丁+本轮文件交付，不是整张P6V镜像已刷入。** 原system-candidate.json/安装出处保持，实际文件状态以本轮收据/索引优先。没有重启、原始分区写入、身份初始化、正式Core/Edge/Media启动或新通话/短信；新二进制受限测试不替代真实业务/冷启动。下一项确定USB/Wi-Fi端点和可信TLS/媒体交付后用新工具首次初始化、启动及验收，不重跑本次helper或旧初始化/安装计划。私有接续/第三安装索引已更新，累计10010仍38，蜂窝等换卡；完成本单项后不启动下一项。

## 此前R01-B检查点：修正版系统与离线镜像已完成（历史）

2026-10-22，用户追加一个任务。镜像审计仍仅接受旧NSH_STATUS_LED_ENABLED配置，实际拒绝新Rust灯控auto；已按精确auto/off字节及安全文件元数据修正，36项rootfs回归通过。冻结组成源码0c817c1，Root灯控按此源重建（9项）；业务仍采用已验证的2b同源Native/签名package7。**完整通用rootfs已组成，8个ARM程序运行、18单元静态依赖、镜像内合成Opus/DTLS及ext4逐项核对均通过。整体75/100，约75%。** 细节与未完成边界见 [P6V](VALIDATION-CORRECTED-SYSTEM-P6V.md)。

新唯一索引 `.private/r01b-system-composition-20261022-first.json` passed=true；Windows制品 `.local/r01b-system-image-0c817c1-first/`。镜像3436183552字节，SHA256 `05029f1424e5ad1d85ad0d991f322fda49832ef3e99554a06a1f6fc38af2d852`；树12373项，SHA256 `784247ca5a70b54ff22800409aaa102a04b107cec6a06b1f6f63a3d45eedf87b`。原WSL树/镜像在P6V列明。输入/基底/源码保持，失败目录及日志保留，无活跃构建或安装。静态检查保留两条Debian包单元弃用提示，不当作实机启动通过。

**本轮未连接NAS/棒子，未部署或初始化业务。** 实机状态沿用2026-10-21最后记录，实际已运行灯控仍2b，不能用新候选0c覆盖设备版本台账。新镜像无私有固件/设备状态，flash/release/boot/recovery门禁仍false，R01剩余交付/初始化和硬件验收；累计10010仍38，蜂窝等换卡。下一项用P6V准确输入重新核当前设备/备份后准备交付，不刷旧3da、不重跑已消费helper。接续索引已更新，完成本单项后不启动下一项。

## 此前R01-A检查点（历史）

## 最新：R01-A同源业务候选准备已完成

2026-10-21，用户追加一个任务。本轮完成最新2b3c49c同源Native（21项）、七ARM程序非Root隔离运行、沿用原开发签名身份的epoch7/8/9应用包，以及实际ARM更新器8条拒绝路径、选择/未确认回退/正确确认和合成Opus/DTLS闭环。整体 **74.5/100，约74.5%**，P6仅增加0.5点离线候选准备进展。准确结果与边界见 [P6U](VALIDATION-SIGNED-CANDIDATE-P6U.md)。没有设备连接、刷写、业务初始化、新通话/短信或普通蜂窝；完成本单项后不启动下一项。

唯一新索引 `.private/r01-candidate-preparation-20261021-first.json` passed=true；七Rust制品沿用冻结2b及原归档5d61a1，Native制品 `.local/native-led-candidate-2b3c49c-second`、签名包 `.private/boot-application-led-candidate-2b3c49c-first`、LED组件输入 `.local/status-led-candidate-2b3c49c-first` 已保存。Native固定镜像实际Rust1.99，与七Rust程序1.98分别记录；首轮工具链选择失败、更新夹具三轮权限/执行失败及清理前语法错误均保留。最终Root更新夹具无宿主绑定、无能力/外部网络，不改变生产安全边界；两个已退出构建容器已保存日志后移除，源码/输出/缓存保留，无活跃构建或安装。

当前棒子运行状态沿用下方灯控完成时记录：业务仍4e6且未初始化。**同源Native与签名包已准备，完整系统重新组合、交付和实机验收仍未完成。** 下一项从冻结2b Git树使用上述输入组成修正版，再独立验证；或者新选源码并全部重建。不能伪造revision让文档HEAD匹配旧制品，不重跑已消费helper，不刷旧3da镜像。接续索引 `.private/third-attempt-continuation-20261021-first.json` 已更新；原安装第三索引仍是设备依据。

## 此前文档同步与灯控检查点（历史）

**GitHub同步与接手补充：**用户要求同步最新代码后，正常 `git push origin main` 已成功把远端从11f80da推进到08f29f4，未强制改写历史，工作流改动完整保留。此前Git对象API目录创建HTTP 404的失败日志仍保留；CLI缺少workflow范围不能单独解释全部发布路径，不能再将它当作当前同步阻塞。详细剩余工作、依赖、验收标准及可复制的Claude接手指令见 [剩余工作与Claude接手清单](REMAINING-WORK-AND-CLAUDE-HANDOFF.md)。本次只整理文档并同步GitHub，固件开发继续暂停，未连接NAS/棒子。

2026-10-21。整体 **74/100，约74%**，固定权重见 [进度刻度](PROGRESS-SCALE.md)。本轮灯控修正、验证和制品保存已收尾，按用户要求暂停。原先不亮已解决，用户确认白色慢速亮灭正常；事件接线业务程序尚未启用，不以灯语代码替代新业务实机验收。完整历史原文见 [本轮前归档](archive/handoff-before-first-os-consolidation-20261021.md) 和 [更早归档](archive/handoff-before-oneclick-consolidation-20261016.md)。

## 当前灯控任务检查点（优先于下文历史）

最终执行程序已更新为精确提交`2b3c49cc469bce5e0f59ae377f9a59db9f9d0a89`，源码归档SHA`5d61a14119580fe216e5922961cf537a0b0673ec9df33e610ed731f23dfb0f61`。七个生产ARM程序已由本机交叉编译成功，归属及glibc边界通过；制品`.local/wsl-artifacts-2b3c49cc469bce5e0f59ae377f9a59db9f9d0a89`，索引`.private/led-final-build-20261021-first.json`。本轮**仅部署其中的灯控程序**：备份旧可执行文件、核固定旧摘要、暂存新程序并以非Root验证build-info、原子替换和fsync、启动/8秒RGB采样均通过。最终记录`.private/status-led-final-deployment-20261021-first.json` passed=true，enabled/active/running、空capability、NRestarts=0，采样MemoryCurrent=618496/MemoryPeak=2723840、两线程；不是长期性能基准。旧58a43de部署和用户确认报告保持，不把其旧二进制摘要冒充当前版本。

本次构建曾因前面Root测试留下的Cargo缓存所有权失败，原日志/exit另存`before-cache-owner-fix`后，仅在核实的本项目target缓存内恢复普通构建用户所有权，再完整构建成功；不删除失败源码目录、不改工具链或宿主其他路径。七ARM制品保存不等于七程序已上机：Core/Edge/updater的事件接线仍待同源Native及签名业务组交付，随后才可初始化业务。此暂停点没有活跃安装器、构建任务或待用户操作窗口；正常灯控服务继续运行。

原先完全不亮的原因是新系统没有状态灯服务，驱动和三个GPIO LED通道均正常。独立Rust灯控58a43de已安装为四个维护文件，服务enabled/active、空capability且只写三条固定LED路径；8秒RGB采样和用户确认均证明白色慢速亮灭正常。记录`.private/status-led-deployment-20261021-first.json` passed=true；用户确认已单独记入，不把纯软件帧测试当作肉眼确认。初版纯绿heartbeat方案未部署。实际运行系统为bbb1081+此前两个维护补丁+灯控四文件，不再按原rootfs摘要宣称现场未改。

完整颜色/优先级/低频提醒规则见[指示灯约定](status-led.md)，源码及验收报告见[P7E](VALIDATION-STATUS-LED-P7E.md)。Core配置租约、查看后显式确认未读、网页保存/关闭及Root更新接线代码已完成；222项Rust、生产Core/format/Clippy及网页关闭顺序检查通过。系统工具139通过/1项缺make跳过，另用验证过的make补跑8项全部通过。事件采样与动画分线程，不高频写sysfs模拟GPIO不支持的平滑呼吸。

**交付边界：当前业务程序仍4e6fea6，尚未初始化，新增Core/Edge/updater接线尚未上机。** 先完成灯控制品收尾，不自动开启新业务初始化。真实网页关闭、未读短信/未接来电、升级和蜂窝联动不能声称已验收；蜂窝继续等换卡。新业务制品仍需同源Native/签名交付，旧3da离线镜像有24位音频守卫缺陷，不得直接安装。累计10010仍38，未新增通话/SMS/普通蜂窝。完整安装、维护SSH和此前硬件健康证明保持，冷启动未复验。

暂停后接续以`.private/third-attempt-continuation-20261021-first.json`和第三安装索引为准，不重跑已消费安装器/一次性连接/校时helper。继续时先核当前灯控源码及制品版本，交付包含音频/恢复/灯控修正的安全业务候选后再启用业务。下文维护暂停和安装过程为历史。

## 此前维护修正检查点（历史）

第三安装已退出0：p14+p15完整写后回读、保护范围/GPT/boot、两FS及正常reset ACK通过。新系统USB和固定主机公钥/预选客户端密钥维护SSH登录均已通过；安装器、monitor及一次性接续均已结束，禁止重新执行已消费的安装/连接helper。实际系统仍是bbb1081组成、4e6fea6应用，业务未初始化。唯一设备索引为 `.private/install-first-boot-20261021-third.json`，后续普通维护连接用 `.local/new_os_third_ssh_20261021.py`，接续检查点为 `.private/third-attempt-continuation-20261021-first.json`。

初次首启健康检查曾失败：音频守卫误要求24位srcversion，而固定来源的16项实际均为23位、实机16项身份全部匹配。已修正为严格23位，保留完整值匹配、固定加载列表和服务隔离；以真实固定profile替换合成测试数据，增加长度/大小写等拒绝测试。140项完整系统工具回归通过（7.024秒，无跳过），报告 `.private/system-tools-audio-profile-fix-20261021-second.log`。首轮其他隔离夹具失败记录保留；夹具现显式等待并核验自身loop的udev标签，生产状态守卫未变。

用户追加部署授权后，通过现有固定公钥SSH，将提交7d307ce的音频守卫和app-recover条件单元部署为**第三系统上的两个文件维护补丁**。部署前逐文件核原bbb1081清单摘要/Root权限/非链接并保存原件；同目录暂存、摘要验证、fsync、原子替换，部署后摘要及权限再验。记录 `.private/third-runtime-audio-fix-20261021-first.json` passed=true，两个文件均hash_verified。音频单元active/exited、Result=success、ExecMainStatus=0，CAP_SYS_MODULE及PrivateDevices/内核保护保持；16项身份匹配。app-recover因未初始化ConditionResult=no、inactive，未触发业务恢复。没有重启或块设备写入；实际状态应描述为bbb1081+这两个补丁，而非原始镜像逐字节未变。

部署后只读健康复核已通过：持久状态/固件守卫、固定音频模块、无线处理器、系统服务、ModemManager、PCM空闲、普通蜂窝未连接以及时钟检查通过，报告由第三索引 `first_boot_health_observations` 保存。另在部署前后确认基带home注册、业务未初始化、无通话、boot_id相同。**这是当前启动中的修正复核，尚未重启或冷启动验收，也未进行新系统业务听音/短信测试。**

已观测ModemManager/rmtfs/radio-resume工作、基带恢复后注册home、普通gsm未连接、无通话/PCM关闭；启动曾发生一次modem watchdog，长期稳定性未验收。设备时钟已校正一次，后续只读观察与可信时钟相差约9秒；不重复执行旧校时helper，冷启动时间保持仍待验证。未初始化app-recover的226失败由此次补丁消除，原失败报告保留。

TLS修复3da4679的ARM/Native、签名包和离线镜像已验证但未部署；该候选仍含修正前的24位音频守卫，**不得直接安装**。恢复后先纳入本次音频修正重新组成并验证，再交付修正产物、完成实机启动健康和业务初始化；不要直接运行旧owner初始化helper。累计10010仍38次，无新短信/普通蜂窝流量。暂停期间无需用户操作；下文“正在安装/SSH拒绝/待观察”为已结束的历史。

## 本轮较早的捕获、安装准备及构建历史（已由上方更新覆盖）

**第三安装前只读捕获（历史）：**新的只读捕获索引为 `.private/new-os-maintenance-diagnostic-20261021-second.json`。用户本轮已完成按键；完整3909091328字节捕获、摘要/GPT/boot、原图保持及loop分离通过，探针退出0，无分区写入或reset。原启动因SSH不可用未正常关机，两FS只读退出4，失败保留。日志确认SSH准备失败、音频检查失败和未初始化时app-recover缺运行目录，尚不能断言维护修正解决全部启动故障。

当前状态独立副本强制preen退出1→只读0，旧101项完全一致、业务目录未初始化且为空、维护/USB材料保持，原图保持。第三安装因此选择同一原始干净维护状态及bbb1081新rootfs；独立 `.private/install-first-boot-20261021-third.json` 的准备/两FS/离线验证全部通过，准备进程退出0。冻结执行工具从已通过CI的c5ee339准确Git对象读取，不用当前HEAD冒充其来源。已在同一已记录9008会话启动一次第三安装：先另存完整现场、核精确未干净摘要，再仅写p14+p15、完整回读/保护范围/两FS和正常reset。结果尚待完成，禁止重复启动或在Unknown后重试。唯一只读monitor为 `.local/monitor-install-third-boot-20261021.py`；执行句柄及准确接续保存在 `.private/third-attempt-continuation-20261021-first.json`。目前无新的物理动作要求，保持连接。

自动审批服务已恢复，同一正确路径的8项内核工具夹具及139项完整系统工具测试均通过，无跳过。新second目录的完整内核/模块集成复现已退出0，181.333秒；两种内核格式及681个打包模块全字节匹配，源码/配置保持。当前工具证明见P6T；硬件捕获与该本机构建相互独立。

已将工具和证明发布到GitHub main `f4bc8313bb8c0851cf6feb907d1fb6fe64d477d9`，[完整CI39541470946](https://github.com/Viper-Boss/RustSimHub/actions/runs/39541470946)成功。没有用CI绕过审批；先在正常审批下完成当前实现的本机验证，才发布。

另修正已确认的未初始化app-recover命名空间失败：恢复单元加入与选择器一致的持久安装完成条件。真实systemd临时单元复现修正前失败，修正后初始化前跳过、初始化后缺目录仍226、一次恢复/第二次停组及marker保持均通过；140项系统工具回归通过。临时业务控制为记录桩，未调度任何真实业务。**这项源码修正不在正在安装的bbb1081冻结镜像中**，不要以当前HEAD推定已上机；实际维护连接后仍须检查音频失败及恢复单元，再决定如何纳入下一通用候选。详情见application-startup-group.md。

**启用业务前新增门禁：**4e6fea6原始锁文件RustSec扫描workspace命中5条TLS公告，Native为0。当前只更新rustls至0.23.46、webpki至0.103.16；同一官方数据库复核均0漏洞，209项Rust/生产检查/Clippy通过。5项维护状态警告仍记录，不忽略。3da4679完整CI39546547484、六ARM构建和NAS隔离实际运行、同源Native21项与构建、复用已选密钥的epoch4/5/6签名均通过。新12367项通用候选（3da组成/3da应用）及3436183552字节完整镜像逐项/只读FS、七ARM非Root和静态启动依赖通过；tree/image摘要和索引见[依赖复核P7D](VALIDATION-DEPENDENCIES-P7D.md)。新制品仍在本机未部署，**当前第三安装的bbb1081仍用旧4e6fea6程序**。先观察第三安装并取得维护/音频日志，**暂不运行业务初始化/owner helper**，完成修复产物交付和真实健康检查后再继续。恢复条件修正6b23340完整CI39544172775也已成功。

## 第二次安装历史现场

**第二次安装已退出0：两分区最终ACK、3909091328字节完整写后回读、目标/保护范围/GPT/boot、写后两FS只读0和正常reset ACK均通过。新USB接入通过，维护SSH有界等待仍拒绝。当前没有活跃写入器或监视进程；保持现状，用户已休息，不请求拔插。**

- 唯一索引：`.private/install-first-boot-20261021-second.json`。
- `.local/monitor-install-second-boot-20261021.py` 已按安装exit0正常结束；连接helper也已退出1。`.private/new-os-second-connect-20261021.log`保留有界拒绝；该索引另记安装成功与维护失败，不能再运行second激活或安装器。
- 冻结安装工具源：`a9cf521af45df7aee431c3e0e8b3cc19445e0921`；完整CI39493697545成功，130板级/34rootfs/7恢复测试通过。
- 第二3909091328字节匹配候选及写后现场均已验证。真实计划/工具目录/摘要只从私有索引读取；PID为历史记录，不代表仍运行。
- 第二安装复用了原固定9008会话，随后已经正常reset；现在是新系统USB，不能假定仍在9008。
- 第二执行前重新捕获的完整现场与当时明确选择的未干净摘要一致，原失败只读FS报告保持false，另记独立替换确认。这是第二次已完成事务的历史证明，第三计划不得继承该现场摘要。
- 第二仅写p14/rootfs、p15/NSHSTATE，写后回读/当前保护范围/两FS/冻结源通过后，仅发一次正常reset并获ACK。NAK/短写/Unknown/检查错误不自动重试或reset的规则保持。
- 第二安装写后结论已通过，但未取得维护公钥登录、完整首启健康或业务初始化；`first_boot_health_verified=false`。没有新拨号/SMS/普通蜂窝。

## 已完成事实与本轮来源

详细报告见 [首次整机安装 P6S](VALIDATION-FIRST-OS-P6S.md)。

1. 第一次安装源8d2fd5a，两个固定分区完整发送及最终ACK、完整写后捕获/目标/保护范围/前后两FS检查和reset ACK均通过。索引 `install-first-boot-20261021-first.json` 保留。
2. 首次新USB身份出现，NAS仅激活选定静态USB配置，直达路由正确、默认路由保持；SSH持续拒绝，未取得维护登录。持久日志确认firstboot成功、SSH准备失败及依赖失败，另有音频驱动检查失败和未初始化时恢复服务运行目录缺失。不能声称仅修SSH就解决全部启动问题。
3. 原SSH准备服务的PrivateDevices隔离隐藏物理/dev，而旧状态守卫要求其存在。修正保持隔离和空capability，改为真实挂载MAJ:MIN与固定sysfs物理分区/父盘/分区号/内核PARTNAME等核验。真实一次性ext4隐藏/dev及错误设备号/只读拒绝回归通过。
4. 修正通用树源 `7d1860a72c0b70f0507ea756d2bf482bc2e19577`：12367项，tree `52ab3e371d3bd84d1d90a4e3d67bc395b2ce671876a1cd0abc3f778206afc4d7`；3436183552字节rootfs `6870406b1a71ab3f9e1270169481dfb243b05c952a8c1012ff0408f0e5ecd1fc`。七非Root ARM程序、静态启动依赖、只读逐项核对/e2fsck通过，NAS副本独立冻结。应用及Native仍4e6fea6，不重标为新编译业务。
5. 新启动现场完整只读捕获、GPT/boot及原图保持通过，索引 `new-os-maintenance-diagnostic-20261021-first.json`。因无法正常关机，原图两FS只读均4，所有失败保留。
6. 状态第三份新副本强制preen退出1、随后只读0；旧101项内容与元数据完全一致、业务目录为空、所选维护/USB材料保持，loop分离和原捕获完整摘要保持。前两份未清除计数/orphan问题的失败副本也保留。第二安装因此可使用同一原选定未初始化维护状态，未替换用户业务身份或记录。
7. 完整旧系统回刷演练仍未通过。用户明确“直接测试新系统”“你想怎么测试怎么测试”，改变了顺序；不把这条授权记成recovery_verified=true。原32KB configure NAK失败和标记不删除，第一次冻结工具也不修改。

## 下一次上机前的修正

已在本机复现另一处组合缺陷：`ProtectSystem=strict`使状态父挂载只读，`ReadWritePaths`仅将system子树设为可写；原守卫无条件要求父挂载rw。新增仅供SSH准备的 `--system-writer` 模式，仍核固定物理p15，并验证实际system写入点为相同设备、rw、非链接、Root0700；其余调用默认仍要求父挂载rw。真实ext4/只读父bind/可写system bind/隐藏设备/空capability、真实预选Ed25519 SSH准备通过，宿主钥匙保持及子树外写入拒绝通过。131板级测试通过。

这是已复现的源码问题，尚未取得第二次启动持久日志，不能断言它是现场唯一故障。音频检查补充固定阶段失败诊断，未放宽任何身份/版本/加载守卫。修正源 `bbb10818e4640412aaacdae3247f156c1fe3be1b` 已重新组合新通用候选：12367项/tree `ccd56c0480bd9c0c0b99277c0b1915546001adb3706fa41648ca4d2842239a8d`，3436183552字节镜像 `608ebe8dd078d39da8ec0958571e498c3b4e6675d5a678998d86a4e9dedb389c`。七ARM非Root程序、静态启动依赖、完整逐项镜像和只读FS检查通过；业务/Native仍4e6fea6。私有manifest前缀 `system-image-p6s-system-writer-bbb1081-first`，树/镜像位于WSL cache同名前缀。未安装；安装器后续仅固定新摘要，旧冻结安装工具保持。

后续需要用户醒来后配合新的只读恢复现场捕获，取得第二启动日志；重新核维护/业务状态后再制定独立第三安装计划。不要复用第二尝试标记、旧现场摘要或二次激活。当前不运行新的捕获/安装watcher，不请求用户操作。

新候选已完成PC独立传输复核，并在NAS全新Root私有目录复制、fsync和再次完整哈希冻结：`.private/system-image-p6s-system-writer-nas-transfer-20261021-first.json`，passed=true，未开USB。唯一后续准备索引 `.private/next-new-os-candidate-20261021-third.json` 记录准确NAS位置/制品及下一步；它不是安装计划，未选择p15/新现场摘要、未请求物理进入或开始硬件动作。安装器固定源 `c5ee3397a595d53be3b90e823682d08548b07b2b` 的[完整CI39505200290](https://github.com/Viper-Boss/RustSimHub/actions/runs/39505200290)成功，含131板级/34rootfs/7恢复、Rust测试、production检查与Clippy；组合修正bbb1081的CI39504723559也成功。

下列second版helper保留作接续模板，**已使用的connect不能重跑，后续应绑定新的独立索引**；尚未运行健康/校时/业务/owner步骤，不能先行绕过门禁：

1. `.local/connect-new-os-second-boot-20261021.py`：一次激活；严格物理口/USB serial/MAC、NAS配置与默认路由、选定主机公钥和PC持有维护私钥；核实际守卫字节、真实p14/p15和维护材料。不要使用第一次connect或旧device.json连接器，也不关闭主机公钥检查。
2. `.local/inspect-new-os-second-health-20261021.py`：只读采集真实单元、固件守卫、modem、radio、PCM、UTC以及16项音频srcversion。每次观察使用独立私有文件，失败保留；只有守卫、所需服务、radio和关闭PCM等全部通过才记首启健康。初次音频问题尚未解决，先看实际版本/单元日志，不放宽守卫。
3. `.local/correct-new-os-second-clock-20261021.py`：仅在已取得真实时钟偏差后按可信NAS/PC UTC校准；不启用普通APN或互联网探测。校准后重新观察，不把它当作RTC持久化已经通过。
4. `.local/initialize-new-os-second-business-20261021.py`：须实际健康和UTC已通过，可信初始化只执行一次；私有交付保存到 `.private/new-os-business-20261021-second`。started/Unknown不得盲目重建身份或设置码。
5. `.local/claim-new-os-second-owner-20261021.py`：已准备、未运行。PC生成并保留owner私钥，NAS只接收公有证书/请求；严格HTTPS及设备挑战签名核验，setup_finish之前记录一次性状态，再登录检查生产API与收件箱。未知setup结果先用原owner只读核验，不重复初始化。

上述9个私有helper及6段远程Python已做语法检查；后续现有owner只读登录helper也已检查，健康观察明确要求16项音频身份全部匹配。这些准备不算实机验收。新系统未安装Python，设备检查使用Shell/SFTP，Python仅运行在PC/NAS。

## 本机完整内核构建

最新覆盖：第七份完整构建已将Image及Image.gz逐字节复现到固定上游，完整来源树与配置保持；1759378798内置cpio时间和临时弱横幅无时间须用目标专属make变量，不能简单全局对齐横幅。681个原装模块经新副本 `objcopy --strip-debug` 后681/681等同原上游包，原输出保持；第一次未打包比较false保留。依据及准确摘要见[P6T](VALIDATION-FULL-KERNEL-P6T.md)。设备、NAS候选和c5ee339冻结工具没有改动。

`tools/rebuild-base-kernel.py`整合ar/tar模块验证、新副本去调试符号和非普通输入预打开拒绝，当前8项夹具及完整139项系统工具回归全部通过。此前审批服务额度失败后没有改换入口或推送CI绕过；服务恢复后才按正常审批重跑。新的真实集成构建也已通过，前版及独立私有证明分别保留。

`.local/run-public-kernel-rebuild-second-20261021.py`已执行成功，不得重跑或覆盖first/second目录。最新报告在WSL cache `full-kernel-rebuild-public-20261021-second/BUILD-REPORT.json`及`PACKAGED-MODULES.json`，PC日志同名前缀。历史证明 `full-kernel-rebuild-20261021-seventh.json`、`stock-module-packaging-reproduction-20261021-first.json`及first公开工具报告保持。仍缺完整外部initramfs重建、新内核实机/维护首启及恢复，不改变74点和刷写门禁。

历史第五/第六份报告与未匹配结果保持，不能改写为第七份成功。原配置没有非零CRC，基础内核与打包模块全字节相同不等于另做了CRC兼容性或实机验收。所有重建未访问设备、生成/写入boot或替换冻结安装输入。

## 持续边界与待验收

- 整体固定P0—P7=74点：7、14.5、15、14.5、7.5、5、6.5、4；实际新系统健康/业务验收后再按里程碑更新，不按测试数加点。每次最终回复报告百分比。
- SIM无流量。换卡且用户明确安排前，不启普通蜂窝APN、不让USB/Wi-Fi隐式回退蜂窝、不跑蜂窝互联网探测；保留电话必需WWAN/IMS。
- 用户持续授权10010短通话，累计 **38**，本轮没有新拨号/SMS/录音。每次独立幂等键和防重标记、有界自动挂断；Unknown先核验，不自动重拨。其他号码或新短信不能推定授权。
- GPT/boot/基带/NV/校准区不写。基础内核与681原装模块的整合字节复现已通过；完整恢复、新内核上机、掉电/更新恢复、自动健康确认、远程TURN/蜂窝/长期验收等仍缺。
- 前次4e6/a19真实Chrome已确认首句及持续下行完整、自动挂断、观察接通至接音0.7秒；不是新OS或双向验收。详见 [浏览器证据](VALIDATION-BROWSER-MEDIA-P3I.md)。Windows临时预览证书已经实际移除；新证书未授权加入系统信任，不复用旧证书授权。
- 后续确需物理冷启动/恢复按键、人工双向/来电听音或换卡时，保存现场并等用户醒来；继续推进其他不依赖人工的任务。当前不再发送操作请求。
- 设备标识、凭据、owner/维护私钥、原始日志/短信/快照仅.private/.local，禁止提交；只从私有索引取真实位置/身份，不猜测。
