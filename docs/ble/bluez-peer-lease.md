# 观察到的 Device1 连接租约

2026-10-30，Linux 显式 `bluez-peer` feature；`bluez-gatt` 依赖此门禁。
本租约只证明观察到的传输上下文新鲜，不证明配对、Trusted、用户批准、密码或
Core 业务权限；不调用 Pair/Connect/Disconnect/RemoveDevice/Trusted/Power。

## 快照和历史共同检查

`PeerLease::bind(&BluezControl, Epoch, device_path)` 捕获控制端口实际 unique owner，
只允许 Hci0 六字节设备路径。在快照之前订阅两个实际 sender 限定的流：
设备路径的 PropertiesChanged/arg0=Device1，以及 BlueZ 根 `/` 的 InterfacesRemoved。
各容量32。GetAll 只接受 Connected 的 bool=true 及 Adapter 的 object-path=Hci0；
缺失、类型不符、断开、对象消失均失败。其余真实 Device1 属性不进入应用协议。

`check_current()` 在读快照前后排空事件、复核原连续 owner 和局部 live fence。
Connected/Adapter 任何变化或 invalidation（即使最终又 true/原值）都单调失效；
同路径 Device1 被移除后重建也不能复活。其他设备/接口移除、RSSI 等无关属性不
替代连接证据。最多排空64个事件，耗尽预算失败，不无限处理洪水。
只查 Connected 快照会漏掉 false→true，受控删除 drain 的真实私有总线回归已
实际失败（Ok != StaleContext），恢复历史检查后同断言通过。

已开始检查被取消也通过 guard 退休；未 poll 的检查无副作用。失败/retire/Drop
均不能回到有效状态，不自动跟随 owner、复用租约或重连。实现没有后台监听任务，
宿主须在空闲期间及工作前后 poll 检查；API 不自动撤销 Core channel 或停止 RF。
bind/整次 check 有5秒截止，慢 GetAll 使租约失效，迟到快照不能复活。
GetAll 是只读 RPC，宿主仍须驱动原生命周期处理清理/故障，不能延长业务预算。

## 与 GATT 的启动阶段

为了避免“必须先连接才能导出服务”的启动循环，`GattExport::publish(control, epoch)`
只导出固定元数据树；没有 peer 绑定时所有方法回调失败，宿主工作队列保持空。
待真实 Device1 连接出现，宿主创建租约，调用 `bind_peer(lease)` 一次性消费它。
绑定验证完全相同的控制实例与 epoch，先检查租约再发布私有 Weak scope；
GATT handle 持有唯一租约，object-server 仅持 Weak，不形成连接引用环。
方便方法 `publish_bound(control, lease)` 可用于已经连接的隔离场景，仍先核验再导出。

一次成功绑定后不能换 peer/epoch，即使旧租约已退休也返回 Busy。元数据导出失败/
Drop/retire 保留固定路径退休对象，须远程清理、排空工作、关闭专用连接后用新
epoch/连接重新开始。没有默认活租约、从载荷分配连接证明或运行时 mock 开关。

每个回调检查租约与 owner，options.device 必须恰等于绑定设备；请求 check_current
与完成值返回也复核租约、固定5秒期限。本次把完成后的 peer 检查纳入同一截止，
避免宿主准时回复而后置检查拖延时返回迟到成功。
`GattExport::check_peer_current()` 提供空闲/pre/post 工作入口，错误时局部退休；
宿主仍须用原 Lifecycle::Connection/Transport、安全会话和 Core 私有能力驱动清理。
通知仍明确 NotSupported，没有物理 peer 驱动/完整 daemon 或业务接线。

## 证据边界

真实私有 dbus-daemon、实际 Device1 属性 RPC/信号、对象移除/重导出、header
sender、连续 owner 以及 GATT 回调路径运行。外部 BlueZ/Device1 和可信宿主回复
均为合成服务；设备路径是固定假数据，不能冒充实际 BLE、角色UID或密码正例。
没有主机系统总线/BlueZ/HCI/RF、ARM/native、Root/NSS/polkit/cgroup、A互操作、
Core-Wi-Fi端到端、签名或部署。默认库无运行依赖，无新第三方包/版本/锁文件变更。
