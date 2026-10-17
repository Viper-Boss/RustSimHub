# RustSimHub 当前接手入口

2026-10-17 当前权威状态；历史原文已完整保留在 [交接归档](archive/handoff-before-oneclick-consolidation-20261016.md)。下述状态覆盖归档中的旧检查点，不把候选用户态当成新系统已经刷入。

## 目标与进度

- 持续目标为 100%；用户要求每次最终回复报告百分比。
- 固定P0—P7分母，当前 **73.5/100，约74%**。P0=6.5、P1=14.5、P2=15、P3=14.5、P4=7.5、P5=5、P6=6.5、P7=4。a15用户确认持续Chrome下行正常，P3+0.5；双向听音、首句完整及新收尾的真实Chrome确认仍缺。见[进度刻度](PROGRESS-SCALE.md)，构建/诊断不加分。
- 当前 SIM 没流量：普通 APN、蜂窝互联网探测及 USB/Wi-Fi 失败后的隐式蜂窝回退全部禁止，等换卡并明确安排。电话需要的 WWAN/IMS 保留。
- 用户持续允许拨打 10010，不需逐次确认。只做独立标记的单次短通话、自动挂断、记录次数；Unknown 先核验，不自动重拨。其他号码或新短信不能推定授权。
- 内核/DTB 精确来源、完整新 OS 启动、回刷等门禁未解除；不写原始分区/GPT/NV/校准区，不产出可刷发布包。

## 当前真实浏览器验收

累计实际10010 **37次**。316005e/a17真实Chrome一键先完成DTLS再拨号，页面显示“观察接通后接音0.5秒”，Native持续约26.6秒/1331captured/sent/played；首句完整尚未明确人工确认。页面末尾“挂断尚未确认”，后台停机只读副本显示本次hangup submitted、精确call ended/observed，MM全部terminated；旧四服务active、PCMclosed、Voice撤回/拒绝、预览/独占守卫停止、Windows临时证书移除全部核验。报告.private/browser-a17-acceptance.json。前一a16自动NAS测试观察ACTIVE后1187ms开PCM、媒体20.657秒；无拨号Verified保持12秒/零PCM/无票据也通过。

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

当前部署构建源 `4e6fea6bf482718e4958d3541a8dee7b362f86c7`，115输入，归档SHA `d02625f65041de8207d8d3d81c20f623c1f9681e00d64abb3f565fee928b6012`。GitHub已精确保存。媒体预连接实际Chrome证据属于316005e，收尾真实控制证据属于4e6fea6/a18，不能混标。

- 本机WSL六生产ARM：`.local/wsl-artifacts-4e6fea6bf482718e4958d3541a8dee7b362f86c7`，209 workspace/退出0。4e6只改JS和文档；共享Rust源码的全量Clippy证据仍是316，不称又跑了4e6 Clippy。
- NAS固定无网络ARM容器Native：`.local/native-4e6fea6/rustsimhub-audio-alsa`，5516704字节、SHA `1733b123fbb40ad18fe4641f181676d943cb5f91359ba76461c096893794bbc2`；21项/release3m12s/退出0，日志`.private/native-4e6fea6-build.log`。旧316 Native/归档及所有日志保留。
- 严格TLS五网页与冻结Git资产一致。和316一样首次启动立即检查遇一次5秒连接timeout，随后完整五项通过；启动helper返回不等于HTTPS就绪，必须检查后才开放用户窗口。Node proof/media/call-test三组通过。
- `.private/media-current-build.json`、默认`.local/source.tar.gz`和frozen 4e6归档指本轮构建。开发签名包`.private/boot-application-4e6fea6`三个epoch已保留，seed仅本机；4e6正式Core/Media/Edge服务组复验passed/cleanup_verified=true，详见P6K追加节；尚未完整新OS启动/刷写。
- `.private/http-preconnect-idle.json`、`.private/headless-preconnect-a16.json`、`.private/browser-a17-acceptance.json`为316预连接证据；`.private/call-cleanup-a18.json`为4e6真实控制/收尾证据。a18退出旧四服务active、MM全部terminated、PCMclosed、Voice拒绝、预览/独占守卫停止、Windows临时证书保持已移除。没有新短信/普通蜂窝测试/分区写入。
- 715旧Media服务组试验的27个无引用重复程序已核本机保留副本/精确摘要、root/模式/链接/设备/inode后回收，空闲97226752→194990080字节。旧试验的身份/短信/日志/manifest等其余文件的聚合摘要完全不变。报告`.private/old-media-fixture-reclaim-plan.json`；旧fixture要复用需先按restore_sources恢复程序，不能假定仍可运行。其他旧制品/签名包保留。

## 已完成但不可扩大的系统实证

- [P6L 当前完整候选](VALIDATION-SYSTEM-TREE-P6L.md)：组合源fba528e、当前4e6签名应用/同源Native与锁定220包基础树，新树12340项/SHA4324468e63920ce79bef8cd0da1a0a2e4bc7c201f6ea866a9da4641e5da8701e。修复umask077导致新公开程序目录0700的真实问题；42项板级、七个非Root ARM实际运行、全服务静态依赖及树保持通过。WSL目录`/home/wzs11/.cache/rustsimhub/system-tree-p6l-4e6fea6-second`，验证报告`.private/system-tree-p6l-4e6fea6-second-verification.json`/verify.exit0。首树权限失败及第二包装脚本CRLF退出127原证据保留，LF脚本重验同一第二树通过；不是新树已在设备启动。新候选全部刷写/发布/恢复/完整启动门禁false，不重复加分。
- [P6K Media 开机组](VALIDATION-MEDIA-STARTUP-P6K.md)：4e6同源全组已重新通过可信初装、签名槽位/UID/Root/inode/空cap、真实Media SIGKILL整组回退、第三版本确认保持、第二次Edge SIGKILL预算耗尽整组停止和清理；715旧证据保留。P6仍6.5，不重复计点。
- 完整旧候选用户态 RootDirectory 仍为 `.private` 报告记录的 system-tree-ofY1ttXm/rootfs；不是新系统完整上电。旧P6J组合及离线 rootfs.img仍保留，所有刷写/发布门禁false。实机 NSHSTATE 初始化、实际维护公钥安装、完整开机/掉电/回刷闭环仍缺。
- [P0B 9008只读](VALIDATION-EDL-READ-P0B.md)：实际进入/完整3909091328字节主区读取/返回通过。原图只读文件系统检查失败，原图保留；独立 preen 派生副本两分区检查0，不等于原图一致或完整恢复成功。boot0/1独立备份保留。无活跃EDL等待器，当前不需要用户拔插。
- NAS兼容桥/旧库迁移、USB/Wi-Fi真实HTTPS与NAS程序化媒体、独立短信等实证见 PROGRESS.md 对应验证文档。其他SSID/首启热点、远程TURN、来电与真实双向听音、长期稳定/等功能性能对照等仍缺。

## 接手顺序与工具

4e6正式服务组与P6L当前完整候选离线验证通过。下一步推进内核/音频DTB来源及真实每设备首启、维护与系统恢复门禁；流量待换卡。固定KyonLi6.12.49-1的七个发布输入已下载并核SHA，无存活下载/编译任务；私有`.private/kernel-source-20261017-first`保存acquisition/boot-comparison/dtb-comparison和repository-dtb/DTB-REBUILD.json。备份boot的gzip内核、解压Image、ramdisk和second与上游逐字节一致，DTB经仓库工具/固定AirSimHub源码重建也逐字节一致（afb1027c…/52161字节）。44项板级严格umask回归/真实DTB重建通过；原缺DTC和测试fixture权限失败均保留，v2退出0。DTC工具仅解包在WSL用户cache dtc-tools-20261017，未安装系统包。额外音频模块来源/完整boot重打包及源归属仍未闭环，board.toml保持UNRESOLVED；下一步从已下载上游源码/headers和固定音频补丁核对模块，不重新下载。见KERNEL-PROVENANCE.md。预连接和一次挂断已实机证明，但首句完整、真实Chrome新收尾和真正双向听音仍需用户参与。当前唯一待答问题是a17是否听全开头，无用户测试窗口/临时证书；a1—18标记均已结束/消费，累计37。

Python 使用 `C:/Users/wzs11/AppData/Local/Programs/Python/Python312/python.exe -X utf8`；私有依赖 `.local/python-deps`。SSH用 `tools/remote.py` 读取 `.private/nas.json`/device.json，严格主机钥匙固定，设备经NAS。主六程序强本机WSL user wzs11、持久cache/8路交叉编译；Native仍NAS，未完成本机Native SDK迁移。源先提交、package-source.py冻结；构建索引改变前确认无编译任务。GitHub使用精确非强制对象发布脚本，勿推缺workflow授权的 codex/recovery-ci-pending 历史分支。
