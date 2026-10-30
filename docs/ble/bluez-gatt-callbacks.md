# 固定 GATT 树与不透明回调边界

2026-10-30，Linux 显式 `bluez-gatt` feature；复用 control/advertisement。
这是本地对象和回调部分，尚不是可启动的恢复服务或完整 GattBackendV1。
没有系统总线连接、密码实现、Core 业务授权、自动注册或通知成功替身。

## 固定对象和输入

`GattExport::publish(&BluezControl, Epoch)` 在同一注入连接导出：

- ROOT 上的标准 ObjectManager；
- SERVICE_PATH 上 GattService1：UUID、Primary=true、Includes=[]；
- 四个 manifest 特征上的 GattCharacteristic1：UUID、Service、Flags。

路径、UUID 和 Flags 均来自既有 manifest，不能从载荷扩展；属性只读。
没有通过 Properties.Get/GetAll 暴露的 Value/结果密文缓存。
单独 GATT 的 GetManagedObjects 实测恰有五个子对象；同连接广播对象存在时为
六个固定子对象，额外的接口是既有 LEAdvertisement1，不是动态 GATT 服务。

回调核对真实 header sender、捕获的 BlueZ unique owner 与连续 owner 历史。
ReadValue/WriteValue 签名完整匹配，额外 body 字段失败。
options 中 device 必须是实际类型 object-path，形状限于 Hci0 六字节设备路径；
这只验证来自可信 BlueZ 的字段形状，不能证明设备已连接、配对或获批。
offset 必须为 u16；mtu 若存在必须为 u16 且至少23，上限由既有 Budget 封顶244；
link 若存在仅 LE。未知选项/类型不符拒绝，不忽略。
Write 只允许 AuthExchange/Command、type=request、offset=0、prepare-authorize=false，
属性帧必须大于12字节且不超过当前 budget（无 mtu 时20）。
帧仍完全不透明，宿主必须交原 Assembler/Transport 检查 magic、顺序、分片、重放等。
Read DeviceInfo 仅返回固定公开 JSON，允许公开长读 offset，不进入宿主队列；
Read Result 必须 offset=0，向宿主索取当前结果，不提供任何默认值。

## 单槽所有权与期限

宿主接收容量1的 `mpsc::Receiver<CallbackRequest>`。
每个请求持有实际 owner/device、epoch、特征、操作/预算及不可复制的 semaphore permit。
全部特征共用一个槽，其他请求立即失败，不堆积等待许可的宿主队列。
`finish(self, Result<CallbackReply, Error>)` 消费请求，permit 随完成值转交给回调，
直到该值被消费/丢弃才释放。丢弃请求/接收方退出/错误回复类别均失败。
Accepted 仅可确认 Write，Value 仅可返回 Result 且长度受当前属性预算约束。
这里 Accepted 只表示**可信注入宿主**接收的这一帧；它不是业务成功或安全认证。

每个请求有固定 Tokio Instant 5秒截止，不延长、不重试。
`check_current().await` 在检查 owner 前后均检查截止与私有请求 live fence。
期限/回调任务退出使该检查失败；超时不丢弃宿主实际在途工作，也不释放其 permit。
迟到 finish 可以排空资源，但不能返回迟到成功或通知。
真实 D-Bus 调用方丢弃等待并不发送方法取消：服务器可能继续等到5秒，
宿主仍须使用 Core 私有 capability/独立会话截止，不能把方法取消当成事务撤销。
本队列限制的是本组件交给宿主的工作，不声称限制 zbus/broker 所有原始消息缓冲。

## 宿主尚须完成的门禁

请求里的 peer/epoch 是传输元数据，**没有每次 BLE 连接的授权证明**。
启用前须实现真实 Device1 连接/断开跟踪、原 Lifecycle::Connection 代次和
Transport/SecurePortV1/Core 私有 capability 接线；不能收到队列消息后再将旧请求
绑定到刚重连的同一设备。当前导出必须限制在单个物理 peer 生命周期，任何断开/
连接状态不明先 retire_local，排空旧工作，远程清理并关闭专用连接后再用新连接/
新 epoch 导出；不能复用本对象承接重连。当前尚无生产宿主执行这些动作。
缺 A 安全端口时不得通过本低层导出 API 启动完整恢复服务。

StartNotify/StopNotify 在核 sender 后明确 NotSupported，Flags 保持冻结接口，
但未声称通知实现；没有 Value 信号发射、AcquireWrite/AcquireNotify 或缓存旁路。
Drop/retire 保留本连接固定路径的退休对象，重复导出 Busy；部分发布失败同样
退休已建立的回调对象。所有本 epoch 对象与宿主工作结束、远程 unregister 后才
能 close_connection。局部退休/连接关闭均不证明无线清理或 quiescent。

## 实测边界

19 项新增真实私有 dbus-daemon/zbus 测试，只有外部 BlueZ manager 和可信宿主
回复为合成组件；不将 Accepted 测试回复冒充密码或 Core 业务成功。
覆盖对象树、公开长读、合法写/结果读取、实际外来 sender、选项/模式/MTU/载荷、
未知/不支持操作、丢宿主、单槽、真实5秒超时与迟到工作、取消调用、owner gap、
退休/Drop、回复类别/长度、同连接固定广播与注册/清理调用。
初版真实测试发现 UUID 属性大小写和多参数 signature 判定问题，保留原断言修正。
没有主机系统总线/BlueZ/HCI/RF/实际 Device1 连接事件、角色 UID/Root/NSS、
ARM/native、A 密码互操作、Core-Wi-Fi 端到端或部署验收。
仅新增本地 feature 与 tokio/time feature 边，无新第三方包/版本/锁文件变化。
