# RustSimHub 当前接手入口

2026-10-14。用户最新目标约 75%，取代旧目标约 60%；当前实证约 60%，按 PROGRESS-SCALE.md 固定分母估算。不要因保存小检查点停止。每次最终回复必须报告百分比。

## 持续约束

- 当前 SIM 无流量，蜂窝数据实测最后。只使用 USB/Wi-Fi；不启用普通 APN、不做互联网蜂窝探测、不隐式回退流量，不关闭语音需要的 WWAN/IMS。预算/授权模拟与只读无线状态允许。
- 用户持续允许测试拨打 10010：短时自动挂断、仅统计、独立 once/幂等键；Unknown 先核验，禁止自动重拨。累计实际 15 次。其他号码/新的短信发送不推定授权。
- 完整系统恢复仍未证明，不能写 boot/rootfs 或 NV/校准。全盘 live capture 不等于离线可恢复镜像。
- 凭据、SIM/短信/联系人、原始日志、私钥、所有备份仅 .private/ 或 .local/，不提交。读 AGENTS.md、PROGRESS.md、PROGRESS-SCALE.md、development-plan.md。

## 已实机验收

- 电话 PCM 经 Opus + ICE/DTLS/SRTP 发送 NAS 实际解码；Root 与独立非 Root Core 986/Media 984 两轮通过，静音上行、自动挂断、关闭与权限拒绝通过。见 VALIDATION-RTC-CARRIER-P3E.md / VALIDATION-ISOLATED-MEDIA-P7A.md。media_available 仍 false，网页声音/真实双向听音/来电/远程 TURN 未完成。
- 签名应用 factory/candidate/confirm，真实候选进程 SIGKILL 后回退；签名/payload/降级/换钥拒绝、身份/短信保留通过。只用私有开发签名钥匙，非生产自动启动或完整系统。见 VALIDATION-APP-UPDATE-P6A.md。
- Wi-Fi 有限试连接、8 秒 NM checkpoint，真实错误口令/无 IP 临时 profile 后 Core SIGKILL 自动恢复；USB/WWAN 无线保留、profile 删除、只读拒绝、持久防重通过。自己的临时 polkit 规则已移除。见 VALIDATION-WIFI-TRIAL-P4A.md。
- 完整 eMMC/启动区只读捕获、NAS 回读、GPT/boot 与派生副本/程序哈希核验完成。原始文件系统需处理 journal/metadata；仅派生副本修复。恢复入口未验证。见 VALIDATION-FULL-CAPTURE-P0B.md。

## 当前代码与未完工作

- GitHub 私有 https://github.com/Viper-Boss/RustSimHub，main。e927381 的 CI、ARM64 138 项测试和两组 release 通过；精确 source SHA 为 1db98d42495e57d74c97af9753d201f6f2b38ce3a31378276fe65ab7d33293e6。生产五程序已核验并部署到 /opt/rustsimhub/dev-state-p1-e927381，自有构建容器 exited 0。
- crates/recovery 独立 age 0.11.5 离线 X25519 身份加密/空目录恢复已实机通过。NAS 持有接收者私钥；设备只拿 recipient。错 key/pin/篡改/覆盖拒绝、空目录无写入、身份 pin/原 owner 签名登录/完整收件箱响应一致通过。原目录保持；未验证整机恢复。见 VALIDATION-STATE-RECOVERY-P1C.md。
- e927381 生产首次签名初始化及实际 9→9 重启去重通过，消息 ID 顺序一致，旧 token 拒绝，重新签名登录成功。.private/firstboot-owner-e927381.json 与 sealed-identity-e927381.json 均 cleanup_verified=true。旧 9→18 是已保留历史失败证据。
- 重启观察修复和只读 key/lock 加载均在 e927381 实机通过。持久 backend 与实时 fence 分开；退休/未知重置/跨 backend/SIM 仍隔离。仅显式 create/open 允许发布 pending seed；load 不创建、以只读 fd 锁既有文件。没有 schema 升级。
- tools/test-service-group.py / device-service-group.sh / device-group-health.sh 第三轮验收通过：非 Root Core/HTTPS、仅 status.read 本地签名健康客户、实际 UID/空能力集合/挂载隔离、候选 SIGKILL 回退与已确认重启。前两次失败和修正保留；所有自有单元清理，旧四服务 active、PCM closed。见 VALIDATION-SERVICE-GROUP-P6B.md。尚无开机安装或完整媒体组。

## 下一步顺序

1. 保存新的验收工具/文档及 GitHub 进度，核实源码 CI。已完成的 firstboot/recovery/group 私有报告不能当新的未知硬件操作重试。
   新增 LocalSetup 门禁与网页首个 owner 正在实现（local-first-owner.md）：只在 --serve-local-setup、空 principals、Core 私有随机码、5 分钟/16 次、设备/浏览器签名条件下启用；普通 IPC 默认拒绝。WebCrypto 新 owner proof 已通过，Linux CI/ARM64/实际浏览器待验证，不加点。
2. 继续正式首启网页/TLS 交付、开机启动、NAS 兼容、网页写/媒体、新配网和长期验收。手机原生 app 延后。
3. 目标仍约 75%，不要在 60 的小检查点停止；普通蜂窝流量最后，完整镜像与恢复入口门禁保持。

## 工具与资料位置

- 本地 C:/Users/wzs11/Documents/ChatGPT/大疆4G模块/RustSimHub，PowerShell；Python 中文文件操作用 -X utf8 或显式 encoding='utf-8'。Windows 缺 MSVC linker，Linux 检查在 NAS/CI。
- .private/nas.json 与 device.json；后者通过 NAS jump，固定 maintenance key 与 known_hosts，RejectPolicy，不关闭主机校验。.local/python-deps 是 Paramiko。
- tools/remote.py --config ... --script ... --output .private/... [NAS --sudo]；SFTP upload/download。密码只经 stdin。
- NAS /mnt/docker-data/rustsimhub/p0，自有 rustsimhub-build-p0 / rustsimhub-audio-build-p3。重建只在 exited 后保存证据，核对 label/image/mount，仅 rm 自有 container、不加 -v。source archive 打包允许列表精确 Git blob，build-input dirty 即拒绝。
- tools/verify-build-source.py 核对归属。生产/开发产物分 target/production 与 target/development，production core fixtures=false；不在两个模式间覆盖 release 欺骗缓存。部署核对文件 hash 和每个 --build-info source hash。
- Git HTTP 有间歇断线。tools/publish-exact-git-objects.py 经官方 API 发布同一 Git blob/tree/commit，逐个 SHA 必须匹配，远端未并发变化且 force=false；不改历史，不上传 private。
- 当前棒子已验证程序在 /opt/rustsimhub/dev-wifi-p4、dev-rtc-p3、dev-rtc-core-p3、dev-app-update-p6；独立锁定 Core/Edge/Media 账户。无新生产自启服务，已完的临时服务/权限均清理。旧四服务最后检查 active、PCM closed；新的会话先核实实际状态。
- 历史所有阶段的完整边界、hash 与测试见各 VALIDATION 文档；源头许可见 NOTICE.md。完整第三方发布 notices、完整镜像、恢复入口仍未完成。
