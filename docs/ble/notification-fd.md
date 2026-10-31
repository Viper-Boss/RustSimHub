# 显式 feature 下的按 peer 通知 FD 候选

2026-10-30。Linux `bluez-notify-fd` 依赖 `bluez-gatt`；默认关闭。
实现[路由提案](notification-routing-proposal.md)的隔离候选，不启动服务、不连接总线，
尚未组成完整 GattBackend/daemon 或接入 A 安全会话/Core 业务。

## 接口与路由

只有固定 AuthExchange/Result 特征增加标准 AcquireNotify、NotifyAcquired/Notifying。
DeviceInfo/Command 保持原接口；UUID/Flags/路径/应用分片格式不变，无 Value 属性缓存。
StartNotify/StopNotify 继续不支持，**任何错误都没有全局 Value 通知回退**。
即使某版 BlueZ 忽略 StartNotify 错误，也没有广播数据通道可向第二 peer 发帧。

AcquireNotify 严格接受 device:object-path、mtu:u16>=23、可选link=LE；
完整签名匹配，其他字段/类型拒绝。真实 header sender 和 device 先匹配已绑定
私有租约，再占特征槽、复核 owner/Device1；外来sender/第二设备不能毒化有效槽。
创建匿名 AF_UNIX SOCK_SEQPACKET pair，NONBLOCK/CLOEXEC，两端配置固定4096字节
缓冲请求（内核可按规则取整/加倍，不冒充总RAM实测）。写端由本epoch/peer/特征持有，
读端通过真实 D-Bus UnixFD 交 BlueZ，返回MTU=min(可信回调MTU,247)。
每特征一次获取，共两个channel；正在创建/已经获取/退休均不能隐式重取或重贴到新peer。
创建/转交中途失败或取消使槽退休，尚未绑定的元数据树不能获取FD。

候选使用标准BlueZ per-ATT FD路径；所读上游函数及版本见提案。目标设备的版本、
UnixFD协商、ATT路由、权限与浏览器互操作仍须主线实测，不能由私有总线证明。
宿主须先处理实际 Connected 事件并 bind_peer；若回调早于绑定，会失败，不能把
BlueZ的CCC确认或GATT订阅状态当成已授权/可用会话。

## 发送、期限与未知结果

`GattExport::notify(epoch, peer, fixed_characteristic, OpaqueBytes) -> Pending<()>`
消费一帧，由可信宿主/原Transport调用；不接受任意方法/路径，不验证或替代A密码。
检查私有租约、epoch/peer、RB/v1/对应kind、长度13～协商预算（最多244）。
全部特征共用一个在途发送许可，future拥有帧/channel/许可到实际结束。
未poll无副作用；已开始future取消会退休FD，不能换帧假装旧工作未提交。

完整5秒逻辑截止覆盖前置核验、等待可写、单包提交和后置核验，不延长Core/NM预算。
仅EAGAIN表示尚未提交，可在同一future等待可写；完整/部分写后都不重发该包。
写入用MSG_NOSIGNAL，避免对端关闭导致SIGPIPE；部分/失败写、HUP、反向非预期字节、
旧owner/peer、迟到或期限均失败并fence，反向FD数据不进入Command/安全后端。
冻结flags只有notify，没有把indicate确认或任意字节当成授权/业务回执。

Ok只表示当前专属FD完整接受本帧，返回前本地门禁有效；**不是ATT/浏览器/业务收到证明**。
实际测试已观察到：对端FD先收到包，后置Device1查询卡住，发送future仍返回Deadline。
所以错误/超时也不是未提交证明，不能自动重发或清除Core Unknown。
Outbox原4逻辑结果、2048/256片/15秒与编号规则不变；可信宿主必须保留原future并
驱动原Transport/Lifecycle/SecurePort/Core失效清理，不把本地FD关闭当radio quiescence。

retire/Drop会fence并shutdown本实例channel；被future持有的Arc不能保持写权限。
属性查询、发送前后与`check_peer_current()`会检测FD健康；没有新后台任务，
空闲期间仍须宿主poll并执行清理。停用/断连后需原epoch排空、远程unregister和
关闭专用连接，新epoch/新连接重新开始，不删除任意BlueZ设备或其他应用资源。

## 实测及依赖边界

18项新增实际生产Rust路径测试：真实私有dbus-daemon/SCM_RIGHTS/SEQPACKET/FD标志、
两包边界/双特征隔离、MTU、实际外来sender/第二device/重复/错误输入、缺FD/错epoch/
peer/kind/超预算、HUP/反向数据、元数据阶段、未poll/取消/单在途槽、retire/Drop、
device/owner gap、获取中退休、小内核缓冲背压截止、已提交后校验超时。
只有外部BlueZ/Device1及可信宿主回复为合成服务；A密码/真实ATT/RF/角色UID/Root/
目标版本/ARM/native设备/资源预算/Core-Wi-Fi端到端未运行。

重用锁定socket2 0.6.5（Rust1.70）、libc 0.2.190（Rust1.65），许可MIT/Apache-2.0，
只给本地ble加两条可选Linux依赖边及已有tokio/net feature；无第三方版本/来源变化。
新源码AGPL-3.0-only，自行实现并引用标准API，上游C代码仅阅读链接，未复制/编译。
默认正常依赖树仍仅ble自身，无Root材料、生产启动、wire/schema或密码格式变化。
