# 固定广播对象与 Release 回调

2026-10-30，Linux 显式 `bluez-advertisement` feature，依赖 `bluez-control`。
本组件导出本地 `org.bluez.LEAdvertisement1`；调用者注入已捕获真实
BlueZ unique owner 的 `BluezControl` 和宿主分配的 `Epoch`。
不连接系统总线、不注册无线广播、不提供 Core/安全会话能力。

## 固定接口

`AdvertisementExport::publish(&control, epoch)` 返回非 Clone 的导出所有权
和一个容量恰为一次的 `oneshot::Receiver<ReleaseEvent>`。
对象仅位于 manifest 的 `ADVERTISEMENT_PATH`，只读属性恰为
`Type=peripheral`、`ServiceUUIDs=[SERVICE_UUID]`。
不提供名称、地址、厂商数据、功率或任意属性入口。

`Release()` 必须为空参数。实际 D-Bus header sender 必须等于捕获的
BlueZ unique owner；继续检查控制端口的 broker NameOwnerChanged 流和当前 owner。
伪造 sender、owner 更换、同一 unique connection 丢失后重新取名均失败。
成功事件只含固定 epoch 和已核验 owner；它不是恢复授权或 Core capability。
第一次成功投递后重复 Release 幂等确认，不再次投递；接收方已经退出时返回固定
失败并记住该失败，后续重复调用也不能返回成功。无等待中的无限消息队列。

## 连接与清理责任

同一连接的固定路径只允许一次导出。Drop/`retire_local()` 立即关闭待投递事件并
拒绝后续回调，但保留失效对象直到整个连接关闭。再次 publish 得到 Busy，不能把
旧路径上迟到的 Release 贴上新 epoch。发布中途取消/失败同样通过 guard 退休。
导出对象只持有控制绑定的 Weak 引用，不形成 object-server→control→connection
引用环；没有其他在途控制 future 持有绑定时，丢弃控制对象也会使回调失败。

宿主必须为每个 epoch 使用新建的专用 D-Bus Connection，并在蓝牙生命周期完成
远程 unregister 后关闭该连接。`close_connection()` 会关闭整个注入连接，
若同连接还有 GATT 对象，须等待它们全部完成清理。
本地退休、事件投递、连接关闭均不证明物理广播已停止；远程调用超时/错误保持
结果不明，由现有 Lifecycle 保留在途 future，不能自动重发或提前声称 quiescent。
回调核验和 publish 没有新增后台 watcher/计时器，宿主仍负责现有 5 秒预算和
idle owner-loss 驱动；尚未提供完整 GattBackendV1 或设备端 daemon。

## 实测边界

15 项新增测试使用本轮实际启动的私有 dbus-daemon 与 zbus。
真实发送者/名字变化/对象导出/属性/调用/错误/连接关闭均经 broker；
只有外部 BlueZ manager 为合成服务。测试包括固定只读属性、合法与外来 sender、
换 owner 和同 unique 重取名、重复/丢接收方 Release、额外参数、Drop、退休、
重复发布、未 poll 的发布、控制绑定退出、旧连接迟到与新 epoch 隔离，
以及同连接 register→Release→unregister→close 调用序列。
这不验证 BlueZ/HCI、RF、系统总线策略、真实 UID 角色、Root/NSS、A 密码互操作、
GATT 通知或 Core/Wi-Fi 端到端。未访问主机系统总线或设备。
默认库依赖不变；没有新增第三方包、版本、Cargo.lock 改动或部署。
