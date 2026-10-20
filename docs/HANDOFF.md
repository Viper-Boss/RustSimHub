# RustSimHub 当前接手入口

2026-10-21。整体 **74/100，约74%**，固定权重见 [进度刻度](PROGRESS-SCALE.md)。用户最新要求继续自主推进，已去休息；当前不需要其操作。完整历史原文见 [本轮前归档](archive/handoff-before-first-os-consolidation-20261021.md) 和 [更早归档](archive/handoff-before-oneclick-consolidation-20261016.md)。

## 当前硬件现场

**第二次安装已退出0：两分区最终ACK、3909091328字节完整写后回读、目标/保护范围/GPT/boot、写后两FS只读0和正常reset ACK均通过。新USB接入通过，维护SSH有界等待仍拒绝。当前没有活跃写入器或监视进程；保持现状，用户已休息，不请求拔插。**

- 唯一索引：`.private/install-first-boot-20261021-second.json`。
- `.local/monitor-install-second-boot-20261021.py` 已按安装exit0正常结束；连接helper也已退出1。`.private/new-os-second-connect-20261021.log`保留有界拒绝；该索引另记安装成功与维护失败，不能再运行second激活或安装器。
- 冻结安装工具源：`a9cf521af45df7aee431c3e0e8b3cc19445e0921`；完整CI39493697545成功，130板级/34rootfs/7恢复测试通过。
- 第二3909091328字节匹配候选及写后现场均已验证。真实计划/工具目录/摘要只从私有索引读取；PID为历史记录，不代表仍运行。
- 第二安装复用了原固定9008会话，随后已经正常reset；现在是新系统USB，不能假定仍在9008。
- 先重新捕获完整执行前现场，必须与已保存的准确未干净现场摘要一致；原失败只读FS报告保持false。仅明确退出4且捕获保持/GPT/boot/范围/分离均核对的结果可由本计划显式确认替换。
- 只写p14/rootfs、p15/NSHSTATE；完整写后回读、当前保护范围保持、两FS干净、冻结源保持后，原安装器只发一次正常reset。NAK/短写/Unknown/检查错误不自动重试或reset。
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

下列second版helper保留作接续模板，**已使用的connect不能重跑，后续应绑定新的独立索引**；尚未运行健康/校时/业务/owner步骤，不能先行绕过门禁：

1. `.local/connect-new-os-second-boot-20261021.py`：一次激活；严格物理口/USB serial/MAC、NAS配置与默认路由、选定主机公钥和PC持有维护私钥；核实际守卫字节、真实p14/p15和维护材料。不要使用第一次connect或旧device.json连接器，也不关闭主机公钥检查。
2. `.local/inspect-new-os-second-health-20261021.py`：只读采集真实单元、固件守卫、modem、radio、PCM、UTC以及16项音频srcversion。每次观察使用独立私有文件，失败保留；只有守卫、所需服务、radio和关闭PCM等全部通过才记首启健康。初次音频问题尚未解决，先看实际版本/单元日志，不放宽守卫。
3. `.local/correct-new-os-second-clock-20261021.py`：仅在已取得真实时钟偏差后按可信NAS/PC UTC校准；不启用普通APN或互联网探测。校准后重新观察，不把它当作RTC持久化已经通过。
4. `.local/initialize-new-os-second-business-20261021.py`：须实际健康和UTC已通过，可信初始化只执行一次；私有交付保存到 `.private/new-os-business-20261021-second`。started/Unknown不得盲目重建身份或设置码。
5. `.local/claim-new-os-second-owner-20261021.py`：已准备、未运行。PC生成并保留owner私钥，NAS只接收公有证书/请求；严格HTTPS及设备挑战签名核验，setup_finish之前记录一次性状态，再登录检查生产API与收件箱。未知setup结果先用原owner只读核验，不重复初始化。

上述9个私有helper及6段远程Python已做语法检查；后续现有owner只读登录helper也已检查，健康观察明确要求16项音频身份全部匹配。这些准备不算实机验收。新系统未安装Python，设备检查使用Shell/SFTP，Python仅运行在PC/NAS。

## 本机完整内核构建

固定93a71ee完整源码、原配置与签名校验的Debian GCC14.2.0-19/binutils2.44已在本机WSL完成 `Image.gz modules dtbs`，681模块、15468项导出表一致，完整源码树保持，约193.5秒。报告 `.private/full-kernel-rebuild-20261021-fifth.json`；前四次来源文本/工具链能力/make准备失败保留。独立sixth也已退出0，配置和上游横幅精确相同，仍有14943个字节不同，逐字节复现未通过。原配置无非零CRC，编译器版本一致不代表编译器二进制相同。两轮未访问设备、生成/写入boot或替换冻结安装输入。详见 [P6T](VALIDATION-FULL-KERNEL-P6T.md)。

## 持续边界与待验收

- 整体固定P0—P7=74点：7、14.5、15、14.5、7.5、5、6.5、4；实际新系统健康/业务验收后再按里程碑更新，不按测试数加点。每次最终回复报告百分比。
- SIM无流量。换卡且用户明确安排前，不启普通蜂窝APN、不让USB/Wi-Fi隐式回退蜂窝、不跑蜂窝互联网探测；保留电话必需WWAN/IMS。
- 用户持续授权10010短通话，累计 **38**，本轮没有新拨号/SMS/录音。每次独立幂等键和防重标记、有界自动挂断；Unknown先核验，不自动重拨。其他号码或新短信不能推定授权。
- GPT/boot/基带/NV/校准区不写。完整内核已独立构建但字节复现未通过；完整恢复、新内核上机、掉电/更新恢复、自动健康确认、远程TURN/蜂窝/长期验收等仍缺。
- 前次4e6/a19真实Chrome已确认首句及持续下行完整、自动挂断、观察接通至接音0.7秒；不是新OS或双向验收。详见 [浏览器证据](VALIDATION-BROWSER-MEDIA-P3I.md)。Windows临时预览证书已经实际移除；新证书未授权加入系统信任，不复用旧证书授权。
- 后续确需物理冷启动/恢复按键、人工双向/来电听音或换卡时，保存现场并等用户醒来；继续推进其他不依赖人工的任务。当前不再发送操作请求。
- 设备标识、凭据、owner/维护私钥、原始日志/短信/快照仅.private/.local，禁止提交；只从私有索引取真实位置/身份，不猜测。
