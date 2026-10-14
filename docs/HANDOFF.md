# RustSimHub 当前接手入口

2026-10-14。用户最新要求“帮我把进度推到80%”，取代此前75%/60%目标；当前实证约66%（65.5点），按PROGRESS-SCALE.md固定分母估算。不要因保存小检查点停止。每次最终回复必须报告百分比。

最新：NAS Go只读桥经实际Rust Core/Edge通过，持久重放、映射/主principal撤销、generation/重启，以及真实收件箱列表/逐条详情与v2一致通过；only observing模式拒绝owner写请求，撤销NAS不影响本地owner。见VALIDATION-NAS-BRIDGE-P5D.md。原NAS页面/PCM与完整兼容尚未通过。旧库离线迁移见VALIDATION-HISTORY-P5C.md；原认证库仅NAS私有快照，禁止导入新principals/业务操作或接入原始认证库；不提交真实数据。

当前生产源码530e6f5dcd3f1510fc4057c4b7dce141669240e8，精确源包SHA256 0c14aad8c5ff9fc9790982c09baba38b761c6f2d72715903b6d231214e2c6ccc，产物.local/wsl-artifacts-530e6f5dcd3f1510fc4057c4b7dce141669240e8。本机WSL163项测试、六程序ARM64生产release、CI38565794201成功；增量release34.24秒。前版d20b9af联合报告.private/nas-compat-d20b9af-first.json，最新.private/nas-compat-530e6f5-live-first.json均passed/cleanup_verified=true。没有新增拨号/短信或蜂窝数据；旧四服务active、PCMclosed，自有测试单元停止。源码.main.go和main_test.go由NAS Go1.26.5离线容器测试/构建，适配器位置/mode/hash见私有报告及tools/nas-adapter/README.md。

## 持续约束

- 所有需要流量的实测跳过，等用户换卡后专门测试。只使用 USB/Wi-Fi；不启用普通 APN、不做互联网蜂窝探测、不隐式回退流量，不关闭语音需要的 WWAN/IMS。预算/授权模拟与只读无线状态允许。
- 用户持续允许测试拨打 10010：短时自动挂断、仅统计、独立 once/幂等键；Unknown 先核验，禁止自动重拨。累计实际 15 次。其他号码/新的短信发送不推定授权。
- 完整系统恢复仍未证明，不能写 boot/rootfs 或 NV/校准。全盘 live capture 不等于离线可恢复镜像。
- 凭据、SIM/短信/联系人、原始日志、私钥、所有备份仅 .private/ 或 .local/，不提交。读 AGENTS.md、PROGRESS.md、PROGRESS-SCALE.md、development-plan.md。

## 已实机验收

- 电话 PCM 经 Opus + ICE/DTLS/SRTP 发送 NAS 实际解码；Root 与独立非 Root Core 986/Media 984 两轮通过，静音上行、自动挂断、关闭与权限拒绝通过。见 VALIDATION-RTC-CARRIER-P3E.md / VALIDATION-ISOLATED-MEDIA-P7A.md。media_available 仍 false，网页声音/真实双向听音/来电/远程 TURN 未完成。
- 签名应用 factory/candidate/confirm，真实候选进程 SIGKILL 后回退；签名/payload/降级/换钥拒绝、身份/短信保留通过。只用私有开发签名钥匙，非生产自动启动或完整系统。见 VALIDATION-APP-UPDATE-P6A.md。
- Wi-Fi 有限试连接、8 秒 NM checkpoint，真实错误口令/无 IP 临时 profile 后 Core SIGKILL 自动恢复；USB/WWAN 无线保留、profile 删除、只读拒绝、持久防重通过。自己的临时 polkit 规则已移除。见 VALIDATION-WIFI-TRIAL-P4A.md。
- 完整 eMMC/启动区只读捕获、NAS 回读、GPT/boot 与派生副本/程序哈希核验完成。原始文件系统需处理 journal/metadata；仅派生副本修复。恢复入口未验证。见 VALIDATION-FULL-CAPTURE-P0B.md。

## 当前代码与未完工作

- GitHub 私有 https://github.com/Viper-Boss/RustSimHub，main。e927381 已验收生产五程序在 /opt/rustsimhub/dev-state-p1-e927381，source SHA 为 1db98d42495e57d74c97af9753d201f6f2b38ce3a31378276fe65ab7d33293e6。8711fba CI、ARM64 全部 141 项测试、development/production release 已通过，五程序已核验安装 /opt/rustsimhub/dev-state-p1-8711fba，source SHA 为 3ce22c188ead4597ad37b5bb1412caca31f79262c0b3d803ec4d898ef426cb85。.local/source.tar.gz 仍为 8711fba；不要当作新 HEAD 源码。
- crates/recovery 独立 age 0.11.5 离线 X25519 身份加密/空目录恢复已实机通过。NAS 持有接收者私钥；设备只拿 recipient。错 key/pin/篡改/覆盖拒绝、空目录无写入、身份 pin/原 owner 签名登录/完整收件箱响应一致通过。原目录保持；未验证整机恢复。见 VALIDATION-STATE-RECOVERY-P1C.md。
- e927381 生产首次签名初始化及实际 9→9 重启去重通过，消息 ID 顺序一致，旧 token 拒绝，重新签名登录成功。.private/firstboot-owner-e927381.json 与 sealed-identity-e927381.json 均 cleanup_verified=true。旧 9→18 是已保留历史失败证据。
- 重启观察修复和只读 key/lock 加载均在 e927381 实机通过。持久 backend 与实时 fence 分开；退休/未知重置/跨 backend/SIM 仍隔离。仅显式 create/open 允许发布 pending seed；load 不创建、以只读 fd 锁既有文件。没有 schema 升级。
- tools/test-service-group.py / device-service-group.sh / device-group-health.sh 第三轮验收通过：非 Root Core/HTTPS、仅 status.read 本地签名健康客户、实际 UID/空能力集合/挂载隔离、候选 SIGKILL 回退与已确认重启。前两次失败和修正保留；所有自有单元清理，旧四服务 active、PCM closed。见 VALIDATION-SERVICE-GROUP-P6B.md。尚无开机安装或完整媒体组。

## 下一步顺序

1. 当前共65.5点，约66%；P5累计5点。以下8f4d15b和bc33a17为早期检查点，不能替代最新530e6f5产物；80%目标保持，不在检查点停止。
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
