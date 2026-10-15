# RustSimHub 当前接手入口

2026-10-15。持续目标100%，换卡流量测试及确需用户参与时才询问。当前按PROGRESS-SCALE.md固定分母为70/100（P6=5.5/12），不是完成后停工。每次最终回复必须报告百分比。上一轮有实际源码、构建及实机进展；没有当前阻塞，不应标记goal complete/blocked。

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

当前根分区可用约226MB，下一次必须重查。不要再上传/展开另一份400MB树或删除旧失败证据腾空间。NAS空间可存后续镜像；原始日志、SIM/短信/联系人、身份、密钥和备份仅.private/.local。测试fixture目录均保留，详细路径读各私有报告。

设备UFI003/SD410，Debian13.7 ARM64，kernel6.12.49-msm8916-g93a71ee9468d/glibc2.41。Root源码kernel_source_commit仍UNRESOLVED。boot p12 64MiB，rootfs p14 3436183552字节，NSHSTATE p15 135249408字节；准确值以实际只读预检为准，不写NV/校准/GPT。

## 下一步优先

1. 完成可信安装/首次设置交付：持久Edge TLS/config、设置码与pin/证书的可信本地交付、真实维护公钥安装。现有--prepare-device-setup只准备私有码，不开窗不输出码；simhubctl identity核对socket/peer UID。不要自动删除setup.used或fallback到--serve-local-setup来重开窗口。
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
