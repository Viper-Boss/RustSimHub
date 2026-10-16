# RustSimHub 当前接手入口

2026-10-16当前：可信首次安装与持久TLS私有交付实机通过，见VALIDATION-INSTALLATION-P6J.md；固定70.5/100约71%，P6=6/12。不可变工具源码fbc2cdba4f8453dfe8484caf35e13a077e41da89，源9f331a82de78d90008b6c7c81858f48995bf23151986633fe10e40246fd22266，191项/六程序ARM/相关Clippy/CI38770338860成功，板级38。主应用/Native仍9b/19f。默认.local/source.tar.gz现属于fbc，独立source-install-fbc2cdba…tar.gz保留；新工具.local/wsl-artifacts-fbc2cdba4f8453dfe8484caf35e13a077e41da89。

实机.private/installation-boot-first.json passed/cleanup_verified=true：未初始化不启动且不耗恢复预算、错误签名无新身份、Root私有handoff、重复初始化拒绝、首次owner/严格TLS/完整Core及原SIGKILL整组回退回归通过。新fixture /var/tmp/rustsimhub-boot-group-gp3sz82L；无安装生产服务、无新OS启动、actual_nshstate_guard_run=false，实际NSHSTATE初始化尚未做。无活跃实机程序；旧四服务active/PCMclosed，新增电话/SMS/蜂窝0，10010累计15。

新完整组合WSL /home/wzs11/.cache/rustsimhub/system-tree-p6j-installation-first，组合源码fbc，12338条目、树3e3f51ced9e4b377576ccfb7bfb697e1bd6cabad542f7961cff78048a553bf4c；systemd依赖只读验证退出0、树保持。所有刷写/发布/恢复/新OS启动门禁仍false。下面P6I的ed786/f83组合是上一轮已验证历史，不是当前工具或组合输入。

离线镜像WSL /home/wzs11/.cache/rustsimhub/system-image-p6j-first/rootfs.img，3436183552字节、SHA26cca986649b9b0aa8cea386f55b2d5620bc11831c12807316815aa835dd41e7；同目录SYSTEM-IMAGE.json passed/source_tree_unchanged=true。Root .local/build-installation-image.py（SHA933dd5c9…）以固定UUID/hash_seed/epoch生成，再e2fsck -fn、私有loop/ro/noload挂载和全部条目/owner/mode/cap核验通过，额外仅空Root0700 lost+found。日志.private/system-image-p6j-build.log及WSL输出目录。类型internal-unflashable-system-image、所有刷写/恢复/发布门禁false，private_firmware_included/device_state_initialized=false。没有上传或写入棒子，不加分；不能拿此镜像跳过恢复与状态准备。

GitHub已发布2927569b94fc的只读恢复工具，前一d15cdc9保存安装实证。原未发布fd76ea1含新增CI步骤，GitHub tree接口404；读取token scopes证实无workflow。该草稿仅保存在本地codex/recovery-ci-pending分支；从未发布，不要直接推送它。main已恢复CI文件到既有版本后重制当前提交，恢复7项测试仅本机通过，不宣称CI包含新测试。无需为无关代码索取新授权，也不要换认证绕过工作流权限。

恢复准备进行中：已读旧r138资料，实际设备树只有mode-bootloader/recovery，无mode-edl，未猜测reboot edl。NAS隔离工具目录由.private/recovery-tools-location.log定位，已Root拥有/0700；此前固定bkerler-edl源码1cda1a616748d608667b2d13b73a28d3197ceb45/归档2907e7f6…及本人旧8916加载器959439aa…已上传，私有venv依赖/--help及新只读封装runtime-check通过。tools/recovery/提供固定只读gpt/read-emmc/reset及7项拒绝路径测试；源码/加载器哈希、实际USB端口+bus/address、SkipWrite、XML操作/读取边界、GPT两份CRC与精确分区门禁。不提供刷写或任意命令。首次封装拒绝上游三个内部链接、第二轮基线拒绝128条目假设；按固定源码链接和实际GPT16条目修正，原失败日志/两版封装保留。第三轮.private/recovery-readonly-runtime-third.log成功；NAS expected-device.json已记录先前独立核验全盘镜像的主备GPT摘要及当前物理端口2-1。封装SHA4a83be46…，只读主区捕获不包括boot0/1。尚未打开EDL USB、进入EDL或离线捕获，不加进度。

已异步问用户实体按键/复位孔及是否在NAS旁，明确先不要操作；等待答复期间继续独立工作。实际进入前干净关机并给明确步骤，不能猜硬件按键或把旧Windows EDL日志当当前NAS恢复已通过。没有活跃EDL等待器。内核声明源提交93a71ee9468dd8d47a9e082e5d818608f08712e3在GitHub已查到verified提交，但具体boot构建/本地音频DTB补丁的闭环还缺，board.toml的UNRESOLVED不因上游存在而静默解除。

空间维护：本机和NAS核对通用归档a73e.../132531369字节后，仅移除棒子多余tree.tar.gz传输副本；候选rootfs及全部实验/失败证据保留，根分区空闲350916608字节。NAS位置读.private/generic-transport-archive-location.json，Win/WSL原归档仍在。旧check-staged-tree-after-runtime.sh不能再直接读设备端归档，后续从已核验副本流式比较。新boot验收可加--reuse-inputs，取.private/boot-service-group-second.json中的directory，只读绑定相同签名package1..3，不删除旧数据。

2026-10-15历史检查点：70/100（P6=5.5/12）。持续目标100%，换卡流量测试及确需用户参与时才询问。每次最终回复必须报告百分比；没有当前阻塞，不应标记goal complete/blocked。

## 本轮最新结果

见VALIDATION-BOOT-GROUP-P6I.md。统一select/Core/Edge/target/一次恢复单元已经组合，并在真实棒子候选RootDirectory中以改名测试单元通过：一次选择同组签名版本、完整Core业务和首次owner、严格TLS、Root/exe inode及986/985空cap、真正Core SIGKILL后自动整组回退、owner/pin/收件箱保持、第三版显式健康确认并重启保持、第二次故障耗尽恢复预算后停止。台账tmpfs与根分区payload实际不同FS，台账没有payload；不是NSHSTATE掉电验收。

实机报告.private/boot-service-group-second.json passed/cleanup_verified=true。第一轮在启动前NAS路径解析失败，已清理并保留。自己的临时单元/规则/tmpfs已清理，旧nassimhub-agent/ModemManager/NetworkManager/rmtfs active，PCMclosed。没有活跃构建/实机测试。新增拨号/SMS/蜂窝0，累计10010仍15。

## 当前源与制品

- 主应用：9b5f1d9c0689ab278e5301e54a207b6ef0831c93，源归档19fba055e597a8d4e3b3ae504d84d5f8631d191ab843b02a5b922427ad194504；.local/wsl-artifacts-9b5f1d9c0689ab278e5301e54a207b6ef0831c93。185项及CI38752335688通过。
- 同源Native：.local/native-9b5f1d9/rustsimhub-audio-alsa，5320088字节，SHA890568f3f4664b2af6cc7db3a8d6888374534e29f0bbd09984d5f2a75895c52c；.private/native-9b5f1d9-build.log。NAS容器rustsimhub-audio-9b5f1d9 exited0，11项测试/release2m53s。Native Rust1.99.0/ALSA1.2.8与主程序本机交叉Rust1.98.0分别记录，不当性能对照。
- 不可变更新器：ed78621da68573a82d19b6b6cb130f3dad4d3208，源44efe51748ad35fea09f28787ace9df2c1660c9766420fdae028ea710611cb29；.local/wsl-artifacts-ed78621da68573a82d19b6b6cb130f3dad4d3208。187项/六程序ARM生产/组件Clippy/CI38757958385通过。主应用仍9b，同源Native不需因更新器工具变化重编。
- 默认.local/source.tar.gz现在属于ed78621/44efe5；独立source-boot-select-ed78621...tar.gz保留。不要当成当前Git HEAD。新构建必须用tools/package-source.py冻结已提交源码。
- 开发签名应用.private/boot-application-9b5f1d9/package1..3、release.public和provenance.json；release.seed只在本机private，绝不上传设备或Git。使用实际五文件应用格式。production_release=false。
- 基础系统源452d75e9f647b01ca2badb80b8e980ee99cec96b：WSL /home/wzs11/.cache/rustsimhub/base-system-p6d-build-second/output，锁定220包，树c6992751d2d246fd423c3b0680fa47d622c711ec2cab494b5ded7c8da279b7c6，不修改该输入。
- 新签名组合源f83f71901ded7c96c61cd8799b0bc3c4b1a3fe61：WSL /home/wzs11/.cache/rustsimhub/system-tree-p6i-startup-second，12335条目，树48012d10bea64d8102b3a360e409d2b5f26abcabbb9d5033086902a764607443。本轮35项板级/34项rootfs通过。初次组合因旧polkit审计拒绝，失败树system-tree-p6i-startup保留。

新组合的app_service_group_installed=true只表示单元/签名包安装。automatic_update_health_confirmation=false；flash_allowed/release_ready/recovery_verified/board_boot_verified仍false。Native ELF包含在应用payload中，但没有产品常驻媒体worker，不宣称媒体可用。

## 实机与空间

棒子上仍是旧完整系统。既有通用用户态/var/tmp/rustsimhub-system-tree-ofY1ttXm/rootfs来自724fc09，树fbcbade0f7137cec59c0eeead9ec092e2e526e70e62550e0b569c5cdc228dd9b，12303条目，不含新的启动组；实机测试把新签名应用/单元通过私有绑定接入。不要把它混称新f83整树。

原归档.private/system-tree-p6f-network.tree.tar.gz：132531369字节，SHA a73e1c0f1bb2ebb9aed764b25fe39313a42b60d36a64d732f53d13e827860cf1。运行后tar全部内容/权限/capability比较通过，.private/system-tree-post-boot-group-compare.log；完整路径清单也匹配12303项，.private/system-tree-post-boot-group-names.log，无额外文件。主机及候选都没有Python；两次尝试未执行，最终用find/sort，没有安装软件。

通用归档冗余设备副本已按首部说明移除，最后验收前空闲350916608字节，下一次必须重查。不要再上传/展开另一份400MB树或删除旧失败证据腾空间。NAS空间可存后续镜像；原始日志、SIM/短信/联系人、身份、密钥和备份仅.private/.local。测试fixture目录均保留，详细路径读各私有报告。

设备UFI003/SD410，Debian13.7 ARM64，kernel6.12.49-msm8916-g93a71ee9468d/glibc2.41。Root源码kernel_source_commit仍UNRESOLVED。boot p12 64MiB，rootfs p14 3436183552字节，NSHSTATE p15 135249408字节；准确值以实际只读预检为准，不写NV/校准/GPT。

## 下一步优先

1. 可信安装/设置交付的隔离实机层已完成，继续真实NSHSTATE每设备准备与维护入口、恢复和完整镜像；不要重复初始化测试来代替完整上电。--prepare-device-setup只准备私有码，不开窗不输出码；simhubctl identity核对socket/peer UID。不要自动删除setup.used或fallback到--serve-local-setup来重开窗口。
2. 完整系统恢复与安装：fastboot进入/只读查询/自动返回已实测，完整EDL/离线一致备份/新系统回刷尚未完成。读VALIDATION-RECOVERY-ENTRY-P0C.md和.private/full-device-live-capture.json。用户已确认需要时能到NAS旁拔插，操作前给明确步骤。
3. 旧资料../r138-flash-validation/README-r138.md确认同系列板曾在2026-10-07通过EDL写boot/rootfs、维护登录/重启；旧镜像带私人配置且版本过时，不能直接当RustSimHub已验收系统。INSTALL.md记载开机长按约5秒进入9008，必须先核对本机准确方法/programmer/备份，不能猜测或因历史成功跳过当前门禁。
4. 媒体常驻/浏览器双向声音、来电、TURN/远程；正式网页发送/删除/故障矩阵、NAS原页面/PCM；Wi-Fi其他SSID/无Wi-Fi首启/热点；稳定性、第二台、性能对照、安全和发布验收。手机原生app不做，最小网页仍在范围内。不要继续对已通过的签名组/RootDirectory重复打包自检来代替这些剩余工作。

## 持续约束与工具

- 流量测试全部等换卡并明确安排：不启用普通APN，不做蜂窝互联网探测、不隐式fallback，不关闭语音需要的WWAN/IMS。
- 持续允许10010短时自动挂断，独立once标记；Unknown先核验不自动重拨。其他号码、新短信发送不由此推定授权。
- 浏览器之前同源8448报ERR_BLOCKED_BY_CLIENT，不能换端口/接口/浏览器绕过。SSH/严格TLS程序化检查可用，GUI仍未验收。
- Python312绝对路径见环境，依赖.local/python-deps；tools/remote.py使用.private/nas.json/device.json及固定known_hosts，RejectPolicy。不要导入会在顶层执行的验收脚本来复用函数。
- Cargo都用WSL wzs11。此前Root测试污染项目target缓存，核对绝对路径后已只修复该缓存owner；Root只跑数字UID/挂载Python夹具，不再Root Cargo。通用ARM在本机交叉编译，Native目前用NAS离线缓存，绝不棒子编译。
- GitHub私有Viper-Boss/RustSimHub/main。先audit-source.py和cached diff check再提交；标准HTTPS偶有连接超时，可用tools/publish-exact-git-objects.py精确同SHA非强制发布，查远端与CI，不重写历史。
- docs/PROGRESS.md与验证文档保存历史；当前事实以本入口、固定进度表、git status及实际报告为准，不把文档提交当已重编二进制。
