# 有时限的已保存 Wi-Fi 试连接

2026-10-14，开发实现尚待目标板验收，不增加进度。

显式 --serve-network-trials 启动模式才启用 WifiTrial；默认核心拒绝。NetworkManage scope 前后核验实际 Principal/generation。仅 UFI003 的 wlan0 与仍正常连接的 usb0；调用前全局 MM 通话必须全部结束。

通过 D-Bus 唯一 owner 固定 NM/MM 身份，核验设备类型与名称、已保存 Wi-Fi 类型、UUID/SSID 身份摘要。只读取 GetSettings，不调用 GetSecrets。单个请求只激活原有 Wi-Fi profile，绝不改无线开关、APN、WWAN、USB、默认全局网络或任意设置；蜂窝数据测试依用户要求最后进行。

执行前 fsync 私有防重意图，最多 256 个 journal 文件；不明结果拒绝重试，同一成功 key 返回原结果。先给唯一 Wi-Fi 设备建立 8 秒自动恢复 checkpoint，再一次 ActivateConnection，最后恢复并核验原 profile 与 USB 维护通道。失败/撤销也对自己的确切 checkpoint 做补偿；owner 变化不向新 daemon 发送旧 checkpoint。进程被终止时 NM 计时器负责恢复。不能使用空设备集合、空 checkpoint 删除、DestroyAll 或 DELETE_NEW_CONNECTIONS 影响其他配置。

边界：这里只试连接当前已保存的 Wi-Fi；不是新 SSID 配网、正式配置持久确认、热点或远程管理。真实自动超时、进程杀死、调用去重、只读拒绝和回退还须硬件验收。

依据 [NetworkManager 官方 D-Bus 文档](https://networkmanager.pages.freedesktop.org/NetworkManager/NetworkManager/gdbus-org.freedesktop.NetworkManager.html)：CheckpointCreate 可以限定设备，并由非零 timeout 触发自动回退。发布状态以 VALIDATION 文档和 PROGRESS.md 为准。
