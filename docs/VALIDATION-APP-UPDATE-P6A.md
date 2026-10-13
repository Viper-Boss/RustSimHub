# 签名应用槽位首次实机验证

2026-10-14，P6 增加 2 点，总体约 53%。这里只验收应用更新基础，没有完整刷机镜像。

全部应用来自 de9e173 的精确源包 a78207e3f5eacc618a2e09ac1bc10cc7d70d9b752773a5bd5e44198203cbd56b。生产 core 自报 development_fixtures=false，生产 ctl/edge/updater 与原生媒体自报相同源包。发布端核验每个产物后，以独立的临时开发发布钥匙生成 epoch 1/2/3 包；三个包使用相同已验证二进制，用版本号区分选择器状态，并非三个不同功能版本。

发布私钥只留 NAS 自有 Root 私有目录，设备只收公钥。断网发布容器仅挂载自有目录；NAS ACL 要求 DAC_OVERRIDE 才能执行该 Root-owned 目录产物，此能力只赋给临时签名容器，不赋给业务服务。

- 棒子 stage factory，再 choose 启动 production core，通过实际 owner 签名登录、health 与 inbox 查询后 confirm。
- payload 改字节、64-byte 签名翻转均拒绝，选择器 state 与 factory 确认状态相同。
- epoch 2 first choose 已持久化 attempted。实际启动候选 core，再 SIGKILL；重新 choose 返回已确认 factory，实际签名登录和 inbox 查询正常。
- epoch 1 回放与替换 release 公钥均拒绝，floor 保持 2。
- epoch 3 通过实际健康查询后确认，下一次 choose 保持该版本，floor=3。
- 独立持久目录不在应用槽位内。identity.seed 哈希前后相同，三个启动版本的完整 inbox JSON 逐字节相同。使用自己的已停止开发库副本，未改写旧应用数据。
- 自有四个临时应用单元已停止，旧四服务 active，PCM 全 closed，无新电话/短信/数据激活，无固件分区写入。

tools/device-app-update-trial.sh 是匹配本产物的隔离验收脚本。包上传前已核对确切允许文件、普通文件类型、长度及哈希；不要直接解压来源不明的包或将临时开发签名钥匙用于发布。
私有证据 .private/app-publisher-third.log、device-app-trial-prepare.log、device-app-update-trial.log；目标私有试验目录中的失败日志与 selector state 保留。

尚缺：统一生产启动器/非 Root 业务组、完整 third-party license 交付、实际新功能版本兼容、远程授权更新、掉电硬件/安装恢复、系统镜像及独立断电启动。SIGKILL 和重新执行 choose 不能代替真实断电验收。system_ota_supported=false。
