# RustSimHub 当前接手入口

2026-10-21。整体 **74/100，约74%**，固定权重见 [进度刻度](PROGRESS-SCALE.md)。用户已醒来并完成本轮按键拔插，继续自主推进。完整历史原文见 [本轮前归档](archive/handoff-before-first-os-consolidation-20261021.md) 和 [更早归档](archive/handoff-before-oneclick-consolidation-20261016.md)。

**最新现场优先于下文历史：**新的只读捕获索引为 `.private/new-os-maintenance-diagnostic-20261021-second.json`。用户本轮已完成按键；完整3909091328字节捕获、摘要/GPT/boot、原图保持及loop分离通过，探针退出0，无分区写入或reset。原启动因SSH不可用未正常关机，两FS只读退出4，失败保留。日志确认SSH准备失败、音频检查失败和未初始化时app-recover缺运行目录，尚不能断言维护修正解决全部启动故障。

当前状态独立副本强制preen退出1→只读0，旧101项完全一致、业务目录未初始化且为空、维护/USB材料保持，原图保持。第三安装因此选择同一原始干净维护状态及bbb1081新rootfs；独立 `.private/install-first-boot-20261021-third.json` 的准备/两FS/离线验证全部通过，准备进程退出0。冻结执行工具从已通过CI的c5ee339准确Git对象读取，不用当前HEAD冒充其来源。已在同一已记录9008会话启动一次第三安装：先另存完整现场、核精确未干净摘要，再仅写p14+p15、完整回读/保护范围/两FS和正常reset。结果尚待完成，禁止重复启动或在Unknown后重试。唯一只读monitor为 `.local/monitor-install-third-boot-20261021.py`；执行句柄及准确接续保存在 `.private/third-attempt-continuation-20261021-first.json`。目前无新的物理动作要求，保持连接。

自动审批服务已恢复，同一正确路径的8项内核工具夹具及139项完整系统工具测试均通过，无跳过。新second目录的完整内核/模块集成复现已退出0，181.333秒；两种内核格式及681个打包模块全字节匹配，源码/配置保持。当前工具证明见P6T；硬件捕获与该本机构建相互独立。

已将工具和证明发布到GitHub main `f4bc8313bb8c0851cf6feb907d1fb6fe64d477d9`，[完整CI39541470946](https://github.com/Viper-Boss/RustSimHub/actions/runs/39541470946)成功。没有用CI绕过审批；先在正常审批下完成当前实现的本机验证，才发布。

另修正已确认的未初始化app-recover命名空间失败：恢复单元加入与选择器一致的持久安装完成条件。真实systemd临时单元复现修正前失败，修正后初始化前跳过、初始化后缺目录仍226、一次恢复/第二次停组及marker保持均通过；140项系统工具回归通过。临时业务控制为记录桩，未调度任何真实业务。**这项源码修正不在正在安装的bbb1081冻结镜像中**，不要以当前HEAD推定已上机；实际维护连接后仍须检查音频失败及恢复单元，再决定如何纳入下一通用候选。详情见application-startup-group.md。

**启用业务前新增门禁：**4e6fea6原始锁文件RustSec扫描workspace命中5条TLS公告，Native为0。当前只更新rustls至0.23.46、webpki至0.103.16；同一官方数据库复核均0漏洞，209项Rust/生产检查/Clippy通过。5项维护状态警告仍记录，不忽略。新ARM与签名程序尚待构建/上机；冻结bbb1081仍用旧4e6fea6程序。先观察第三安装并取得维护/音频日志，**暂不运行业务初始化/owner helper**，完成修复产物交付和真实健康检查后再继续。见[依赖复核P7D](VALIDATION-DEPENDENCIES-P7D.md)。恢复条件修正6b23340的完整CI39544172775也已成功。

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
