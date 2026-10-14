# RustSimHub 当前接手入口

2026-10-14。用户最新目标约 75%，取代旧目标约 60%；当前实证约 57%，按 PROGRESS-SCALE.md 固定分母估算。不要因保存小检查点停止。每次最终回复必须报告百分比。

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

- GitHub 私有 https://github.com/Viper-Boss/RustSimHub，main。最新离线 recovery 实现 de4ef7d 的 CI 通过；ARM64 构建正在用精确该提交源包，不得拿正在写入的 target/release 部分产物部署。先查自有构建容器实际状态。
- crates/recovery 独立 age 0.11.5 离线 X25519 身份加密/空目录恢复；没有把加密恢复依赖引入常驻 core。篡改/错钥匙/错 pin/覆盖/链接测试 CI 通过，目标板未验收。见 encrypted-identity-recovery.md。
- 生产 core 首次签名初始化、SIM ready/9 条短信、重新签名登录/旧 token 拒绝通过；发现重启后 9→18 候选消息，不能称重复已修复。证据 .private/firstboot-owner-production.json。
- 重启观察修复 8cd88ee/75d0c79/9f8f406：持久 backend 使用真实 broker UUID+MM owner+SIM/Modem 路径，runtime instance/连接计数/fence 保持独立；从同一 ready line SQLite 恢复实际 source/content_hash/编号。退休/未知重置不恢复旧 source，跨 backend/SIM 仍失效。无 schema 升级。第一次 CI 因 FakeBus 未实现 GetId 超时，9f8f406 已补真实模拟 broker UUID，最新 CI 待查。见 observer-resume.md。
- 新工具 tools/test-firstboot-owner.py / capture-full-device.py 已保存本地，检查 git status 后提交。不要修改旧工程或伪造测试通过。

## 下一步顺序

1. 核实 de4ef7d 自有 ARM64 构建完成并保存 source/binary hash；实际加密备份恢复及错误 pin/错 key/覆盖拒绝，并保留匹配身份授权的登录证明。
2. 等最新 source-resume CI，通过后打包精确 Git blob、构建/部署新 production core；新独立库重新初始化，实际 9 条短信重启前后应保持 9 条且 ID 不变，验证真实退休/失联/SIM/撤销边界。
3. 生产服务组/首次初始化、统一签名选择与健康确认、自启/重启持久化；备份/恢复门禁保持。继续 NAS 兼容、网页写/媒体、新配网与长期验收。蜂窝数据最后。

## 工具与资料位置

- 本地 C:/Users/wzs11/Documents/ChatGPT/大疆4G模块/RustSimHub，PowerShell；Python 中文文件操作用 -X utf8 或显式 encoding='utf-8'。Windows 缺 MSVC linker，Linux 检查在 NAS/CI。
- .private/nas.json 与 device.json；后者通过 NAS jump，固定 maintenance key 与 known_hosts，RejectPolicy，不关闭主机校验。.local/python-deps 是 Paramiko。
- tools/remote.py --config ... --script ... --output .private/... [NAS --sudo]；SFTP upload/download。密码只经 stdin。
- NAS /mnt/docker-data/rustsimhub/p0，自有 rustsimhub-build-p0 / rustsimhub-audio-build-p3。重建只在 exited 后保存证据，核对 label/image/mount，仅 rm 自有 container、不加 -v。source archive 打包允许列表精确 Git blob，build-input dirty 即拒绝。
- tools/verify-build-source.py 核对归属。生产/开发产物分 target/production 与 target/development，production core fixtures=false；不在两个模式间覆盖 release 欺骗缓存。部署核对文件 hash 和每个 --build-info source hash。
- Git HTTP 有间歇断线。tools/publish-exact-git-objects.py 经官方 API 发布同一 Git blob/tree/commit，逐个 SHA 必须匹配，远端未并发变化且 force=false；不改历史，不上传 private。
- 当前棒子已验证程序在 /opt/rustsimhub/dev-wifi-p4、dev-rtc-p3、dev-rtc-core-p3、dev-app-update-p6；独立锁定 Core/Edge/Media 账户。无新生产自启服务，已完的临时服务/权限均清理。旧四服务最后检查 active、PCM closed；新的会话先核实实际状态。
- 历史所有阶段的完整边界、hash 与测试见各 VALIDATION 文档；源头许可见 NOTICE.md。完整第三方发布 notices、完整镜像、恢复入口仍未完成。
