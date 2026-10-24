# 最新开发进度：2026-10-23

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

发布补充：`91cb708`已同步，完整 [CI 39835048209](https://github.com/Viper-Boss/RustSimHub/actions/runs/39835048209) success；精确编译输入、模块实际核验和CI各自边界已记录，设备未改动，整体仍76.5%。

**pon_dump片段已审查并实际离线编译，整体76.5%不加点。** 收到源码后修正读reg失败的pdev泄漏及读取全失败仍成功的状态，移除缺依据的0x0e；实际init模拟依赖10分支与原版负面对照通过。原版逻辑副本/审查稿真实ARM64模块分别7568/7776字节，版本/9和10强依赖及原配置/导出通过；首次仅Git元数据收尾失败保留false，second独立核验通过。现有匹配ELF无DWARF，尚不能按所提方式得到准确行号。未访问/加载到设备或关机，继续搁置实机电源调试；详见 [P6Z](VALIDATION-PON-DUMP-REVIEW-P6Z.md)，私有索引 `.private/pon-dump-review-build-20261023-second.json`。并发解绑和寄存器读取无副作用均未证明。

**补充分析已核对，整体仍76.5%。** 用户转来关机/SMD分析后，仅检查固定源码、已有参考配置和原始上游讨论，保存 [核对说明](POWEROFF-EXTERNAL-REVIEW-2026-10-23.md)。明确PMIC revision分支、PON原因的boot对应及推论边界；没有新的UAF复现或寄存器实测，没有收到pon_dump附件。未连接设备或恢复关机测试，其他固件任务继续按HANDOFF推进；P2C仍已验证未部署。

发布补充：网页补同步实现 `cede3d7` 的完整 [GitHub CI 39829439401](https://github.com/Viper-Boss/RustSimHub/actions/runs/39829439401) 已success，精确源提交与结果核对并保存；代码/交接材料已同步，设备未更新，整体仍76.5%。

**网页事件补同步修正已完成本地验证，整体仍76.5%。** 用户要求搁置关机问题，转做R04离线部分：新增每类8页/256条有界追赶，列表刷新成功才提交游标，失败保留原位置，退出/刷新清理和迟到回复隔离；六组网页回归、Linux Edge 26项及Clippy、format/diff通过，路由/CI已接入，未部署或访问设备。详见 [P2C](VALIDATION-WEB-EVENT-SYNC-P2C.md)，索引 `.private/web-event-sync-20261023-first.json`。另已提供可转发关机证据摘要；不继续电源/内核调试，无新电话/短信或普通蜂窝激活，实际组成保持。

**关机问题交接文档已整理，整体仍76.5%。** 按用户要求，仅整理已有证据形成 [详细排查交接](POWEROFF-INVESTIGATION-HANDOFF-2026-10-23.md)。PMIC 缺驱动已确认但补齐后仍关机失败；SMD worker 的 NULL 异常及匹配指令已定位，销毁期间迟到排队仍为待验证推测，无修补制品或复现结果。读取已有报告、基线提交与固定进度表完成；补充读取 WSL 源码遇本地访问限制，未提权重试。未连接 NAS/棒子、执行驱动加载、关机、刷写或蜂窝测试。下一步交由用户阅读固定源码与私有报告。

**P6Y最终恢复检查通过，整体仍76.5/100。** 普通插回后USB/固定身份SSH、联通注册/数据关闭/无gsm配置、临时模块与控制清理、PCM及业务门禁均通过。取回日志确认测试后新boot中基带恢复触发`qcom_channel_state_worker` NULL指针Oops与后续watchdog；后续优先查内核恢复路径，不永久安装未修复关机的PMIC候选。当前不需用户继续操作，goal active；下段等待描述为过程历史，最终索引`pmic-final-health-20261023-second.json`，见 [P6Y](VALIDATION-PMIC-TRIAL-P6Y.md)。

**P6Y实际临时加载与单次关机已执行，修复未通过，76.5/100。** 用户本轮授权后，实际PMIC模块绑定/notifier及输入/同boot维护检查通过；只请求一次正常poweroff，USB重新枚举但两个SSH会话超时。用户恢复操作后NAS识别9008，未启动刷写或reset；已请求完全松键普通拔插，等待恢复后复核。缺驱动事实不足以单独解释结果，尚未永久安装、未确认清理/新正常boot，不能增加完成点。详情、唯一证据和当前接续见 [P6Y](VALIDATION-PMIC-TRIAL-P6Y.md) 与HANDOFF顶部；100%目标active。
**关机后重启只读诊断及离线候选已完成，整体仍76.5/100。** 真实关机日志正常进入poweroff；现场内核缺少PMIC电源键/关机准备驱动，节点存在却没有绑定/notifier，是高可信原因，因果仍待受控实测。独立两项配置片段及实际ARM模块已离线验证，严格链接/24项原导出、版本和别名通过；前两次失败保留，第三次新输出成功，未替换原源码或参考产物。最终同boot/原卡数据关闭/无gsm配置/PCM关闭复核通过；未加载候选、关机或刷写，不能宣称修复验收完成。详情、制品和下一项候选维护/关机实测见 [P6X](VALIDATION-POWEROFF-DIAGNOSIS-P6X.md) 与HANDOFF顶部。

**追加小任务P7F已完成，整体仍76.5/100。** GitHub自动检查原来仅运行网页灯控，现补齐身份/首次owner、音频生命周期、一键通话和网络/月策略；五组本地通过，a679da8的完整CI39723798698已success，原Rust/生产检查/Clippy及系统工具均通过。无设备访问、固件部署或流量使用，不重复增加功能验收点。命令、实际边界及接手证据见 [P7F](VALIDATION-WEB-CI-P7F.md) 和HANDOFF顶部；下一项R02/poweroff诊断仍待安排。

**R11移动数据与后台网络模块已完成本轮验收，整体76.5/100。** 用户授权换移动卡、测试和持久安装网络监测/守卫后，真实接口绑定的DNS/严格HTTPS、1MiB下载样本约2.16Mbps、按SIM月账本及256字节剩余额度耗尽阻断/断开普通gsm通过；NM进程在监测重启中保持。网页新增IP/DNS/APN、无线/小区信息、实时速度、月统计、限额开关/额度/重置日及显式授权主动测试。232项Rust、8个Rust ARM构建、Native21项、最终Root6项和网页/系统回归通过。业务交付383/epoch13，Root网络a239，Root灯控仍0c；未刷新整机镜像或初始化正式业务。P4加1点，见 [P4B](VALIDATION-CELLULAR-NETWORK-P4B.md)。

用户已换回原联通卡；最终只读复核home注册、普通数据关闭、没有gsm自动连接profile、原卡账本0，移动卡1,152,128字节单独保存。正常poweroff曾返回新boot，Root守卫/LED及账本复核通过，但关机自动回启动仍是待诊断问题，不算成功停机或完整业务冷启动。正式owner/业务仍未初始化，无新电话/SMS，累计10010仍38；临时测试/构建均已收尾。下一项R02及关机诊断，未自动开始；下文为历史。

**追加单项R01-C已完成，整体75.5/100。** 13份旧文件备份/回读后，在已安装新OS中原子交付2b/epoch7签名factory、新六程序slot及6个必要配套文件，签名公钥保持；Root灯控更新0c。设备8程序来源、Core自检、UID984空cap/私有设备和网络的合成Opus500帧及DTLS各50帧、灯控GPIO采样、初次设置门禁跳过/不创建台账通过。索引 `.private/r01c-delivery-20261022-first.json`、`r01c-final-gate-20261022-first.json` passed=true，Root私有收据与备份保存；见 [P6W](VALIDATION-DEVICE-DELIVERY-P6W.md)。P6增加0.5实际交付点，未刷整张P6V镜像、未重启/初始化/启动正式业务、无新通话/短信/普通蜂窝；下一项首次所有者/端点/媒体初始化和真实业务/冷启动验收。完成本单项后不继续，下文为历史。

**追加单项R01-B已完成，整体75/100。** 修正镜像审计拒绝新灯控配置的实际缺陷，36项rootfs回归、重建灯控9项通过。0c组成/Root灯控+2b同源签名业务生成3436183552字节通用rootfs；8个ARM程序、18单元静态依赖、候选内合成音频/DTLS、只读FS和12373条目内容/权限/所有者/capabilities核对通过。原基底、源码和制品保持，镜像导出Windows并复核SHA256。索引 `.private/r01b-system-composition-20261022-first.json`、制品 `.local/r01b-system-image-0c817c1-first/`；见 [P6V](VALIDATION-CORRECTED-SYSTEM-P6V.md)。P6增加0.5离线镜像进展，门禁仍false，无设备连接/交付/初始化或新通话/短信/蜂窝；下一项才是交付与实机验收，完成本单项后不继续。下文为历史。

**追加单项R01-A已完成，整体74.5/100。** 精确2b源码的同源Native构建/21项测试、七Rust ARM非Root来源/自检、原开发签名身份epoch7/8/9包，以及实际ARM更新器8拒绝路径、未确认回退、正确确认及合成Opus/DTLS验收通过。补齐LED组件元数据，所有输入摘要保持，私钥留PC；成功/失败日志和索引已保存，临时构建容器已清理。P6增加0.5点候选准备，系统组成/部署仍待做，真实业务与冷启动未验收。本轮无棒子连接、初始化或新电话/短信/蜂窝，完成单项后不进入下一项；见 [P6U](VALIDATION-SIGNED-CANDIDATE-P6U.md)、HANDOFF顶部和 `.private/r01-candidate-preparation-20261021-first.json`。下方74点及Native待构建为此前历史。

文档与同步补充：用户要求同步后，正常git push已把11f80da之后的本地代码推送到GitHub（首轮到08f29f4），未强制覆盖历史。新增 [详细剩余任务与Claude接手清单](REMAINING-WORK-AND-CLAUDE-HANDOFF.md)，按R01—R12列出入口、依赖、验收标准与离线/实机分工。此前API发布失败保留为历史，不再作为当前阻塞。本次无固件改动或设备操作，整体仍74%，继续暂停。

**灯控任务已收尾并按要求暂停，整体74/100。** 精确2b3c49c的七生产ARM程序交叉编译成功；本轮只把最终灯控程序更新到棒子，旧程序备份/新旧摘要/非Root build-info/原子发布/fsync及白色脉冲复核通过，服务enabled/active、无capability、无重启。其余业务接线程序仅为已保存制品，未交付启用，实际业务仍4e6且未初始化；真实未读/网页/升级联动不宣称已验收。准确源码、私有部署/构建索引及下一步见HANDOFF顶部。无活跃安装/构建任务，无需用户动作，累计10010仍38。

**本轮灯控收尾后按用户要求暂停，整体仍74/100。** 独立Rust灯控已部署启用，修复完全不亮；用户实物确认白色慢速亮灭正常。完整灯语、低频青/紫短闪、冷暖两组0.9/1.8秒未读动画，以及配置/升级优先级已实现。Core配置租约、网页退出与显式未读确认、Root升级事件源码接线通过222项Rust、生产检查/format/Clippy与网页生命周期测试；系统工具139通过/1项缺make跳过，补跑8项含该项通过。当前业务仍4e6未初始化，接线业务程序尚未上机，真实未读/网页/升级/蜂窝联动待后续业务启用实机验收；不把代码和策略测试算成完成验收。最终灯控制品/部署及接续入口见HANDOFF顶部与VALIDATION-STATUS-LED-P7E.md，下文暂停记录为历史。

**追加授权的修正版部署已完成，保存后暂停，整体仍74/100。** 提交7d307ce的音频守卫与app-recover条件作为第三系统的两个维护补丁落盘，原件私有备份、新旧摘要/权限/原子替换及完整回读通过。音频服务active/success、16项固定身份匹配、能力/隔离保持；未初始化恢复单元正确跳过。部署后只读硬件/系统健康及时间复核通过，基带home注册、无通话、PCM关闭、普通蜂窝未连接。没有重启、业务初始化或新通话/短信。当前启动内的修正复核不代替冷启动/长期稳定和业务验收；TLS修复应用仍未交付，离线候选仍须纳入音频修正重新组成。准确运行状态为bbb1081+两个维护补丁，记录third-runtime-audio-fix-20261021-first.json，不能按原rootfs摘要宣称现场未改。接续见HANDOFF顶部；下条“尚未部署”是追加授权前的历史。

**按用户要求，本轮收尾后暂停；整体仍74/100。** 第三次新系统安装已退出0，两个分区完整回读、保护范围/GPT/boot、两FS和reset通过；新USB及固定公钥维护SSH登录通过。首启健康尚未通过：实机16项音频模块身份匹配，但守卫错误要求24位srcversion，固定来源实际为23位。源码现严格要求23位并保留完整身份/加载限制，真实profile及拒绝路径测试已补齐，完整140项系统工具回归通过（7.024秒、无跳过）。第一次其他隔离夹具失败保留，夹具已显式核验自身loop的udev标签；生产状态守卫未改。本次音频修正及此前app-recover修正尚未部署。TLS修复候选仍含旧音频守卫，须重新组成后才能安装；实际业务保持未初始化。无活跃安装器/monitor，不重跑一次性连接或校时helper，无新通话/短信/普通蜂窝。下一步及私有证据入口见HANDOFF顶部。下文保留历史检查点，不代表当前仍在安装或SSH拒绝。

依赖复核：4e6原workspace锁文件命中5条RustSec TLS公告；仅更新rustls 0.23.46/webpki 0.103.16后，同一数据库下workspace及Native均0漏洞，209项Rust/生产检查/Clippy通过。仍有5项维护警告，未忽略。3da4679完整CI、六ARM及隔离运行、同源Native21项/构建、复用已选密钥签名，以及新通用树/完整镜像/七ARM非Root/静态启动依赖均通过，新制品尚未部署；正在安装的bbb1081仍含旧程序，先完成维护/音频诊断，业务初始化暂缓至修复产物交付。详见VALIDATION-DEPENDENCIES-P7D.md；仍74/100。

补充：日志证实的未初始化app-recover问题已在源码修正，真实systemd夹具复现前失败/后跳过，并验证初始化后缺目录仍拒绝、一次恢复预算与停组保持；本机140项系统工具回归通过。此改动未纳入正在安装的bbb1081冻结镜像，实机尚待维护入口打通后验证，进度不加点。

用户已醒来并完成一次按键拔插，独立second只读完整捕获/GPT/boot/原图保持通过，探针退出0，不写分区或reset。原图两FS只读4保留；新状态副本preen1→只读0，旧101项/维护身份一致、业务仍未初始化。日志证实SSH准备/音频检查/app-recover运行目录失败。第三独立修正版计划的准备/两FS/离线验证全部通过；已在同一9008会话启动一次固定p14+p15安装，最终写入/回读/reset尚待完成，不重复启动、不要求再次拔插。自动审批已恢复，8项内核夹具/139项系统回归、181.333秒整合构建均通过，两种内核格式与681模块匹配；f4bc831完整CI39541470946成功。整体仍74/100，10010仍38；实际新系统维护、健康和业务尚未验收。现场唯一索引和接续见HANDOFF顶部，下文审批阻断与用户休息描述为历史。

最新P6T：固定93a71ee基础内核Image及Image.gz、681个原装模块打包后均已逐字节复现，完整源码/配置保持。仓库工具前版也实构通过；当前整合模块验证的版本新增第8项夹具后，WSL测试因自动审批服务额度错误未能执行，保持待测WIP，不把旧CI或前版证明覆盖给当前代码，不绕过审批。见[P6T](VALIDATION-FULL-KERNEL-P6T.md)与HANDOFF。本轮没有访问棒子或改变NAS候选；完整首启/外部initramfs/实机恢复仍缺，整体74/100，10010仍38。

本轮收尾：修正版已在NAS新Root私有目录独立哈希冻结，后续唯一准备索引 `next-new-os-candidate-20261021-third.json`，不含已选第三状态/安装计划，未开始硬件动作。c5ee339准确固定候选的完整CI39505200290成功，bbb1081修正CI39504723559成功；131板级、34rootfs、7恢复、Rust/production/Clippy通过。新系统USB正常但SSH维护仍不可用；待用户醒来配合物理进入后先取第二启动日志，再定独立方案。用户休息期间不请求操作，保持连接即可。进度74/100，累计10010仍38，无新短信/普通蜂窝。

维护写入点修正源bbb1081已组成新通用候选：12367项、3436183552字节，七ARM非Root/静态依赖/镜像逐项与只读FS检查通过；准确摘要及私有索引见HANDOFF，尚未上机。安装器固定到新摘要，不放宽默认门禁或改动第二次冻结工具。当前新系统保持正常USB但维护SSH不可用；之后需要用户醒来配合新的恢复入口，先取得现场日志与状态，不重跑旧尝试。总74/100。

第二次修正安装完整写后回读/保护范围/GPT/boot、两FS只读0及正常reset ACK均通过，安装退出0。新USB接入通过，维护SSH有界等待仍拒绝，尚未健康/业务验收，仍74/100。无活跃硬件写入器，用户休息，保持现场待之后配合；详情见HANDOFF顶部。本机又复现父挂载被ProtectSystem设只读而守卫仍要求rw的组合缺陷：SSH专用模式核实际同设备可写system子树，其余默认仍需rw；真实SSH/挂载/设备/空capability组合及131板级测试通过，新候选尚未安装。音频守卫补阶段诊断，不放宽加载核验。本机完整内核已独立构建两次，681模块、15468导出表与来源保持通过，但相同配置/横幅仍有字节差异，未上机，见[内核构建P6T](VALIDATION-FULL-KERNEL-P6T.md)。

本轮修正接续：完整只读启动现场保存及GPT/boot核验通过，取得SSH依赖失败/音频检查失败日志；原图两FS只读4保留。第三份状态副本强制preen1→只读0，确认旧101项完全相同、业务目录空、维护材料保持且原捕获未改，前两次副本失败保留。新候选7d1860a已重建/逐项核对并独立传NAS。a9cf521的精确未干净现场替换路径130板级/34rootfs/7恢复及完整CI39493697545通过；新第二安装副本正在离线生成/默认验证，不开USB。设备保持9008，继续只用second索引观察，待默认验证通过后同一物理会话单次修正；未正常启动、未初始化业务，不重复加分，仍74/100。

最新 P6S：首次新系统 p14+p15 完整写入/明确最终 ACK，3909091328 字节写后捕获、目标与当前保护范围及前后两 FS 检查通过，原安装退出0/reset明确ACK。新USB身份与NAS无默认路由变化接入通过，SSH持续拒绝，未验收首启健康或初始化业务。已修正维护服务隔离与物理 `/dev` 检查冲突，新增真实ext4隐藏设备及拒绝测试通过；正保存启动后完整只读现场，后续冻结新候选，原报告/工具不改写。详见 [P6S](VALIDATION-FIRST-OS-P6S.md) 和 HANDOFF 顶部。整体仍74/100，10010仍38，无新短信/普通蜂窝。下文暂停/尚未写入为历史状态。

**本轮已完成并按用户要求暂停，整体74/100，剩约26%。** 9947a0f固定恢复执行器的联调/失败路径、本机90板级/34rootfs/7恢复测试、完整CI39109738551与NAS真实备份/原计划默认validate全部通过；未开USB、未reset、未消费尝试或写分区。源码/实际报告/下一步见VALIDATION-RESTORE-EXECUTOR-P6P.md和HANDOFF.md顶部，私有索引restore-executor-validation-20261018-first.json。没有活跃任务或需用户操作的窗口，收到新继续指令再推进。维护落盘、实际恢复、完整新系统冷启动和部分通话验收仍待完成，流量等换卡；不为离线执行器重复加点。下文保留旧检查点。

本轮接续P6O：真实干净备份已由7b9d40f生成固定rootfs/NSHSTATE恢复计划；acd6b08加入严格ACK与无重试USB协议基础，完整83板级/34rootfs/7恢复及CI39099022089通过。维护客户端钥匙仅本机生成/核对，NAS USB路由只读检查通过；完整执行器、实际公钥落盘、主机身份、新接口接入与冷启动仍未完成。未再次进入9008或写分区/拨号/短信/普通蜂窝，整体仍74/100。详见VALIDATION-RESTORE-PLAN-P6O.md和HANDOFF.md顶部，不能把计划/协议夹具当成已回刷。

整体74/100，约74%；P0=7、P1=14.5、P2=15、P3=14.5、P4=7.5、P5=5、P6=6.5、P7=4。P0C新原始3909091328字节离线捕获独立回读、两文件系统只读e2fsck均0、整图/GPT/boot保持及正常返回通过，增加P0半点。旧不一致原图/派生副本保留，仍未完整回刷或启动新OS。实际NSHSTATE守卫和所有者固件包只读核验通过，旧目录权限/身份/服务保持；未创建Rust状态命名空间或安装真实维护钥匙。只读备份工具/失败路径测试与同步夹具隔离已保存7091d0a，GitHub完整CI39097766897通过68板级/34rootfs/7恢复工具/209 Rust、production检查及Clippy。累计10010仍38，无新短信/普通蜂窝/原始分区写入。见VALIDATION-EDL-CLEAN-P0C.md、PROGRESS-SCALE.md及HANDOFF.md顶部；以下为保留的历史检查点，不代表当前仍在等用户拔插。

2026-10-17补充：598f273完整CI通过（https://github.com/Viper-Boss/RustSimHub/actions/runs/38996792278），包含Rust测试/production检查/Clippy与板级夹具。随后对固定orig源码tar流式计算86859个文件/符号链接的Git blob、mode和目录树，1479589504字节，根tree精确为72d4e8e42649f763dd969a1de893051ed64387a1，与93a71ee提交一致；此前只核68个qcom文件的范围已补到完整源码。没有解包或执行源码。私有complete-source-tree-hash.json及.local/hash-complete-kernel-source-tree.py保留；这证明完整来源包与固定Git提交一致，不声称独立重编译了整个内核。 整体73.5/100约74%，10010仍38。代码在598f273后未变，后续仅此进度/来源文档。

2026-10-17 P6M：组合源075b68e、应用/Native仍4e6fea6；22个逐字节重建音频驱动已纳入全新候选。12367项/SHA c621fdbdedee685d0f6a930a949d36f2cb6dea29953d8240b4dda7bb17b95376；host及候选自身ARM modprobe的22项依赖图/四个别名一致，后者在只读/断网/无CAP_SYS_MODULE条件下运行，没有加载模块。七ARM非Root实际执行、包含音频驱动服务的完整systemd静态依赖及树保持通过；55板级/34rootfs回归通过。原initramfs为zstd，551项/100模块，无qcom音频；固定上游boot+重建DTB+root=LABEL=rootfs+补零64MiB完整复现备份SHA d5790e07…。公开核验器只写JSON不写启动镜像，实际报告passed=true；初次只替换DTB对照失败保留。新驱动守卫仅隔离夹具/静态验证，未在棒子加载或启动新OS。旧P6L树、原失败记录及所有旧制品保留，10010仍38，无新短信/蜂窝/分区写入，整体73.5/100约74%。 详见VALIDATION-KERNEL-AUDIO-P6M.md及HANDOFF.md。

2026-10-17 最新权威更新：2026-10-17，4e6fea6/a19真实Chrome一键听音：用户明确“现在所有的声音都听完整了”，并反馈“测试已结束，电话已挂断；观察接通后接音0.7秒”。Native约24.26秒传声，1212captured/sent/played、1220received、19dropped、1次可恢复xrun；answer1493ms/verified40ms/activation141ms。停机后只读一致数据库确认本次dial/hangup均submitted且同一精确call ended/observed；MM全部terminated、PCMclosed、Voice撤回/拒绝、旧四服务active、预览/独占守卫停止、Windows临时证书直接移除并核验。私有browser-a19-acceptance.json与browser-preview-4e6fea6-a19-native.log保留，累计10010=38。0.7秒是页面观察ACTIVE至媒体启用，不是运营商接通至扬声器首声的端到端测量；尚未验收双向听音。无新短信/普通蜂窝测试/分区写入。

22个额外音频模块已从固定KyonLi 93a71ee源码/headers与AirSimHub ea019485补丁重建，22/22逐字节等同设备现用文件。此前归档的25个qcom模块均为上游原装，不能代替这22个override；16个已加载模块的srcversion与只读捕获一致。本机私有Ubuntu GCC14.2.0-4ubuntu2~24.04.1交叉编译，补丁零fuzz；第二版19/22全字节相同，三差异仅__FILE__路径及build-id，第三版用-fmacro-prefix-map对齐原诊断路径后22/22相同。源码/headers输入未变、未安装或加载新模块。当前P6L通用树尚缺22个override，不能据旧系统通话通过宣布新系统声音可用；下一步在新候选纳入驱动并验证depmod/别名/预加载及冷启动。完整内核/initramfs独立重建、boot重打包和恢复仍缺，board.toml保持UNRESOLVED，所有刷写门禁false，不重复加分。 整体仍73.5/100约74%；持续下行已计分，本次完善同一功能验收不重复计点。详见HANDOFF.md、media-preconnect.md和KERNEL-PROVENANCE.md。以下为历史检查点。

2026-10-17 系统/来源推进：P6L当前4e6签名应用/同源Native已纳入12340项完整通用候选，SHA4324468e63920ce79bef8cd0da1a0a2e4bc7c201f6ea866a9da4641e5da8701e。修复严格umask导致新程序父目录0700的真实错误，七ARM实际非Root执行/全服务静态依赖/树保持通过。再取得固定上游内核源码/发布包/构建记录并核SHA，备份boot的压缩内核和ramdisk与上游逐字节一致；上游DTB加固定AirSimHub源码重建与备份DTB逐字节一致。源码/许可及有限重建工具已入仓库，44项板级严格umask回归与实际重建通过。额外音频模块、完整boot来源和真实首启/恢复仍需闭环，board.toml保留UNRESOLVED，所有刷写门禁false，不新增通话/短信/蜂窝/分区写入，不重复计点，整体73.5/100约74%。见VALIDATION-SYSTEM-TREE-P6L.md及KERNEL-PROVENANCE.md。

2026-10-17 权威更新：整体73.5/100约74%，10010累计37。316005e无拨号真实DTLS保持12秒/零PCM及a16接通后1187ms开PCM通过；a17真实Chrome显示接音0.5秒、设备约26.6秒传声。页面末尾未确认挂断，后台该次hangup submitted、call ended/observed和清理已核实。4e6fea6收尾改为27秒一次挂断、最多3秒/6次只读确认；209 workspace/六ARM、Native21及Node三组通过。a18冻结同一JS在Node经真实严格HTTPS单次10010，一次dial/一次hangup、未续租、精确ended/observed与终态“电话已挂断”通过，28.654秒；该用例不测试音频，不能把stub的0.0秒当声音指标。全部清理/旧四服务/PCM/Voice和临时证书移除核验通过。4e6同源正式服务组首次owner/严格TLS、Media硬退出整组回退、第三版确认保持、第二次Edge故障预算耗尽停组及清理通过；不重复加分，首句完整和双向听音仍待人工，流量等换卡，完整OS/恢复门禁保持。详见HANDOFF.md和media-preconnect.md。

2026-10-16 当前：整体仍 73/100，10010 累计 26；实机捕获空音频负载触发 Codec 致命退出。新增空负载安全丢弃与一键 10010/自动连接声音/约30秒结束页面源码，202 workspace、21 Native、Clippy 和 Node 三组通过；新 ARM 与真实听音待验收，不加分。见 VALIDATION-BROWSER-MEDIA-P3I.md。

2026-10-16最新：71506f0冻结ARM/Native通过，正式Media可信安装和签名Core/Media/Edge组、实际Media SIGKILL自动整组回退和第二次Edge故障预算耗尽全停在候选根实机通过；P6+0.5，总体73/100。新增一次浏览器10010，累计25，4—5秒后明确Codec错误退出，P3不加分；细分拒绝与乱序/重复包处理正在验证。见VALIDATION-MEDIA-STARTUP-P6K.md。电话/PCM/权限/预览服务已清理；连续诊断的同一临时证书仍需结束后实际移除，当前状态以HANDOFF及私有trust记录为准。流量等换卡，完整系统上电/恢复门禁保持。

2026-10-16交互历史：实际Chrome配对/登录/防重和真实下行一句已发生，但媒体打开约3秒后提前退出，30秒后保护才挂断，持续声音未通过。新增实际10010两次，累计24，中间一轮只等待未拨号。预览/Voice权限/PCM及临时Windows证书已清理；见VALIDATION-BROWSER-MEDIA-P3H.md。新增Media正式启动/可信初装源码和有限退出分类诊断，本机202 workspace/20 Native与Clippy、39板级/Node通过；新ARM与服务组实机待验收。仍72.5/100约73%，不将部分听音或源码当成故障已解决。

2026-10-16：严格HTTPS两阶段媒体在USB/Wi-Fi各一次10010通过，真实DTLS/Opus/PCM到NAS解码159/147帧；重复激活防重、Edge正常退出后Core仍活而PCM关闭、相同身份/证书名/Origin/owner token跨两个真实监听地址通过。四份私有报告passed/cleanup_verified，旧四服务active、电话空闲、PCM关闭、权限撤回。累计10010=22，新SMS/蜂窝/分区写入0；保留可恢复欠载/少量丢帧。874f01c六程序ARM/Native ARM、本机201/20与Clippy/Node通过。见VALIDATION-WEB-MEDIA-P3G.md；P3+0.5、P4+0.5，总体72.5/100约73%。浏览器权限/麦克风/听音、产品服务组硬退出联动及完整系统/恢复仍缺；换卡流量延后。

2026-10-16历史：可信首次安装、持久TLS及设置码/身份的Root私有交付、未初始化不启动和重复安装拒绝在棒子候选用户态实机通过；真实崩溃整组回退回归及清理通过。新组合12338项/树3e3f51ce…、离线启动依赖与树保持核验通过，191项Rust/38板级/六程序ARM/CI通过。见VALIDATION-INSTALLATION-P6J.md。P6加0.5至6，总体70.5/100约71%。尚未实际NSHSTATE初始化、新系统上电或刷写，恢复/维护/媒体仍缺；拨号/SMS/蜂窝新增0。

2026-10-15：统一签名开机组在棒子候选用户态通过实际生产单元验收：完整Core/Edge、首次owner/TLS、跨FS台账、真正Core SIGKILL自动整组回退、第三版显式健康确认/重启及第二次故障停止；清理及归档内容/12303项路径清单通过，旧四服务active/PCMclosed。新签名系统树12335项已审计，187项Rust/35板级/34rootfs通过。见VALIDATION-BOOT-GROUP-P6I.md。P6加1至5.5，整体70/100；未完整上电/刷机，配置/维护交付、恢复及媒体等仍待完成，拨号/SMS/蜂窝新增0。

2026-10-15最新：9b5f1d9本地准备设置码/UID核验公钥与窗口实机通过，185项/六程序ARM生产/Clippy/CI38752335688通过，见VALIDATION-LOCAL-SETUP-P6H.md。SSH持久host key与公钥策略源码、30项板级测试、候选ARM ssh-keygen/sshd -t/-T合成检查通过；未装真实维护公钥或启动新SSH，见persistent-ssh-maintenance.md。完整开机组/可信配对交付/媒体/恢复仍需推进，69/100不额外加点，无新通话/SMS/蜂窝。

2026-10-15补充：a10a492冻结源1dbc6775...本机WSL181项、六程序ARM生产构建、CI38749275454成功；新签名factory首装/候选回退/floor保持及丢失台账拒绝和next恢复在候选RootDirectory通过，见factory-application-baseline.md。不额外计点，仍69/100，正式启动/首次配对和完整恢复待做。

最新（2026-10-15）：候选用户态实机Core/Edge归属、首次设置/真实SIGKILL、短信/Wi-Fi及运行后全树比较通过；同源Native构建和Media984合成Opus/DTLS/SRTP通过，见VALIDATION-CANDIDATE-RUNTIME-P6G.md。P6加0.5至4.5，总体69/100。无新电话/SMS/蜂窝，流量等待换卡。签名factory首装和丢失台账拒绝组件9项测试及clippy通过，尚待冻结构建/实机验证。正式首启/完整启动组/媒体/恢复仍待完成。用户可协助拔插，目前无需操作。下面为历史记录。

最新（2026-10-15）：用户目标100%，实际仍68.5点、约69%。724fc09内部板级候选树组合/通用审计、23项隔离板级测试、离线启动依赖及factory实际路径六程序只读运行通过；真实NSHSTATE守卫接受、空目录/只读别名拒绝，旧身份/挂载owner/四服务/PCM保持。NM配置已接入持久目录，详见VALIDATION-SYSTEM-TREE-P6F.md；不把候选层和重复程序运行额外计点。应用自启组、可信正式首启/维护、媒体及完整恢复仍待完成，不出刷机包，不新增拨号/短信/蜂窝测试，流量等待换卡。此前ca596a7首个owner同进程转业务及崩溃窗口边界实机已通过，见P6E。下面为历史记录。

最新实证：非 Root Wi-Fi 试连接、真实错误口令/无 IP 临时 profile 和核心 SIGKILL 后 NM 自动恢复通过；USB/WWAN 无线保留，临时 profile/权限规则已清理，不明操作不重试。见 VALIDATION-WIFI-TRIAL-P4A.md。整体约 56%，继续至 75%，蜂窝流量最后测。

最新实证：Core 986 和 Media 984 的独立非 Root 电话加密音频通过，空能力集合；实际最小 Voice 权限与其他控制拒绝、规则移除后恢复拒绝通过。累计 15 次 10010 拨号。见 VALIDATION-ISOLATED-MEDIA-P7A.md。整体约 56%，75% 长任务继续。

最新实证：签名应用槽位在棒子验证通过，生产 core 的实际候选进程 SIGKILL 后选择器回到 factory，身份和短信内容一致，低版本/换钥/签名及 payload 篡改拒绝。见 VALIDATION-APP-UPDATE-P6A.md。整体约 56%，继续推进 75%；普通蜂窝流量最后验收。

最新实证：10010 真实电话下行经 Opus + DTLS/SRTP 到 NAS 解码成功，150 帧；静音上行、自动挂断、PCM 全关闭、旧四服务 active。见 VALIDATION-RTC-CARRIER-P3E.md。整体约 56%，75% 长任务继续，流量最后测。

# 开发进度

2026-10-21当前检查点：8d2fd5a完整CI39479577805成功，NAS匹配完整新OS副本生成、保护范围/GPT/boot保持、独立两文件系统检查及默认validate实际退出0通过（未开USB/写入/reset/消费安装标记）。新维护USB配置重核inactive；已按用户直接测试新系统的授权准备唯一新安装watcher并正常halt一次，待60秒核验后给本轮物理步骤。索引`install-first-boot-20261021-first.json`，不要重跑prepare/arm/halt，不执行旧恢复。新OS尚未写入/启动，仍74/100；累计10010仍38，无新增短信或普通蜂窝测试。

2026-10-21最新方向：用户要求跳过旧系统回刷演练、直接测试新OS并持续自主推进；按[首次整机实验](experimental-first-boot.md)实施。新固定候选/安装入口122板级/34rootfs/7恢复回归通过，尚未生成NAS匹配整图、未安装或启动新OS，仍74/100。710c73b完整CI和NAS无USB显式接续验证已通过，原restore失败标记保留但不再执行该旧系统接续。当前旧OS已正常返回，无新短信/10010/普通蜂窝测试。

2026-10-21当前结果覆盖：授权的一次旧系统回刷在configure阶段明确NAK（32KB请求，硬件支持16KB），未发送program或raw数据，原完整前快照及两FS检查通过。单独记录只读返回并独立验证旧boot/身份/四服务/PCM/空闲/状态目录保持，原交易仍失败。16KB修正及明确同plan接续门禁116板级/34rootfs/7恢复通过，尚未再次实机写入；原attempt保留，实际重试需新授权。整体仍74/100，当前无活跃用户窗口，新OS未安装；通用rootfs已在NAS独立复核保存。见[P6R](VALIDATION-RESTORE-CONFIG-P6R.md)，下文运行中状态为历史。

2026-10-21最新覆盖：用户解除旧暂停并明确授权一次旧系统p14+p15实际回刷，已配合本轮物理进入。614d95c完整CI39473392006成功，来源与保留两份完整捕获的空间预检通过，同一唯一运行现正在读取完整执行前快照；没有实际恢复成功结论，新OS未安装，仍74/100。此前短GPT/boot只读复核及明确reset ACK/正常返回已通过，真实所选维护身份在USB候选服务的正确/负向认证和清理通过，NAS新MAC配置实际登记inactive，离线NSHSTATE候选两次e2fsck、101旧项保持、身份保持和源图不变通过。见[P6Q](VALIDATION-MAINTENANCE-RECOVERY-P6Q.md)及HANDOFF顶部；不重复加点，累计10010仍38，无新增短信/普通蜂窝。当前恢复一旦失败不得自动reset或重跑，先检查准确阶段；完整结果必须从`restore-run-20261021-first.json`接续。

2026-10-18权威补充：f270完整源码核验器实际核对86859项Git树，board来源现固定93a71ee；P6N新内部rootfs.img的e2fsck和逐项只读挂载核对通过，62板级/34rootfs测试通过。用户已在NAS旁配合新的只读离线备份，探针与私有20261018尝试记录就绪；poweroff自动重启后改为一次正常halt，后续捕获与返回尚待实测。未刷写/拨号/短信/蜂窝；所有发布/恢复/冷启动门禁保持false。整体73.5/100约74%，见VALIDATION-SYSTEM-IMAGE-P6N.md与HANDOFF.md顶部覆盖记录。

2026-10-16源码推进：网页两阶段媒体适配、受信任连接地址/owner绑定、单次票据内部化、Native接管清理与多bind白名单、Edge多私有监听及浏览器麦克风/播放/取消实现。本机201 workspace/20 Native/Clippy及Node媒体生命周期和proof通过；ARM/HTTP实机/真实浏览器仍待验证，不加进度点，仍71.5/100约72%。见web-media.md。没有新增电话、短信、蜂窝或分区写入。

2026-10-16最新：e271ca1/621994eb…的生产Core与常驻Media、受限IPC在候选根中两轮真实10010授权媒体集成通过：123/214帧捕获、NAS123/212帧解码，主动停止/挂断和全部隔离清理通过；分别1/2次欠载成功恢复，不能称长期稳定。194 workspace/19 Native/Clippy及同源六程序ARM与Native ARM通过。P3加0.5至13.5，总体71.5/100约72%，见VALIDATION-MEDIA-WORKER-P3F.md。累计10010=20，短信/普通蜂窝新增0；网页信令、真实双向听音、正式媒体服务与完整刷写仍待完成。

2026-10-16新增：实际9008进入、主备GPT和完整3909091328字节主区读取、一次自动返回及旧身份/服务/PCM保持通过，见VALIDATION-EDL-READ-P0B.md。原始捕获文件系统检查退出4，原图保留；第三份独立副本journal+安全元数据preen后两分区只读检查0、GPT/boot及原图摘要保持。P0加0.5，总体71/100约71%。未完整回刷、未解除内核来源或刷写门禁。

常驻媒体：233110a、173b21f同源ARM构建及真实负向DTLS/权限预检通过。一次10010实际生产Core/票据/PCM与NAS解码已成功到达，但36帧后连续4次欠载退出，未算稳定媒体通过；通话/PCM/临时权限清理通过，累计10010为16。7077565已修正PCM故障误报、降低实机轮询负载，本机194 workspace/19 Native及Clippy通过，六程序ARM构建通过、Native ARM构建进行中；待实际复测，不提前加P3分。流量测试仍等换卡。

2026-10-15：182a6ec完整Core组合与cd4c96e无旧NAS数据初始化两轮实机passed/cleanup_verified=true；短信/历史/NAS在配网期间可读，拨号准备拒绝，模拟真实电话端证明旧prepared意图未消费且挂断可执行；真实SIGKILL后原Wi-Fi恢复、业务去重/NAS映射保留，明确保存后自动重连通过。空历史首次安装不需要NAS原库/示例快照或NAS principal。见VALIDATION-PLATFORM-P6C.md，集成不重复计点，仍66.5点约67%。未刷机/开机安装/物理断电，未新拨号/短信或蜂窝测试；目标80%继续。

2026-10-14：新有限Wi-Fi配置的实际独立profile连接、owner确认持久保存、Core退出后NM自动重连、错误PSK候选存在时真实SIGKILL恢复及跨重启防重通过；本轮连接既有SSID，不声明不同SSID/热点/未配置首启或完整Core组合已验收。见VALIDATION-WIFI-PROVISION-P4B.md；P4加1至7/12，总体66.5点、约67%。170项本机测试/lint、六个ARM64生产构建及CI38570475237通过。第二轮私有报告passed/cleanup_verified=true，旧profile/WWAN保留、自有配置/规则/服务已清理，旧四服务active、PCMclosed，无新通话/短信/蜂窝测试。继续目标80%。

2026-10-14：d20b9af实现有限NAS桥，530e6f5增加实际观察只读模式与联合Go验收；两轮私有报告passed/cleanup_verified=true。P5加1.5点至5/8，总体65.5点、约66%。源码/源归档/产物核验、本机163项测试、六个ARM64生产release及CI38565794201通过。530e6f5本次增量release34.24秒，不作为全量构建/等功能性能对照。自有单元停止，旧四服务active、PCMclosed，无新拨号/短信/蜂窝测试。下一步NAS页面/媒体整合、网络与完整系统剩余工作；目标80%继续。

2026-10-14：真实旧NAS通信库离线迁移通过：只读一致快照、重复导入无新增、独立逐字段/摘要一致、仅非认证历史归档到棒子并通过非Root只读检查。见VALIDATION-HISTORY-P5C.md。增加P5半点，整体64点、约64%；真实在线查询与NAS协议/PCM仍未完成，目标80%继续，蜂窝测试跳过。

2026-10-14：正式历史查询已接入组合Core/HTTPS，157项本机测试、ARM生产、CI及实际跨用户/来源/标记/tombstone/撤销与generation/重启/无业务重放通过。见VALIDATION-HISTORY-P5B.md；整体63.5点、约64%。真实旧库与NAS在线协议/PCM、浏览器显示仍待验收；流量继续跳过。

2026-10-14 本机交叉编译验收：稳定8f4d15b 的151项Linux测试、六个生产ARM64程序及NAS Debian13容器/棒子隔离非Root无硬件运行自检通过。默认本机WSL 8路编译，持久缓存；原生音频仍NAS构建。见VALIDATION-LOCAL-CROSS-P7C.md。此为构建流程优化，整体仍62.5点、约63%；历史查询WIP尚未接线/提交/验收。

最新约束（2026-10-14）：当前卡没有流量，普通蜂窝数据实测最后进行。只使用 USB/Wi-Fi 做网络/媒体验收；禁止普通 APN 激活、互联网蜂窝探测或隐式流量回退。只读状态与预算模拟可继续；不关闭语音使用的 WWAN/IMS。10010 持续授权仍有效。


新增 VALIDATION-RTC-P3D.md：棒子真实 UDP/ICE/DTLS/SRTP 合成回环、NAS 与棒子五秒双向编码静音通过。没有打开 PCM、拨号或新增短信。
native aff5dcd 已部署 /opt/rustsimhub/dev-rtc-p3/，self-check 双向 50 帧；LAN 各发 250、收 250/249，解码静音只有 1 LSB 量化残差。
最新源码补充证书替换失败、信令输出原子发布和一秒媒体断流关闭，尚须下一次原生构建验证。root 2c69ff9 的 125 项、默认 production core 构建通过，prod/development 分开；P6 实机签名应用验收仍未加点。
不要重跑旧电话 once；用户持续允许 10010，但新的电话测试要用独立标记且确保挂断。约 75% 长任务继续。


更新时间：2026-10-14 / 实际浏览器与 Wi-Fi 只读业务；整体约 **56%**（按 PROGRESS-SCALE.md 固定里程碑估算）。

实际浏览器经回环 QA 代理严格验证设备 Wi-Fi HTTPS，配对/批准/签名登录、短信正文与摘要、事件分页、只读发送准备拒绝通过。
复用隔离身份/授权重启后旧 token 拒绝，新登录通过；退出清空显示。
修正并实际验证在途只读响应延迟时能立即退出，旧响应不恢复界面；最新静态 release 已通过不同 UID 实机烟测。
Opus 固定 16k/20ms 适配器独立原生构建通过；棒子 500 帧合成往返成功，未打开 PCM，见 VALIDATION-CODEC-P3C.md。
见 VALIDATION-BROWSER-P2.md。约 75% 长任务仍活跃。以下为历史记录。

新增 Rust HTTPS 入口与静态网页，NetworkStatus 前后重新鉴权并核验唯一 NM owner；120 项 workspace、CI Clippy、ARM64 release 通过。
不同 core/edge UID 实机验证：严格 TLS、离线 owner 签名、只读 SIM/短信摘要/网络、scope 与撤销、错误 UID 和 Origin/role 注入拒绝。
外部 NAS 经 USB HTTPS、公钥/证书核对通过；自有临时单元已停止，旧四服务 active，PCM closed，无新短信/拨号/网络写入。
修正 Cargo 源包 mtime 造成旧 release 复用的问题；每个二进制自报被编译器跟踪的源包 SHA，部署再核对传输 hash。
只读配置使用 O_RDONLY 与实际 fd 的 UID/mode/nlink 检查；不通过可写挂载绕过安全边界。
网页浏览器交互、网页写业务、实际 Wi-Fi 入口/配网、蜂窝预算、WebRTC 与完整系统仍在后续工作中。
见 edge-and-network.md / VALIDATION-EDGE-P1.md。约 75% 的长任务仍活跃，不在本小检查点停止。以下为历史阶段。

core/media 授权 IPC 已接通；真实 Principal、scope、独占租约、ACTIVE、SIM/backend/path fence 在每次续验重新验证。
实机两秒和五秒 PCM 运行成功，五秒采集/播放各 246 帧，上行仅静音；启动一次 EPIPE 可恢复，未放宽欠载上限。
核心正常退出时实际授权丢失、PCM 关闭、自动挂断通过。11 次独立 10010 测试私有库均一个 dial / 一个 hangup，无新短信操作。
用户已持续授权本项目拨打 10010，无需再次逐次确认；其他号码或新短信不推定授权。
验证和产物见 VALIDATION-MEDIA-P3B.md / media-authorization.md。media_available 仍 false，WebRTC / 真实双向听音未完成。
HTTPS 调试入口、WebCrypto 客户端、网页和网络只读状态正在开发验证，未作为已完成硬件验收计分。
用户要求持续长任务推进至约 75%；该目标仍在进行，不能因本检查点保存而停止。下面为此前阶段记录。

新增 crates/media：固定 16kHz/20ms PCM、有界帧队列、分段 I/O、静音/断流/xrun 处理和媒体票据/撤销原语。
新增独立 native/audio-alsa：有限 UFI003 profile、两个 CS-Voice holder 及采集/播放句柄，safe Rust 调用 libasound。
102 项 workspace + 4 项原生 ALSA 测试通过，源码 CI Clippy 成功，NAS 两个 ARM64 release 构建成功。
非 root 棒子只读探测通过：profile 匹配、7/7 路由开启，PCM 未打开、mixer 未写入；旧服务正常。
core/media IPC 与真实授权还没接通，media_available 仍为 false；真实双向声音未验收。
已将用户要求的“每次结束报告整体百分比”写入 AGENTS.md，估算口径见 PROGRESS-SCALE.md。
实现与证据见 audio-foundation.md / VALIDATION-AUDIO-P3A.md。下面为此前通话阶段记录。

新增呼叫观察/显式控制模式、v5 持久记录与单次许可、独占租约、过期/撤销/退出清理及私有 CLI。
90 项测试、CI Clippy、NAS ARM64 默认检查/release 通过。
非 root 实机只读检查通过；独立 root 写模式仅拨打 10010 一次，观察 ACTIVE 后自动挂断，最终 state=7、active_calls=0。
私有 DB 核验只有一个 dial 和一个 hangup，均 Submitted/resolved，没有新 SMS 操作。临时服务已停止，旧四个服务及网络正常。
双向声音、真实来电/DTMF、生产非 root 写权限及完整固件尚未验收。
详见 call-control.md / VALIDATION-CALLS-P3.md；下面为此前短信阶段记录。

本轮新增显式 --serve-messaging、sms_send 操作权限、v4 持久单次许可和 CLI 私有草稿。
79 项测试、CI（含 Clippy）、NAS ARM64 默认检查/release 通过。
非 root 只读实机回归通过；独立 root 写模式只向 10010 发送一次 CXYE，已核验 SENT 并收到私有运营商回复。
一个持久操作及一次提交事件经私有备份只读核验。临时服务已停止，旧服务和网络正常。
实现见 sms-send.md；验证与当前产物见 VALIDATION-SMS-SEND-P2.md。
下面 73 项及此前收件箱结果为历史阶段记录。

本轮完成唯一 MM owner 信号订阅、连接重试、有界批次/逐对象隔离、路径 incarnation、执行前 SQLite 门禁和受鉴权保护的观察计数。
73 项测试及 CI Clippy 全部通过，私有 D-Bus 模拟覆盖坏对象、路径复用、SIM 信号和连接关闭。
实机订阅、已有 1 条短信读取、周期修复和撤销拒绝通过；测试单元已停止，旧服务/网络正常。空闲 RSS 样本约 6.27 MiB。
实现见 [observer-events.md](observer-events.md)，证据见 [VALIDATION-OBSERVER-P2.md](VALIDATION-OBSERVER-P2.md)。旧阶段记录保留。

## 已完成

- 新建本地 RustSimHub 目录、独立 Git 仓库及 Viper-Boss/RustSimHub 私有 GitHub 仓库。
- 详细开发计划已迁入 docs/development-plan.md；新增来源/许可证和接手约定。
- 通过 NAS 的 USB 链路及既有维护密钥连接棒子，完成只读硬件与服务基线。
- 新增 domain/wire/core Rust workspace：通话租约、SIM epoch、短信提交规则、有界协议、自检、Linux 只读 MM 探测及 UDS 服务。
- NAS 的 ARM64 官方 Rust 容器已通过测试和 release 构建；源码以允许列表打包，隔离使用数据盘，不挂载旧应用数据或主机 Docker socket。
- boot 与模块清单哈希已固定；64 MiB boot 备份已保存到本机与 NAS 的私有目录，哈希校验一致。
- ARM64 程序已在棒子运行并完成非 root D-Bus 及 UDS 烟测，拒绝非授权 UID；测试单元已停止。
- 新增 storage crate：单写者文件锁、32 容量请求队列、WAL/FULL、schema v1、持久幂等键、事务事件、SIM 隔离、Unknown 恢复、Backup API 与安全恢复标记。
- 默认 core 构建不提供存储 fixture；显式 dev-fixtures 开发构建在棒子独立目录完成合成数据验证。
- 新增 auth/policy crate、schema v2 身份/权限记录与 v1 迁移；签名挑战绑定双方公钥、purpose、scope、generation 与进程随机 instance。
- 新增离线 owner CLI 和 v2 鉴权 UDS；token 只保存摘要，重启失效，撤销持久生效；此前仅开放只读，当前增加显式短信写模式。
- 新增可信离线恢复、身份明文私有备份、独立客户端密钥/固定设备 pin/私有会话文件与 owner 管理 CLI。
- 鉴权 daemon 支持 SIGTERM/SIGINT 排空、唯一 writer 关闭、运行锁、同路径重启和受限 stale socket 接管。
- 增加显式只读观察 daemon、SIM 身份稳定门禁、原子换卡/outbox 隔离、完整入站短信和持久收件事件、32 条摘要分页及正文单独读取。
- 不同对象的同内容短信保留并标注疑似重复；同对象同内容重复读取返回原记录，换卡保留旧历史。
- 本轮增加独立持续监听任务、owner/连接失效代次、有界目录/路径提示与公平批次；同路径删除/复用或完整内容变化保留新记录。
- 单写者在状态/正文写入执行前检查观察 gate，监听器关闭或代次失效的排队任务拒绝。

## 验证状态

- Windows 格式化执行成功。Windows 原生编译未通过：MSVC link.exe 未安装。
- P0/P1 初始 ARM64 Linux：12 项测试通过（core 2、domain 7、wire 3）。覆盖断开/截断帧、超大帧、角色注入、租约过期与换卡、未知短信禁止重发。
- GitHub CI 已通过：Linux 测试、格式检查、Clippy（-D warnings）。首次运行：[38153648494](https://github.com/Viper-Boss/RustSimHub/actions/runs/38153648494)。
- P0/P1 初始 ARM64 release、自检、真实 D-Bus 与受限 UDS 实机验证均通过。该阶段程序约 1.88 MiB，只读服务 RSS 样本约 3.3 MiB。
- 完整证据与哈希：[VALIDATION-P0-P1.md](VALIDATION-P0-P1.md)。这些结果不能替代完整业务/性能验收。
- 身份阶段 47 项测试及实机记录见 VALIDATION-AUTH-P1.md；恢复/CLI 阶段 59 项通过（auth 19、core 8、ctl 1、domain 7、policy 1、storage 19、wire 4）。CI：[38265061208](https://github.com/Viper-Boss/RustSimHub/actions/runs/38265061208)。
- 恢复/CLI 阶段开发 core 约 4.15 MiB、simhubctl 约 0.75 MiB，完成真实离线 CLI 恢复、重新批准/撤销、SIGTERM/SIGKILL 重启；仍不等同于完整性能或固件验收。
- 板上旧服务仍承担日常业务；本轮新代码完成一次授权测试短信，本轮另完成一次授权拨号/挂断，未启用蜂窝数据或改写分区。
- 收件箱阶段源码 fdaf4c2：66 项通过（auth 21、core 10、ctl 1、domain 7、policy 1、storage 22、wire 4），CI：[38298747817](https://github.com/Viper-Boss/RustSimHub/actions/runs/38298747817)。NAS 默认 workspace 检查/release、自检及实机已有短信读取通过；该阶段 core 约 4.40 MiB，CLI 约 0.75 MiB。
- 事件观察阶段源码 31be30a：73 项（auth 21、core 16、ctl 1、domain 7、policy 1、storage 23、wire 4），CI [38316851912](https://github.com/Viper-Boss/RustSimHub/actions/runs/38316851912) 成功；NAS 默认检查/release、实机订阅与周期修复通过。该阶段 core 约 4.52 MiB，CLI 约 0.75 MiB。

## 阶段状态

P0 硬件/恢复基线进行中：系统和权限已检查，备份及恢复入口未闭环。
P1 基础工程进行中：纯规则、协议、持久化、公钥鉴权、恢复/客户端 CLI 与关闭重启已实现并验证；仍缺网页密码/TLS、最终用户设置 UI、完整开机部署和系统恢复。
P2 进行中：SIM 观察、短信接收/读取、订阅/重连、基础 Create/Send 与 Unknown 对账已实现并验证；历史归属/歧义、保留策略、长短信/投递报告、生产权限与更多硬件验收未完成。
P3 进行中：基本通话控制、独占租约和一次拨号/挂断实机通过；PCM/媒体授权原语与原生 ALSA 基础已验证，核心授权 IPC、真实声音、WebRTC、真实来电/DTMF与跨重启通话管理待验收。
P4–P7 未开始。当前不是可刷写完整固件。

## 下一步

1. 从实际呼叫控制器生成可信媒体授权快照，接通受限 core/media UDS、单次票据与持续失效通知；再做 CS-Voice/PCM 实机打开和双向声音验收，随后连接 WebRTC。
2. 补齐真实来电/拒接/DTMF、跨重启孤儿管理、历史状态/路径歧义、生产非 root 写权限及 systemd/TLS。
3. 短信历史/外部事件、保留策略、长短信/投递报告与编码/存储满硬件验收继续；Unknown 禁止自动重发。
4. 本轮一次电话及此前一次短信授权都已使用；不能删除 marker 或重放实机测试脚本。
5. 继续 P0 精确内核来源、完整私有备份/恢复入口，随后网络切换、加密远端接入、OTA 和系统镜像；完成恢复闭环前不刷 rootfs/boot。
