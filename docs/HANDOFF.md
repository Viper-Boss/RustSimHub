# RustSimHub 当前接手入口

2026-10-14。用户最新要求“帮我把进度推到80%”，取代此前 75%/60% 目标；当前实证约 63%（62.5 点），按 PROGRESS-SCALE.md 固定分母估算。不要因保存小检查点停止。每次最终回复必须报告百分比。

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

最新：持久 `rustsimhub-history-build-p5` exited 0，完整日志保留 `.private/history-async-complete-8f4d15b.log` 与 NAS build-Cqi2Xu3q/build.log，ARM64 十项测试/release 全通过。产物 SHA 9c88aea0f46b0f24a780edd239a4089504458d1957fbe81b36d8fb804513e14d，.local/artifacts/8f4d15b/rustsimhub-migrate；棒子 /opt/rustsimhub/dev-history-p5-8f4d15b/rustsimhub-migrate。tools/test-history-migration.py 完整旧 schema 合成导入实际通过，.private/history-full-schema-8f4d15b-first.json passed=true/pcm_closed_and_test_units_stopped=true。NAS 与棒子各首次 15、再次 0/unchanged15、源快照不变，独立查询标记/归属及无执行队列通过。P5 当前 2 点，合计 62.5，约 63%。先前下面“构建进行中/待验收/P5 1 点”均为已保留过程；不要重复旧尝试。
优先继续：授权历史读取/分页和真实旧库副本评估、NAS 在线兼容与限权会话、开机安装/完整服务组、Wi-Fi 新配置/热点及媒体交付。当前仍不是完整系统或 NAS 在线交付，80% 目标保持。

0. 8f4d15b 完整 CI success（38519741620），包括离线历史归档五项关键失败路径。P5 当前计 1 点为原 Go 黄金向量；P7 状态基线加半点，合计 61.5。`.private/status-performance-e927381-first.json` passed/cleanup_verified=true；200 次本地 CLI+IPC 全通过、P95 15.949 ms，Core+Edge RSS 约 10.43 MiB；不是等功能速度对照或长期稳定。见 VALIDATION-STATUS-BASELINE-P7B.md。
   正在运行 `rustsimhub-history-build-p5`，label revision=8f4d15b70ee7ac380d0404cdc770e05a3e298fd8，NAS /mnt/docker-data/rustsimhub/p0/history-build-Cqi2Xu3q。持久离线容器保留终态，不因观察超时重启。工具 nas-start-history-build.sh；之前临时 foreground run-4 的 SSH 超时后旧容器已 collected，未当作成功 release；run-3 因 mtime=0 缓存旧测试拒绝计证。新构建 tar --touch，输入 source SHA 0df653f7810727d1d56ab6da3257bc2a0a09ddcf41a6999a058393750cf6eaff，.local/source-history-p5-private.tar.gz 匹配 8f4d15b。.local/source.tar.gz 仍 8711fba。
   等上述容器 exited 0，保存 docker logs 与 build-info/文件 hash，取回 rustsimhub-migrate；然后 tools/test-history-migration.py --nas --device --binary --archive --revision 8f4d15b --legacy-schema ../nassimhub/internal/platform/database/schema.sql --report 新私有 JSON。完整旧 schema 合成构造/外键检查已本地通过；工具最新修正尚需提交。第一次实现 SQLite u64/usize FromSql 编译失败由 i64 检查转换修正，首次测试私有目录权限由显式 0700 修正，失败证据保留。不能将此归档宣称为真实旧库迁移或已运行 NAS 网关；后续接入授权查询与在线兼容。

1. 保存新的验收工具/文档及 GitHub 进度，核实源码 CI。已完成的 firstboot/recovery/group 私有报告不能当新的未知硬件操作重试。
   LocalSetup 门禁（local-first-owner.md）：仅 --serve-local-setup、空 principals、Core 私有随机码、5 分钟/16 次、签名条件启用。8711fba 生产 HTTPS API 已通过，.private/local-owner-api-8711fba-second.json passed=true；错误码/角色/签名/重放/替换 owner 拒绝，正确初始化后签名登录通过，无原始数据/硬件访问。实际浏览器仍未通过，不加点。ee94e85 的 await CI 失败已由 8711fba runtime.block_on 修复。
   prepare-browser-owner.py 使用新空库、PrivateDevices 和 /run/dbus 禁止访问，不读取原始短信/电话或其他业务数据。浏览器 UI 验收先限定这个无敏感数据、无硬件权限的测试域。app.js 新增单项状态不可用时仍显示其他状态的修复，尚未进入 8711fba 产物；不能混淆本地 source UI 与已安装 release。
   浏览器本地 QA 页在载入前 net::ERR_BLOCKED_BY_CLIENT，未绕过或更换入口；GUI、IndexedDB/reload 仍未验证。自有代理和离线测试单元已停止，旧四服务 active、PCM closed，.private/local-owner-api-cleanup.log 保留。不要重试旧的过期 setup realm。
   crates/compat 新增协议原语和当前 NAS principal 的只读 scope 映射；不是已启用网关。5 组签名/标识与 12 组版本向量由精确旧提交的 Go 模块在 NAS network=none 容器生成。f536c63 NAS ARM64 五项测试全通过，CI 全工作区测试/生产 feature 通过，首次 Clippy 两处建议已修复，需核对修复提交 CI。NAS 无 Clippy，测试报告 .private/compat-rust-run-1.log；.local/source-f536c63.tar.gz 仅该 lib 测试来源，现有 .local/source.tar.gz 已还原为 8711fba。后续做独立只读适配器/时间重放/撤销与迁移；P5 暂仍 0。见 nas-compat-contract.md。
2. 继续正式首启网页/TLS 交付、开机启动、NAS 兼容、网页写/媒体、新配网和长期验收。手机原生 app 延后。
3. 目标约 80%，不要在 60/75 的小检查点停止；流量实测跳过到换卡后专门安排，完整镜像与恢复入口门禁保持。

## 工具与资料位置

- 本地 C:/Users/wzs11/Documents/ChatGPT/大疆4G模块/RustSimHub，PowerShell；Python 中文文件操作用 -X utf8 或显式 encoding='utf-8'。Windows 缺 MSVC linker，Linux 检查在 NAS/CI。
- .private/nas.json 与 device.json；后者通过 NAS jump，固定 maintenance key 与 known_hosts，RejectPolicy，不关闭主机校验。.local/python-deps 是 Paramiko。
- tools/remote.py --config ... --script ... --output .private/... [NAS --sudo]；SFTP upload/download。密码只经 stdin。
- NAS /mnt/docker-data/rustsimhub/p0，自有 rustsimhub-build-p0 / rustsimhub-audio-build-p3。重建只在 exited 后保存证据，核对 label/image/mount，仅 rm 自有 container、不加 -v。source archive 打包允许列表精确 Git blob，build-input dirty 即拒绝。
- tools/verify-build-source.py 核对归属。生产/开发产物分 target/production 与 target/development，production core fixtures=false；不在两个模式间覆盖 release 欺骗缓存。部署核对文件 hash 和每个 --build-info source hash。
- Git HTTP 有间歇断线。tools/publish-exact-git-objects.py 经官方 API 发布同一 Git blob/tree/commit，逐个 SHA 必须匹配，远端未并发变化且 force=false；不改历史，不上传 private。
- 当前棒子已验证程序在 /opt/rustsimhub/dev-wifi-p4、dev-rtc-p3、dev-rtc-core-p3、dev-app-update-p6；独立锁定 Core/Edge/Media 账户。无新生产自启服务，已完的临时服务/权限均清理。旧四服务最后检查 active、PCM closed；新的会话先核实实际状态。
- 历史所有阶段的完整边界、hash 与测试见各 VALIDATION 文档；源头许可见 NOTICE.md。完整第三方发布 notices、完整镜像、恢复入口仍未完成。
