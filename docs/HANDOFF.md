# RustSimHub 当前接手入口

2026-10-18 当前权威状态；历史原文已完整保留在 [交接归档](archive/handoff-before-oneclick-consolidation-20261016.md)。下述状态覆盖归档中的旧检查点，不把候选用户态当成新系统已经刷入。

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
