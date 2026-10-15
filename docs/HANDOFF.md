# RustSimHub 当前接手入口

2026-10-15。用户最新要求推进到100%，取代此前80%/75%/60%目标；换卡流量测试及确需人工参与的地方询问，其他授权工作持续推进。当前实证约69%（68.5点），按PROGRESS-SCALE.md固定分母估算。不要因保存小检查点停止。每次最终回复必须报告百分比。用户已确认需要时可协助拔插NAS上的USB，操作前明确通知。

最新板级候选层：源码724fc0922794ad08214b574b9c63bb438d808f1f组合P6D基础树和ca596a7六程序；WSL /home/wzs11/.cache/rustsimhub/system-tree-p6f-network，12303条目，树fbcbade0f7137cec59c0eeead9ec092e2e526e70e62550e0b569c5cdc228dd9b。23项隔离板级测试、systemd离线依赖检查exit0、factory实际安装路径六程序只读非Root运行通过；源基础树未变。新NM profile目录在NSHSTATE Root私有命名空间，USB profile仅种入一次、现有内容保留。见firmware/system/README.md和VALIDATION-SYSTEM-TREE-P6F.md。本轮不重复增加已计基础树/程序点数，仍68.5≈69%。

真实棒子只读state-check验收：.private/state-guard-p6f-first.json passed/cleanup_verified=true。私有mount/network namespace临时shadow路径，缺挂载拒绝、实际NSHSTATE在新目标接受、只读别名拒绝；主机挂载/旧身份摘要/目录owner未变，旧四服务active/PCMclosed，没有运行firstboot/USB/radio或新电话/SMS/蜂窝。不能把该守卫验证当新系统开机。firstboot未来会将NSHSTATE挂载根收紧Root:Root700以防旧UID复用，只创建rustsimhub-v1内目录、保留旧文件bytes/owner；回旧系统须核对其firstboot恢复旧目录owner。

当前没有活跃构建/实机进程。最终候选manifest明确flash_allowed=false、app_service_group_installed=false、native_media_installed=false、board_boot_verified=false；未出镜像/未刷机。第一份system-tree-p6f-first为6899b6a旧候选，不含NM持久化，不要混用。首次组合在写输出前因Windows CRLF对Git LF摘要不一致拒绝，修正为精确Git lock blob后成功。私有最终manifest/runtime/unit报告前缀system-tree-p6f-network；较新WSL systemd对Debian原装networkd/timesyncd两条`!!`兼容提示，不改上游包。

下一步优先：补上完整Core/Edge生产启动和可信本地enrolment/TLS/维护准备，再媒体与签名组/完整恢复。可先用新用户态RootDirectory在棒子隔离运行，不刷机；当前只读空间检查rootfs available=1134952448字节，tar/gzip存在。新树/程序可压缩复制到独立/var/tmp/rustsimhub-system-tree-XXXXXXXX，核验归档/解包后才做RootDirectory；不要删除旧测试目录腾空间或降低恢复门禁。RootDirectory必须ProtectSystem/PrivateDevices/no-new-privileges/空cap，数据用独立private bind，实际factory程序路径与应用ca596a7源hash核验，不能把主机旧libs运行称新RootFS。正式rootfs完整启动尚未通过。浏览器既有ERR_BLOCKED_BY_CLIENT边界保持，不绕过。

供下一步的通用用户态归档已经准备好，尚未上传到棒子或执行RootDirectory：.private/system-tree-p6f-network.tree.tar.gz，以及WSL /home/wzs11/.cache/rustsimhub/system-tree-p6f-network.tree.tar.gz，132531369字节，sha a73e1c0f1bb2ebb9aed764b25fe39313a42b60d36a64d732f53d13e827860cf1；元数据.private/system-tree-p6f-package.json。打包前重新核对最终树，保留numeric owner/权限/链接/capability xattr，不含设备私有数据；仍不允许刷机。上传前重查空间，只在新私有目录解包并核验全部归档内容。55f6800 CI38741954827 success（现有Rust177/rootfs33/板级23）。首次git push连接超时，HTTP1.1有界正常重试后远端已核对为55f6800，无强推；所有本轮进程均已结束。

最新组合首启运行模式：ca596a79acd43f968caa749172bd1e6f72fdac07增加--serve-device及LocalSetup::for_device：私有setup.code显式供给才开窗，身份绑定setup.used在入网前CREATE_NEW+fsync，真实进程退出/崩溃不能刷新窗口，已有principals无code也封闭，旧身份+新DB不能重开；损坏/外设备/链接标记拒绝。见device-first-owner.md和VALIDATION-DEVICE-FIRST-OWNER-P6E.md。177项本机测试/lint、六程序ARM64生产构建和CI38734877173 success。源归档8bdf8fcb6b9ebce139a8c0c4fd8de742fd3a17b8690d729f3506c0bbdd999f52，.local/source-device-first-owner-ca596a79acd43f968caa749172bd1e6f72fdac07.tar.gz，产物.local/wsl-artifacts-ca596a79acd43f968caa749172bd1e6f72fdac07。默认source.tar.gz已恢复8711fba归属。

实机.private/device-first-owner-ca596a7-first.json passed/cleanup_verified=true：未认领域真实SIGKILL后同pin、窗口关闭/正确码拒绝，域完整保留；另建明确独立测试身份，同一Core PID签名初始化owner后直接使用完整业务。删除code后再次真实SIGKILL，owner/pin/收件箱保持、窗口不重开，有限Wi-Fi回退/保存/重连和空历史保持通过；真实UID986/985与所有Cap集合零。自有单元/profile/规则已清理、旧四服务active/PCMclosed、checkpoint0/WWAN保持。没有新电话/SMS/蜂窝，累计10010仍15。P6加0.5至4，总体68.5≈69%。不能据此称首启GUI/证书、生产开机安装、板级overlay或完整刷机已完成。

当前没有活跃构建/实机测试。初次本机构建入口私有shell CRLF在构建前退出，规范LF后同一源包构建成功，.private/wsl-build-ca596a79acd43f968caa749172bd1e6f72fdac07.exit=0；增量release37.34秒，不是全量对照。验收脚本新增实际Edge UID Identity IPC就绪判断及新SIGKILL步骤有限停机等待。下一步仍是板级启动/持久分区/正式首启与生产服务组、媒体及完整恢复；不要把--serve-local-setup用作自动fallback来重开已封闭窗口。

最新基础系统：见VALIDATION-BASE-SYSTEM-P6D.md，P6加0.5至3.5/12。已提交tools/rootfs/、来源/许可和firmware/base-system.lock.json，220包经实际bootstrap及签名索引验证；锁sha bb040be5491f6098b73c098110671c7bf2f89be46f9942bc342b5070b67e8190，包集合sha cdd70c5a3c412e9ff6d5a7b9bda8ffc03e68c1620f57bb17a084f74d93a00ada。工具源码452d75e9f647b01ca2badb80b8e980ee99cec96b冻结构建成功（.private/base-system-p6d-build-second.exit=0），实际220包完全匹配，账户/残留审计PASS。WSL /home/wzs11/.cache/rustsimhub/base-system-p6d-build-second/output，树c6992751d2d246fd423c3b0680fa47d622c711ec2cab494b5ded7c8da279b7c6，归档349624320字节/sha f1334e41580cd4a27404aabb896b3f4805ea3ffaffafd5b75958e93b95a07216。不得修改基础输入树；下一层复制/派生再集成。

33项rootfs核心工具测试通过；首轮6291d9a因审计仍查旧账户名拒绝，output.FAILED保留。修复后第二轮完整成功，两次独立bootstrap清理后的树实际相同、差异0，不声称两份最终归档比较或跨日期复现。新tools/test-base-runtime.py在独立mount/network namespace、只读树/制品、986:986运行六个cd4c96e生产程序build-info/Core self-check/CLI help通过，.private/base-system-p6d-runtime-first.json passed=true/base_tree_unchanged=true；source归属仍69a2450c...。没有实际硬件/D-Bus/网络访问，没有新编译Rust二进制或把新文档提交当成应用源码版本。

所有本机rootfs构建/运行进程已结束，没有待轮询构建。452d75e CI38732828672 success（rootfs33项及现有Rust workspace/production/lint）。两次Git对象API在tree步骤404，父树读取正常；标准git push已把6291d9a/452d75e成功同步，遇同问题优先标准push，不强推。下面私有kit准备条目为过程记录，当前正式工具以tools/rootfs和实际452d75e制品为准。继续板级overlay、持久分区/首启/服务组和媒体，不因基础输入已构建就生成可刷包；full recovery仍未证明。

最新恢复入口：一次reboot bootloader，NAS对同物理USB端口的唯一fastboot设备读product/大小，自动reboot返回；三分区大小匹配正常系统，boot ID改变，boot哈希/旧身份保持，USB/Wi-Fi、旧四服务与PCMclosed恢复，未新拨号/短信/蜂窝测试。见VALIDATION-RECOVERY-ENTRY-P0C.md，P0加1至6/8，共67.5点。私有recovery-entry-20261015-second.json passed=true；first只读预检无Python失败，未重启。fastboot_entry_verified=true单独记录，recovery_verified/release_ready仍false；EDL、离线一致捕获/完整回刷仍未证明。无需人工拔插。

系统基础候选准备：复用AirSimHub ea019485e5d890a96acae357c52b94c746de837f的私有构建工具fork，.local/rootfs-kit-p6d/reuse-provenance.json记录每文件原始SHA；品牌改RustSimHub，包集增加维护openssh-server与校时，省略Bluetooth，可选dnsmasq-base仅准备依赖，不称热点已通过。32项离线核心测试通过，旧modules归档703模块vermagic全匹配，归档展开的全部文件摘要与当前棒子1552358...清单一致；boot使用.private/backups/boot.img的d5790e...完整64MiB。实际解析在本机WSL root运行，目标/home/wzs11/.cache/rustsimhub/base-system-p6d-20261015-c，.private/base-system-p6d-resolve-c.log/.exit，依赖锁.private/base-system-p6d.lock.json。先前两轮缺少公开自测指纹/imagepolicy依赖，在bootstrap前失败，日志保留。不要把旧r134基础缓存或构建中的新锁称完整Rust镜像。新完整系统仍需服务组/首启、硬件overlay、实际介质和恢复验收。

最新集成：182a6ec共用--serve-platform完成真实Wi-Fi/SMS/历史/NAS并行及拨号互斥/进程终止恢复；cd4c96e1df09c598438e088e0e1b5add20bb99e8增加空历史init，无旧NAS/合成数据、无NAS principal的组合Core实机通过。源hash69a2450c5419844975ed1960efd2c27900c444e870a393b93672e4c7aeaa2fa7，产物.local/wsl-artifacts-cd4c96e1df09c598438e088e0e1b5add20bb99e8；174项本机测试/lint与六程序ARM64生产构建通过。两轮报告.private/platform-182a6ec-first.json、platform-cd4c96e-empty-first.json均passed/cleanup_verified=true，见VALIDATION-PLATFORM-P6C.md。原Wi-Fi/WWAN保留，自己的配置/规则/服务均清理，旧四服务active、PCMclosed，新增拨号/短信/蜂窝测试0。通话累计仍15。集成完善不重复加点。

下一步重点转向完整系统候选与生产启动/更新组、媒体交付，保留recovery_verified=false/UNRESOLVED刷写门禁。新init是空归档原子发布，不是整机/开机安装；NAS只是验收客户端，不声称NAS关闭/拔USB/物理上电已经通过。旧Wi-Fi检查点以下为历史记录。上轮自动审批额度失败未执行WSL命令，2026-10-15查询ordinaryUsageAllowed=true后按同一审批流程构建完成，无当前该阻塞。

最新Wi-Fi：9b5f6b07762f7d9dc727f81f92ed21ea19f73467新增owner确认的新配置。源归档72215080d62106507534294a5f6cf58cfeed45ba754a9c7ae77efc22de5ae975，产物.local/wsl-artifacts-9b5f6b07762f7d9dc727f81f92ed21ea19f73467；170项本机测试/lint、六程序ARM64生产构建与CI38570475237成功。真实新profile连接/取消、确认持久保存、Core退出后NM自行重连、错误PSK真实SIGKILL恢复及重启防重通过，.private/wifi-provision-9b5f6b0-second.json passed/cleanup_verified=true；第一轮NAS shell路径解析失败未启动测试服务，保留。见VALIDATION-WIFI-PROVISION-P4B.md。本轮现有SSID，同完整媒体/历史/NAS Core组合、无旧Wi-Fi首启、其他SSID和热点仍待验收。自己的profile/规则/服务已清理，原profile/WWAN保持，旧四服务active、PCMclosed，通话次数仍15。下一步组合Core网络接线与并发控制，再推进完整系统和媒体剩余项。

最新：NAS Go只读桥经实际Rust Core/Edge通过，持久重放、映射/主principal撤销、generation/重启，以及真实收件箱列表/逐条详情与v2一致通过；only observing模式拒绝owner写请求，撤销NAS不影响本地owner。见VALIDATION-NAS-BRIDGE-P5D.md。原NAS页面/PCM与完整兼容尚未通过。旧库离线迁移见VALIDATION-HISTORY-P5C.md；原认证库仅NAS私有快照，禁止导入新principals/业务操作或接入原始认证库；不提交真实数据。

此前NAS桥检查点530e6f5dcd3f1510fc4057c4b7dce141669240e8，精确源包SHA256 0c14aad8c5ff9fc9790982c09baba38b761c6f2d72715903b6d231214e2c6ccc，产物.local/wsl-artifacts-530e6f5dcd3f1510fc4057c4b7dce141669240e8。本机WSL163项测试、六程序ARM64生产release、CI38565794201成功；增量release34.24秒。前版d20b9af联合报告.private/nas-compat-d20b9af-first.json，随后.private/nas-compat-530e6f5-live-first.json均passed/cleanup_verified=true。没有新增拨号/短信或蜂窝数据；旧四服务active、PCMclosed，自有测试单元停止。源码.main.go和main_test.go由NAS Go1.26.5离线容器测试/构建，适配器位置/mode/hash见私有报告及tools/nas-adapter/README.md。

正反对照.private/platform-cd4c96e-empty-positive.json passed/cleanup_verified=true、valid_call_prepare_allowed_only_after_network_restored_no_submit=true：实际有效line在配网期间拒绝准备，恢复后只准备成功、不提交，active_calls=0。三份本轮报告都保持无实际新拨号/短信/蜂窝流量。cd4c96e CI38726463632 success；所有临时服务均停止，无待轮询构建/实机进程。

## 持续约束

- 所有需要流量的实测跳过，等用户换卡后专门测试。只使用 USB/Wi-Fi；不启用普通 APN、不做互联网蜂窝探测、不隐式回退流量，不关闭语音需要的 WWAN/IMS。预算/授权模拟与只读无线状态允许。
- 用户持续允许测试拨打 10010：短时自动挂断、仅统计、独立 once/幂等键；Unknown 先核验，禁止自动重拨。累计实际 15 次。其他号码/新的短信发送不推定授权。
- fastboot入口/查询/自动返回已证明；完整系统恢复仍未证明，不能写 boot/rootfs 或 NV/校准。全盘 live capture 不等于离线可恢复镜像。
- 凭据、SIM/短信/联系人、原始日志、私钥、所有备份仅 .private/ 或 .local/，不提交。读 AGENTS.md、PROGRESS.md、PROGRESS-SCALE.md、development-plan.md。

## 已实机验收

- 电话 PCM 经 Opus + ICE/DTLS/SRTP 发送 NAS 实际解码；Root 与独立非 Root Core 986/Media 984 两轮通过，静音上行、自动挂断、关闭与权限拒绝通过。见 VALIDATION-RTC-CARRIER-P3E.md / VALIDATION-ISOLATED-MEDIA-P7A.md。media_available 仍 false，网页声音/真实双向听音/来电/远程 TURN 未完成。
- 签名应用 factory/candidate/confirm，真实候选进程 SIGKILL 后回退；签名/payload/降级/换钥拒绝、身份/短信保留通过。只用私有开发签名钥匙，非生产自动启动或完整系统。见 VALIDATION-APP-UPDATE-P6A.md。
- Wi-Fi 有限试连接、8 秒 NM checkpoint，真实错误口令/无 IP 临时 profile 后 Core SIGKILL 自动恢复；USB/WWAN 无线保留、profile 删除、只读拒绝、持久防重通过。自己的临时 polkit 规则已移除。见 VALIDATION-WIFI-TRIAL-P4A.md。
- 完整 eMMC/启动区只读捕获、NAS 回读、GPT/boot 与派生副本/程序哈希核验完成。原始文件系统需处理 journal/metadata；仅派生副本修复。见 VALIDATION-FULL-CAPTURE-P0B.md；最新P0C已验证fastboot入口，完整离线恢复仍未验证。

## 当前代码与未完工作

- GitHub 私有 https://github.com/Viper-Boss/RustSimHub，main。e927381 已验收生产五程序在 /opt/rustsimhub/dev-state-p1-e927381，source SHA 为 1db98d42495e57d74c97af9753d201f6f2b38ce3a31378276fe65ab7d33293e6。8711fba CI、ARM64 全部 141 项测试、development/production release 已通过，五程序已核验安装 /opt/rustsimhub/dev-state-p1-8711fba，source SHA 为 3ce22c188ead4597ad37b5bb1412caca31f79262c0b3d803ec4d898ef426cb85。.local/source.tar.gz 仍为 8711fba；不要当作新 HEAD 源码。
- crates/recovery 独立 age 0.11.5 离线 X25519 身份加密/空目录恢复已实机通过。NAS 持有接收者私钥；设备只拿 recipient。错 key/pin/篡改/覆盖拒绝、空目录无写入、身份 pin/原 owner 签名登录/完整收件箱响应一致通过。原目录保持；未验证整机恢复。见 VALIDATION-STATE-RECOVERY-P1C.md。
- e927381 生产首次签名初始化及实际 9→9 重启去重通过，消息 ID 顺序一致，旧 token 拒绝，重新签名登录成功。.private/firstboot-owner-e927381.json 与 sealed-identity-e927381.json 均 cleanup_verified=true。旧 9→18 是已保留历史失败证据。
- 重启观察修复和只读 key/lock 加载均在 e927381 实机通过。持久 backend 与实时 fence 分开；退休/未知重置/跨 backend/SIM 仍隔离。仅显式 create/open 允许发布 pending seed；load 不创建、以只读 fd 锁既有文件。没有 schema 升级。
- tools/test-service-group.py / device-service-group.sh / device-group-health.sh 第三轮验收通过：非 Root Core/HTTPS、仅 status.read 本地签名健康客户、实际 UID/空能力集合/挂载隔离、候选 SIGKILL 回退与已确认重启。前两次失败和修正保留；所有自有单元清理，旧四服务 active、PCM closed。见 VALIDATION-SERVICE-GROUP-P6B.md。尚无开机安装或完整媒体组。

## 下一步顺序

1. 当前共68.5点，约69%；P0 6点/P4 7点/P5 5点/P6 4点。以下8f4d15b和bc33a17为早期检查点，不能替代最新ca596a7应用或452d75e基础输入；100%目标保持，不在检查点停止。
   8f4d15b CI 38519741620 success；bc33a17 CI 38522521958 success。源头与产物分别核验，不能把文档提交当作新编译结果。
2. 持久 rustsimhub-history-build-p5 已 exited 0，label revision=8f4d15b70ee7ac380d0404cdc770e05a3e298fd8。NAS /mnt/docker-data/rustsimhub/p0/history-build-Cqi2Xu3q，完整日志 build.log/.private/history-async-complete-8f4d15b.log；十项测试及 release 完成。
   source SHA 0df653f7810727d1d56ab6da3257bc2a0a09ddcf41a6999a058393750cf6eaff，.local/source-history-p5-private.tar.gz 匹配 8f4d15b；.local/source.tar.gz 仍 8711fba。migrate SHA 9c88aea0f46b0f24a780edd239a4089504458d1957fbe81b36d8fb804513e14d，.local/artifacts/8f4d15b/rustsimhub-migrate，棒子 /opt/rustsimhub/dev-history-p5-8f4d15b/rustsimhub-migrate。
   完整旧 schema 合成快照在 NAS/非 Root 隔离棒子各首次插入15、再次0/unchanged15、源文件不变。独立 Python 确认原 ID/删除时间/外发历史状态/每用户标记/联系人归属保留，无 principal 或执行队列表。.private/history-full-schema-8f4d15b-first.json passed=true、pcm_closed_and_test_units_stopped=true。见 VALIDATION-HISTORY-P5A.md。真实旧库/不同版本和授权网页历史查询仍未证明。
   前两次编译/私有测试目录失败已修正并保留；临时 run-3 的 mtime=0 缓存旧测试排除，foreground run-4 的 SSH 超时后确认容器 collected，未冒称成功，随后改用持久构建。每个复用工作区的源码归档必须 tar --touch；不因观察超时重启同一活跃构建。
3. .private/status-performance-e927381-first.json passed/cleanup_verified=true。200 次本地 CLI+IPC 状态请求成功，P95 15.949 ms，Core+Edge 测后 RSS 约10.43 MiB，实际 UID/空能力集合/越权拒绝通过；不是网络延迟、等功能速度对照或长期稳定。见 VALIDATION-STATUS-BASELINE-P7B.md。
4. 继续授权历史读取/分页与真实旧库副本评估、NAS 在线兼容及限权会话/重放账本/PCM 桥、开机安装/完整服务组、Wi-Fi 新配置/热点、媒体交付和长期验收。不要把纯签名契约库或离线归档称为正在运行的 NAS 网关。
5. LocalSetup（local-first-owner.md）仅 --serve-local-setup、空 principals、Core 私有随机码、5分钟/16次及签名条件启用。8711fba 生产 HTTPS API 已通过，.private/local-owner-api-8711fba-second.json passed=true；错误码/角色/签名/重放/替换 owner 拒绝，正确初始化后登录通过，无原始数据或硬件访问。
   prepare-browser-owner.py 使用新空库、PrivateDevices 和禁止 /run/dbus。实际浏览器页在载入前 net::ERR_BLOCKED_BY_CLIENT，未绕过或更换入口；GUI、IndexedDB/reload 与证书交付仍未验收。所有自有代理/测试单元已停止，旧四服务 active、PCM closed。不要重试旧过期 setup realm。
   0a8c107 app.js 状态部分失败时仍显示其他状态的修复未进入8711fba产物，不能混淆本地 source UI 和已安装 release。手机原生 app 延后；所有流量实测等待用户换卡后专门测试。完整镜像/恢复入口门禁保持。

## 工具与资料位置

- 本地 C:/Users/wzs11/Documents/ChatGPT/大疆4G模块/RustSimHub，PowerShell；Python 中文文件操作用 -X utf8 或显式 encoding='utf-8'。Windows 缺 MSVC linker；已打通本机 Ubuntu WSL 的 Linux 测试/ARM64 交叉编译，见 local-cross-build.md 与 VALIDATION-LOCAL-CROSS-P7C.md。NAS 保留 ARM 验证与原生音频构建。
- .private/nas.json 与 device.json；后者通过 NAS jump，固定 maintenance key 与 known_hosts，RejectPolicy，不关闭主机校验。.local/python-deps 是 Paramiko。
- tools/remote.py --config ... --script ... --output .private/... [NAS --sudo]；SFTP upload/download。密码只经 stdin。
- NAS /mnt/docker-data/rustsimhub/p0，自有 rustsimhub-build-p0 / rustsimhub-audio-build-p3。重建只在 exited 后保存证据，核对 label/image/mount，仅 rm 自有 container、不加 -v。source archive 打包允许列表精确 Git blob，build-input dirty 即拒绝。
- tools/verify-build-source.py 核对归属。生产/开发产物分 target/production 与 target/development，production core fixtures=false；不在两个模式间覆盖 release 欺骗缓存。部署核对文件 hash 和每个 --build-info source hash。
- Git HTTP 有间歇断线。tools/publish-exact-git-objects.py 经官方 API 发布同一 Git blob/tree/commit，逐个 SHA 必须匹配，远端未并发变化且 force=false；不改历史，不上传 private。
- 当前棒子已验证程序在 /opt/rustsimhub/dev-wifi-p4、dev-rtc-p3、dev-rtc-core-p3、dev-app-update-p6；独立锁定 Core/Edge/Media 账户。无新生产自启服务，已完的临时服务/权限均清理。旧四服务最后检查 active、PCM closed；新的会话先核实实际状态。
- 历史所有阶段的完整边界、hash 与测试见各 VALIDATION 文档；源头许可见 NOTICE.md。完整第三方发布 notices、完整镜像、恢复入口仍未完成。

## 中断后工作区检查（2026-10-14）

历史读取/授权已在a5e4eb7实现、接线并通过157项本机测试/lint、ARM生产构建、CI38547240598与组合Core/HTTPS实测。工作区旧WIP已提交，不再需要重做接线。见history-query.md与VALIDATION-HISTORY-P5B.md。新产物.local/wsl-artifacts-a5e4eb7f0f38581915fb9124160fed1b06f4f584，源hash7f7de34cb6278521e80644ae9d8f606bacc613811a8c5fd5f4a194a6e7859d17；.private/history-api-a5e4eb7-fifth.json passed/cleanup_verified=true，原始数据/硬件访问均关闭。当前约64%；下一步真实旧NAS库边界与NAS在线兼容、音频/网络/系统安装剩余工作。

2026-10-14 本机构建更新：稳定8f4d15b的151项测试/六程序ARM自检先通过，历史查询a5e4eb7现在也用同一流程通过。Rust/下载包在.local/wsl-cross，GNU工具链及target在WSL用户~/.cache/rustsimhub；/tmp是易失tmpfs。原先WIP现在已包含在a5e4eb7归档与产物。构建优化不增加进度点数。保留.source.tar.gz原有8711fba归属、独立命名的新源归档、旧已验收部署和失败证据。
