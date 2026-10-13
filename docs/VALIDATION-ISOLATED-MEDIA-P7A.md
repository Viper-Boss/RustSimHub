# 专用账户电话音频隔离验收

2026-10-14，P7 增加 1 点，整体约 54%。沿用已验收的 core de9e173 / native 93a4005 产物。

创建锁定 rustsimhub-media 账户 UID 984，没有永久 audio/sudo/netdev 附加组。核心 UID 986、网页 UID 985、IPC 组 987 为此前已创建账户。未改变旧用户。
临时 polkit 规则仅给 Core ModemManager Voice；对三个项目账户拒绝其他 MM/NM 控制动作。真实 pkcheck：Core Voice 通过，Edge/Media Voice、Core Messaging/Device.Control/network-control/WWAN 控制全部拒绝。

核心单元 PrivateDevices=yes、能力集合空；媒体单元能力集合空，只在该单元附加 audio+IPC 组，DevicePolicy=closed，有限 control/voice/PCM 白名单。身份/业务数据库只由 Core 访问；Root 操作端的 owner 私钥/草稿/lease 在自己的 0700 目录；media ticket/config 在 Media 自己的 0700 目录。核心媒体套接字父目录为 Core 所有、组不允许写，原生程序核验实际 Core UID 和 SO_PEERCRED。

单次 10010 调用：Core 986 创建/启动/挂断，Media 984 实际打开 PCM。三秒捕获/发送 150 帧、播放 149 帧；NAS 收并解码 149 帧，峰值 16828；播放静音峰值 1，transport_failed=false，关闭通过，未录音。18 秒独立挂断保护与 once 标记保持。
此前一次隔离准备因 Root 草稿父目录权限不符合私有文件检查，在拨号前停止，没有消耗拨号；修正为 Root 独立操作目录后成功。当前总计 15 次 10010 拨号，未新增短信发送。

验收后按 Root UID/mode/nlink 与确切文件 hash 移除仅自己新增的临时规则；Core Voice 恢复拒绝。全部自己服务停止，旧四服务 active、PCM 全 closed。
私有证据 .private/device-media-user.log、device-voice-policy-trial.log、rtc-carrier-isolated-second-prep.log、device-isolated-policy-cleanup.log。

参考规则 firmware/development/voice.rules 仅为开发权限样板，不自动安装。tools/test-rtc-carrier.py --isolated-users 先核验 Core Voice 权限；需要受控安装临时规则，完成测试后移除，不能为了测试授权所有 MM/NM 动作。
尚未配置生产自启，core 是显式 dev-fixtures 产物；这不等于完整非 Root 固件交付。正式 messaging/network 权限及生产组仍待逐项验收。普通蜂窝数据继续最后测。
