# RustSimHub 当前接手入口

**当前第二次修正安装正在离线生成/验证候选，尚未启动写入。** 原新OS完整现场已只读保存，GPT/boot保持，SSH失败及音频检查失败有持久日志。两原FS只读检查4原样保留；第三份状态副本强制preen退出1、随后只读0，原图保持、旧101项逐项摘要/元数据相同、业务目录空、所选维护身份保持。前两份只回放/未强制检查的失败副本也保留。源a9cf521完整CI39493697545成功，130板级/34rootfs/7恢复通过；新增固定完整现场摘要的显式未干净卸载替换，默认/旧恢复不放宽。新通用rootfs 7d1860a已重建核对并在NAS独立冻结。接续只看 `.private/install-first-boot-20261021-second.json` / `.local/status-install-second-boot-20261021.py`；候选默认validate通过后由已准备的 `.local/install-second-boot-20261021.py`在当前9008会话单次执行，不再halt/要求拔插，不运行第一次安装/连接器。原首次安装及只读诊断索引保持。当前设备留在9008，禁止手动reset；正常启动仍未验收，整体74/100。详情[P6S](VALIDATION-FIRST-OS-P6S.md)。

**最新 P6S：第一次新系统 p14+p15 安装已完整回读/当前保护范围/两 FS 检查通过，安装退出 0 且 reset 明确 ACK。新 USB 身份出现并完成无默认路由变化的 NAS 接入，但 SSH 持续拒绝，尚未通过首启健康。** 已发现 `PrivateDevices=yes` 与原状态守卫依赖可见物理 `/dev` 的确定冲突；修正改为 mount MAJ:MIN 对固定 sysfs 分区身份，保留隔离。真实一次性 ext4 隐藏 `/dev` 的回归通过，不当成实机新版本已通过。用户已完成新的只读物理进入；当前唯一活动诊断为 `.private/new-os-maintenance-diagnostic-20261021-first.json` / `.local/status-new-os-diagnostic-20261021.py`，保存启动后完整现场，不写分区、不 reset。不要再跑第一次 connect/activate/install 或旧 restore/return。详情见 [P6S](VALIDATION-FIRST-OS-P6S.md)。整体仍74/100、10010仍38；下面“写后回读仍在进行”为历史阶段。

2026-10-21 当前权威状态；历史原文已完整保留在 [交接归档](archive/handoff-before-oneclick-consolidation-20261016.md)。下述状态覆盖归档中的旧检查点，不把候选用户态当成新系统已经刷入。

**当前新安装的p14/rootfs、p15/NSHSTATE均已完整发送且取得明确最终ACK，原安装器正在完整写后回读。** 唯一索引仍`install-first-boot-20261021-first.json`，源8d2fd5a，观察器`.local/status-install-first-boot-20261021.py`；禁止拔插、手动reset或第二任务。尚未取得完整后快照、目标/当前保护范围比较及两FS检查结果，也未请求正常启动，整体仍74/100。成功后原安装器一次reset，先严格所选新USB/主机公钥连接，再真实启动健康核验和可信业务初始化。已准备私有connect-new-os-first-boot、inspect-new-os-health、initialize-new-os-business三个20261021 helper，均有前置状态门禁，不得提前运行或套旧四服务基线。下面“p14仍在发送”为历史阶段。

**重要：同一新系统安装已开始发送p14/rootfs原始数据。** 16KB配置已通过；完整执行前快照及两FS检查通过，索引`install-first-boot-20261021-first.json`，源8d2fd5a。当前可能已有实际分区写入，旧OS不可假定仍可直接启动；禁止拔插、reset、旧只读返回或第二个安装/恢复任务。只用`.local/status-install-first-boot-20261021.py`观察当前NAS运行，等待两个明确program最终ACK、完整后快照及目标/当前保护字节/文件系统检查，再由原安装器一次reset。新OS尚未取得写后校验或正常启动结论，整体仍74/100；下面的“尚未写入/未开USB”为此前检查点。

**用户已完成本轮新系统物理进入，同一安装watcher已识别9008并启动唯一安装器。** `install-first-boot-20261021-first.json`接续，当前先复核输入、再完整前快照，尚未取得分区写入/新系统启动成功结论。只观察同一运行，不重跑arm/halt/launcher或让用户再拔插；任何NAK/Unknown先查准确阶段，不自动reset或重试。成功的完整回读/reset后再用严格所选新USB身份和主机公钥连接，不能套用旧四服务基线。下文“待本轮物理步骤”已完成；整体仍74/100。

**当前：新OS匹配副本与独立默认验证均已在NAS完成，用户新授权下的唯一新安装探针已就绪，已请求正常halt一次。** 私有索引`install-first-boot-20261021-first.json`固定源8d2fd5a、完整CI39479577805成功、精确计划/工具和NAS目录。3909091328字节副本的保护范围/GPT/boot保持、两FS检查和默认validate实际退出0；无USB/写入/reset/安装标记消费。NAS新USB配置重新核对inactive且无网关/DNS；已核原身份/boot/四服务/空闲。继续时先核同一探针和60秒停机等待，再给本轮新系统安装的物理步骤；不要重跑prepare/arm/halt或旧恢复运行。已准备`.local/status-install-first-boot-20261021.py`观察器及严格新USB/主机公钥维护连接helper。尚未写新OS、未实测启动，仍74/100。详见[实验入口](experimental-first-boot.md)，下文“尚待NAS生成/验证”和“无活跃物理窗口”由本条覆盖。

**用户最新改为直接测试新OS，不再先做旧系统回刷演练。** 用户原话及范围见[首次整机实验](experimental-first-boot.md)。这是执行顺序变更，不代表完整恢复能力通过；仍74/100。710c73b完整CI39478305112成功，NAS默认显式接续验证passed/exit0且未开USB/未消费原标记，但该旧系统接续当前不执行。新固定install-system.py已实现独立候选生成/无USB验证/显式实验安装入口，122板级/34rootfs/7恢复通过；仅p14/p15，完整前后捕获与当前保护字节保持，独立install-attempts，原失败保留。当前旧OS正常、无活跃物理窗口；下一步冻结工具，在NAS生成匹配安装副本并验证，再准备新USB身份接入并安排本轮物理进入。不要运行旧恢复watcher/再次回刷旧OS；普通蜂窝仍禁用。下面“实际重试需新授权”为旧恢复路径，已被本条新OS方向覆盖。

**2026-10-21最新结果：本轮恢复在写配置阶段被明确NAK拒绝，未发送program/raw数据；棒子已单独安全返回并独立验证旧系统正常。** 614请求32KB，而真实8916声明16KB。`restore-run-20261021-first.json`仍passed=false，原干净完整前快照、NAK、消费标记保留；无分区写入，整体仍74/100。当前无活跃恢复/停机/用户操作窗口，不要复用本轮launcher或再次拔插。16KB固定修正和显式同plan接续门禁已通过116板级/34rootfs/7恢复；默认验证不接USB、不删除原标记，实际再次回刷需要用户重新授权。详见[P6R](VALIDATION-RESTORE-CONFIG-P6R.md)。新系统通用rootfs已在NAS独立校验保存，离线状态候选保持，尚未安装新OS。下文“正在读取/运行”均为这一失败前的历史检查点。

**2026-10-21最新覆盖：用户已授权一次旧系统p14+p15实际恢复，并已完成本轮按键拔插。** 唯一运行索引`restore-run-20261021-first.json`，冻结执行器614d95c，完整CI39473392006成功；来源和两份全盘捕获空间检查通过。同一watcher已识别物理口9008，当前正在另存完整执行前快照，尚不能宣告回刷通过。只允许原计划的两个分区，成功须完整写后回读、当前保护范围保持、文件系统检查、reset明确ACK和独立旧系统返回；失败不自动重试/reset，不重新运行prepare/halt/launcher。下文短只读探针已完成且正常返回验证通过；旧暂停已被用户“继续做”解除。维护身份真实USB候选登录、NAS配置inactive登记、离线状态候选均已通过，详见[P6Q](VALIDATION-MAINTENANCE-RECOVERY-P6Q.md)。整体仍74/100，累计10010仍38，无新增短信/普通蜂窝，新OS尚未安装。后续接手先观察同一私有索引和NAS运行，不要求用户重复拔插。

**2026-10-21用户已要求继续，旧暂停已解除。** 当前整体仍74%。3df97a8离线维护身份已在NAS实际生成，客户端私钥未上传；maintenance-seed-20261021-first.json固定所选材料，不得重新生成。1e7/39ff维护登录工具的loopback首测因旧SSH禁止转发而失败，旧策略保持；USB-direct第二测使用真实客户端和主机身份通过正确登录、错主机/错key/撤销拒绝、恢复与身份保持，cleanup_verified=true，见maintenance-seed-login-20261021-usb-second.json；仅候选夹具，未装实际状态。65b2生成的NAS新MAC配置经支持的/run目录实际注册为inactive，默认路由保持，无激活，nas-usb-profile-20261021-first.json保存前两次适配失败与成功结果。本次完整3909091328字节只读快照及两个文件系统检查通过，但9947执行器最终reset收到NAK；原失败保留，随后原只读工具另一次明确返回成功，旧boot/身份/四服务/PCM/空闲与101:103:700保持，索引restore-inspect-20261021-first.json。af889控制XML改为固定上游完整包封并保留每条回复，99板级/34rootfs/7恢复测试通过；尚待实机复核。当前新的短只读control-probe-20261021-first.json已请求一次halt，用户可配合，需完成60秒和探针检查后给步骤；不得复用旧halt或返回尝试。该探针只读GPT/boot、不写分区，受控reset回复保存私有。当前应用仍4e6、10010仍38、无新短信/蜂窝/刷写，新OS与完整恢复门禁false。下段旧“暂停”及“无活跃窗口”均为历史状态。

**当前已暂停。** 用户要求完成本轮后暂停并外出；三项收尾已完成，收到新的继续指令再启动后续工作。见[执行器P6P](VALIDATION-RESTORE-EXECUTOR-P6P.md)：9947a0f已实现固定恢复执行器，90板级/34rootfs/7恢复测试和完整CI39109738551成功；NAS真实备份/原计划的默认validate实际退出0，未开USB/未写分区/未reset/未消费尝试。私有索引restore-executor-validation-20261018-first.json保存来源、准确路径和结果。没有活跃构建/恢复探针/用户窗口。整体74%，未启动新OS或完成实机恢复；下段“执行器仍未实现”是P6O旧检查点，由本条覆盖。不要重跑已有launcher、旧停机或拨号尝试。

最新接续见[恢复计划P6O](VALIDATION-RESTORE-PLAN-P6O.md)：7b9d40f已在NAS对真实干净备份生成固定p14+p15计划，私有restore-plan-run-20261018-first.json passed/exit0，未开USB/未写分区。acd6b08严格ACK/固定阶段/无重试USB基础模块已通过83板级/34rootfs/7恢复测试及完整CI39099022089；它无CLI，完整恢复执行器仍未实现，勿接旧自动reset探针。维护客户端私钥已在本机生成并由Paramiko加载核对，索引maintenance-bootstrap-20261018-first.json，不要替换；未上NAS/设备，未安装公钥/主机身份。NAS现有USB直达10.55.0.2，但新serial会改变MAC，需有界匹配新物理接口。实际状态落盘需锚定目录FD或离线副本，保留旧101:103:700挂载根；不要用父目录可变的路径式先检查再写入。当前无活跃构建/恢复探针/用户操作窗口。整体74%，应用仍4e6，不重标旧制品。

最新覆盖：见[干净原始备份P0C](VALIDATION-EDL-CLEAN-P0C.md)和[内部镜像P6N](VALIDATION-SYSTEM-IMAGE-P6N.md)。新完整原始备份独立回读、两文件系统只读检查0、整图保持与正常返回通过，P0=7、总74/100约74%；下文旧73.5评分由本条覆盖。f270公开完整源码核验器已实际通过，board.kernel_source_commit现固定93a71ee，旧UNRESOLVED由本条覆盖；完整内核独立重建/冷启动/回刷未完成。新内部rootfs.img构建、逐项只读挂载核对与e2fsck通过。7091d0a完整CI通过：68板级/34rootfs/7恢复工具/209 Rust、production检查及Clippy；GitHub main已匹配7091d0a。所有刷写/发布门禁仍false，累计10010仍38。

物理阶段已结束：用户2026-10-18完成按键拔插，新recovery-edl-20261018-first.json passed=true/phase=completed，汇总recovery-edl-20261018-nas-evidence.json保存实际目录和摘要。探针/离线检查均退出0，两个只读loop已分离；旧系统四服务active、身份/boot保持、PCMclosed、电话空闲、普通蜂窝断开。不得重跑prepare/halt/launch helper，当前不需用户操作。正常poweroff会自动重启，本次另一次正常halt+60秒后才物理进入，事实记录保留。实际状态守卫及私有固件--check第二轮通过，第一轮暂存脚本执行权限失败保留。真实Rust命名空间/维护钥匙仍未安装，旧挂载根101:103:700保持；新首启改根属主意味着回退不能仅换rootfs。下一步先准备维护入口、USB10.55.0.2/29在NAS的接入及有限写入/回读恢复方案，再安排新OS冷启动，不能跳过门禁。

## 目标与进度

- 持续目标为 100%；用户要求每次最终回复报告百分比。
- 固定P0—P7分母，当前 **74/100，约74%**。P0=7、P1=14.5、P2=15、P3=14.5、P4=7.5、P5=5、P6=6.5、P7=4。a15持续Chrome下行已计P3+0.5；a19首句完整和新收尾真实Chrome确认通过，不重复计点；双向听音/来电及完整系统启动仍缺。见[进度刻度](PROGRESS-SCALE.md)，构建/诊断不加分。
- 当前 SIM 没流量：普通 APN、蜂窝互联网探测及 USB/Wi-Fi 失败后的隐式蜂窝回退全部禁止，等换卡并明确安排。电话需要的 WWAN/IMS 保留。
- 用户持续允许拨打 10010，不需逐次确认。只做独立标记的单次短通话、自动挂断、记录次数；Unknown 先核验，不自动重拨。其他号码或新短信不能推定授权。
- 内核/DTB 精确来源已固定；完整新 OS 启动和回刷仍未验证。不写GPT/NV/校准区，不产出可刷发布包；原始分区写入须先完成有限恢复/维护方案。

## 当前真实浏览器验收

2026-10-17，4e6fea6/a19真实Chrome一键听音：用户明确“现在所有的声音都听完整了”，并反馈“测试已结束，电话已挂断；观察接通后接音0.7秒”。Native约24.26秒传声，1212captured/sent/played、1220received、19dropped、1次可恢复xrun；answer1493ms/verified40ms/activation141ms。停机后只读一致数据库确认本次dial/hangup均submitted且同一精确call ended/observed；MM全部terminated、PCMclosed、Voice撤回/拒绝、旧四服务active、预览/独占守卫停止、Windows临时证书直接移除并核验。私有browser-a19-acceptance.json与browser-preview-4e6fea6-a19-native.log保留，累计10010=38。0.7秒是页面观察ACTIVE至媒体启用，不是运营商接通至扬声器首声的端到端测量；尚未验收双向听音。无新短信/普通蜂窝测试/分区写入。

此前316005e/a17显示0.5秒、Native约26.6秒，但当时首句完整和页面挂断未确认；原报告保持。a16无拨号Verified保持12秒/零PCM/无票据及单次通话观察ACTIVE后1187ms开PCM通过。

a15/0587cf7用户确认持续Chrome下行正常，但初始约7秒漏句；P3此前只因该项加0.5，预连接改进不重复加分。a10—12已确定旧Go agent约15秒Hangup冲突，必须保留独占/设备独立恢复；a13原24秒目标未通过，原passed=false与adjudication均保留；a14未拨号，不能计数。a1—17实际标记都不可复用；a17自动撤销了a15留下的旧prepared挂断，无重复提交。

收尾4e6fea6已冻结/构建，拨号提交起27秒开始一次挂断，给原30秒设备保护留3秒；不先做阻塞查询，随后最多3秒/6次只读观察精确call。Node三组含迟到ended、丢失挂断回复、始终active、查询挂起通过。a18在本机Node执行冻结的同一CallTest JS，通过严格HTTPS真实单次10010：一次dial/一次hangup、未续租，精确call ended/observed和“电话已挂断”终态通过，全程28.654秒。该用例Audio为stub，未打开PCM/麦克风，不把0.0秒显示值当成音频证据，也不冒充Chrome已完成新收尾验收。.private/call-cleanup-a18.json passed/cleanup_verified=true。

详细证据见 [浏览器验收](VALIDATION-BROWSER-MEDIA-P3I.md)。此前 Chrome 插件能列标签页但接管为 Debugger unattached，没有实际控制浏览器，也不绕过其他页面的访问阻止。

## 当前窗口、守卫和证书清理

主要私有记录 `.private/browser-preview-20261016-first.json`，保留同一设备身份、浏览器配对和预览证书。访问地址从report.origin读取；实际目录、各 unit、owner 私钥和 SSH 凭据只从私有文件读取，不提交或打印。当前程序来自 report.diagnostic_revision 与 media-current-build.json，不能运行旧 874/715 重启脚本。

- 当前 helper：`.local/start-codec-browser-preview.py`、`.local/collect-latest-media-closure.py`、`.local/collect-core-media-diagnostics.py`；a12已消费，禁止复用其once/watch脚本拨号。`tools/legacy_controller.py`管理旧Go控制器的有界暂停/先停Core后恢复，单一systemd unit防重入，保留原先inactive状态；仅供旧系统过渡测试，新OS不包含旧agent。
- 独立 root 观察器最多等十分钟，发现一次 outgoing 后 30 秒停止 Core，撤回精确哈希的临时 Voice 规则。它不主动拨号或重拨。浏览器自己的测试亦不续租 Core 30 秒控制租约。
- Native collector 的输出文件名仍是历史 `.private/browser-preview-71506f0-current-native.log`，内容是当前实际 PID；采集后复制为当前源版本文件，别误认为程序是 715。
- **本轮证书已直接移除，installed=false/removed=true。** 后续同一证书仍只有临时听音授权，结束直接运行`.local/cleanup-preview-trust.ps1`并核验；不要依赖旧后台清理进程。SHA-256 e4726db3ab982ee19661c9af85bf6ab08fd59b224d045e5de49913ed07404809；拇指纹61EB6A4F98B49290C939A2187B331098B38003A4。只能处理本次证书，不关闭证书检查。
- 结束执行 `.local/browser-preview-control.py cleanup`，再核验呼叫全 terminated、PCM全closed、旧 nassimhub-agent/ModemManager/NetworkManager/rmtfs active、临时 Voice 规则撤回且恢复拒绝。cleanup helper 本身不替代呼叫/规则核验，也不移除 Windows 证书。

## 当前源码与制品

当前部署构建源 `4e6fea6bf482718e4958d3541a8dee7b362f86c7`，115输入，归档SHA `d02625f65041de8207d8d3d81c20f623c1f9681e00d64abb3f565fee928b6012`。GitHub已精确保存。316005e/a17为历史Chrome预连接证据，4e6fea6/a18为音频stub真实控制，4e6fea6/a19为最新真实Chrome首句/持续下行/收尾证据；新增驱动源码/文档不重标既有应用制品。

- 本机WSL六生产ARM：`.local/wsl-artifacts-4e6fea6bf482718e4958d3541a8dee7b362f86c7`，209 workspace/退出0。4e6只改JS和文档；共享Rust源码的全量Clippy证据仍是316，不称又跑了4e6 Clippy。
- NAS固定无网络ARM容器Native：`.local/native-4e6fea6/rustsimhub-audio-alsa`，5516704字节、SHA `1733b123fbb40ad18fe4641f181676d943cb5f91359ba76461c096893794bbc2`；21项/release3m12s/退出0，日志`.private/native-4e6fea6-build.log`。旧316 Native/归档及所有日志保留。
- 严格TLS五网页与冻结Git资产一致。和316一样首次启动立即检查遇一次5秒连接timeout，随后完整五项通过；启动helper返回不等于HTTPS就绪，必须检查后才开放用户窗口。Node proof/media/call-test三组通过。
- `.private/media-current-build.json`、默认`.local/source.tar.gz`和frozen 4e6归档指本轮构建。开发签名包`.private/boot-application-4e6fea6`三个epoch已保留，seed仅本机；4e6正式Core/Media/Edge服务组复验passed/cleanup_verified=true，详见P6K追加节；尚未完整新OS启动/刷写。
- `.private/http-preconnect-idle.json`、`.private/headless-preconnect-a16.json`、`.private/browser-a17-acceptance.json`为316预连接证据；`.private/call-cleanup-a18.json`为4e6真实控制/收尾证据。a18退出旧四服务active、MM全部terminated、PCMclosed、Voice拒绝、预览/独占守卫停止、Windows临时证书保持已移除。没有新短信/普通蜂窝测试/分区写入。
- 715旧Media服务组试验的27个无引用重复程序已核本机保留副本/精确摘要、root/模式/链接/设备/inode后回收，空闲97226752→194990080字节。旧试验的身份/短信/日志/manifest等其余文件的聚合摘要完全不变。报告`.private/old-media-fixture-reclaim-plan.json`；旧fixture要复用需先按restore_sources恢复程序，不能假定仍可运行。其他旧制品/签名包保留。

## 已完成但不可扩大的系统实证

- [P6M 固定驱动候选](VALIDATION-KERNEL-AUDIO-P6M.md)：2026-10-17 P6M：组合源075b68e、应用/Native仍4e6fea6；22个逐字节重建音频驱动已纳入全新候选。12367项/SHA c621fdbdedee685d0f6a930a949d36f2cb6dea29953d8240b4dda7bb17b95376；host及候选自身ARM modprobe的22项依赖图/四个别名一致，后者在只读/断网/无CAP_SYS_MODULE条件下运行，没有加载模块。七ARM非Root实际执行、包含音频驱动服务的完整systemd静态依赖及树保持通过；55板级/34rootfs回归通过。原initramfs为zstd，551项/100模块，无qcom音频；固定上游boot+重建DTB+root=LABEL=rootfs+补零64MiB完整复现备份SHA d5790e07…。公开核验器只写JSON不写启动镜像，实际报告passed=true；初次只替换DTB对照失败保留。新驱动守卫仅隔离夹具/静态验证，未在棒子加载或启动新OS。旧P6L树、原失败记录及所有旧制品保留，10010仍38，无新短信/蜂窝/分区写入，整体73.5/100约74%。
- [P6L 当前完整候选](VALIDATION-SYSTEM-TREE-P6L.md)：组合源fba528e、当前4e6签名应用/同源Native与锁定220包基础树，新树12340项/SHA4324468e63920ce79bef8cd0da1a0a2e4bc7c201f6ea866a9da4641e5da8701e。修复umask077导致新公开程序目录0700的真实问题；42项板级、七个非Root ARM实际运行、全服务静态依赖及树保持通过。WSL目录`/home/wzs11/.cache/rustsimhub/system-tree-p6l-4e6fea6-second`，验证报告`.private/system-tree-p6l-4e6fea6-second-verification.json`/verify.exit0。首树权限失败及第二包装脚本CRLF退出127原证据保留，LF脚本重验同一第二树通过；不是新树已在设备启动。新候选全部刷写/发布/恢复/完整启动门禁false，不重复加分。
- [P6K Media 开机组](VALIDATION-MEDIA-STARTUP-P6K.md)：4e6同源全组已重新通过可信初装、签名槽位/UID/Root/inode/空cap、真实Media SIGKILL整组回退、第三版本确认保持、第二次Edge SIGKILL预算耗尽整组停止和清理；715旧证据保留。P6仍6.5，不重复计点。
- 完整旧候选用户态 RootDirectory 仍为 `.private` 报告记录的 system-tree-ofY1ttXm/rootfs；不是新系统完整上电。旧P6J组合及离线 rootfs.img仍保留，所有刷写/发布门禁false。实机 NSHSTATE 初始化、实际维护公钥安装、完整开机/掉电/回刷闭环仍缺。
- [P0B 9008只读](VALIDATION-EDL-READ-P0B.md)：实际进入/完整3909091328字节主区读取/返回通过。原图只读文件系统检查失败，原图保留；独立 preen 派生副本两分区检查0，不等于原图一致或完整恢复成功。boot0/1独立备份保留。无活跃EDL等待器，当前不需要用户拔插。
- NAS兼容桥/旧库迁移、USB/Wi-Fi真实HTTPS与NAS程序化媒体、独立短信等实证见 PROGRESS.md 对应验证文档。其他SSID/首启热点、远程TURN、来电与真实双向听音、长期稳定/等功能性能对照等仍缺。

## 接手顺序与工具

当前优先使用P6M候选：/home/wzs11/.cache/rustsimhub/system-tree-p6m-audio-4e6fea6-second，源075b68e，应用4e6不重标，12367项/树SHA c621fdbdedee685d0f6a930a949d36f2cb6dea29953d8240b4dda7bb17b95376。22个源重建驱动已安装，目标ARM modprobe22图/四别名、七ARM程序、完整systemd静态依赖和树保持通过；完整OS上电/实际驱动加载仍未验证。私有system-tree-p6m-audio-4e6fea6-second-verification.json及system-tree-p6m-target-kmod-first.json passed=true。原P6L保持不改；first包装参数+错误在创建树前拒绝、exit2记录保留。

22模块的固定源码/编译器、零fuzz补丁和原诊断路径映射已闭环，公开锁firmware/kernel/audio/modules/source.json，私有重建kernel-source-20261017-first/module-rebuild-third.json/log/exit0。WSL artifacts在/home/wzs11/.cache/rustsimhub/kernel-module-audit-20261017/rebuild-third/artifacts，编译器在/home/wzs11/.cache/rustsimhub/module-compiler-20261017。composer含Native必须--audio-modules；所有组合输入先提交。新test-boot-service-group必须显式--verify-preloaded-audio先只读核16个现存驱动，然后仅验证用户态；它不会调用宿主模块加载器，也不验证新驱动服务的实际加载。既有P6K全组证据属于旧依赖图。

启动配方核验已通过：固定上游boot+源重建DTB、仅root=LABEL=rootfs替换与Android v0 image-id/补零64MiB，完整SHA与board记录d5790e07…一致；tools/verify-audio-boot-recipe.py实际报告repository-boot-recipe-first.json passed=true，只输出JSON，未写boot镜像。原ramdisk只读zstd/CPIO审计无qcom音频模块；初始gzip假设及只替换DTB失败证据保留。55板级/34rootfs最终回归日志audio-module-boot-recipe-tests-final.log/exit0。

下一步先把完整源码Git树的核验固化为公开工具/板级来源记录，再推进每设备初始化、维护/完整冷启动和恢复；独立重编译整个内核是可复现性的后续工作，不把它混同于来源归属；boot重打包差异已解释，不再重复排查根标签。board.kernel_source_commit仍UNRESOLVED，所有flash/release/recovery/board_boot门禁false。不要重下载已核验的七个上游输入，不要修改既有候选或使用旧应用制品重启脚本。无存活编译/下载任务，598f273完整CI已通过，当前不要求用户拔插。

真实Chrome首句/新收尾已由a19人工确认，不必重复问旧问题；双向听音/来电等仍待用户参与，流量待换卡。当前无待答问题、无用户测试窗口/临时证书；a1—19标记均结束/消费，累计38。

Python 使用 `C:/Users/wzs11/AppData/Local/Programs/Python/Python312/python.exe -X utf8`；私有依赖 `.local/python-deps`。SSH用 `tools/remote.py` 读取 `.private/nas.json`/device.json，严格主机钥匙固定，设备经NAS。主六程序强本机WSL user wzs11、持久cache/8路交叉编译；Native仍NAS，未完成本机Native SDK迁移。源先提交、package-source.py冻结；构建索引改变前确认无编译任务。GitHub使用精确非强制对象发布脚本，勿推缺workflow授权的 codex/recovery-ci-pending 历史分支。


2026-10-17补充：598f273完整CI通过（https://github.com/Viper-Boss/RustSimHub/actions/runs/38996792278），包含Rust测试/production检查/Clippy与板级夹具。随后对固定orig源码tar流式计算86859个文件/符号链接的Git blob、mode和目录树，1479589504字节，根tree精确为72d4e8e42649f763dd969a1de893051ed64387a1，与93a71ee提交一致；此前只核68个qcom文件的范围已补到完整源码。没有解包或执行源码。私有complete-source-tree-hash.json及.local/hash-complete-kernel-source-tree.py保留；这证明完整来源包与固定Git提交一致，不声称独立重编译了整个内核。
