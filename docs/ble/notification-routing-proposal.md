# GATT 通知：按 peer 的 FD 路由与提交语义提案

2026-10-30。**本批仅接口/失败样例，没有生产 AcquireNotify、通知实现或启动接线。**
当前固定特征/UUID/flags、应用分片格式和 GattBackendV1 不变；A 密码协议不在本批。
后续独立feature候选见[通知FD](notification-fd.md)；不改变本提案PR只有测试/文档的范围。

## 需要解决的问题

[冻结T5接口](../team/ADDITIONAL-OFFLINE-PACKAGES-2026-10-27.md#ii-gatt传输与bluez生命周期--gattbackendv1--secureportv1)
要求 notify 绑定 `epoch,current_peer,fixed_characteristic,frame_bytes`，第二 peer
不得继承结果。一个 Device1 租约只证明选中的连接，不证明整个适配器没有其他订阅者。
把 Value 放进普通 PropertiesChanged 不能表达目标物理 peer；即使 D-Bus destination
指定 BlueZ unique name，仍只定位到同一个守护进程。

实读并固定上游 BlueZ commit `14bd6ab65f4a647302edbd6df27f6faa91bee8a8`：

- [API文档](https://github.com/bluez/bluez/blob/14bd6ab65f4a647302edbd6df27f6faa91bee8a8/doc/org.bluez.GattCharacteristic.rst)：StartNotify 无 device 参数，通知由订阅会话共享；AcquireNotify 可带 device/mtu，Value 可选。
- [property_changed_cb](https://github.com/bluez/bluez/blob/14bd6ab65f4a647302edbd6df27f6faa91bee8a8/src/gatt-database.c#L3052)：Value 更新调用 send_notification_to_devices。
- [AcquireNotify及回退](https://github.com/bluez/bluez/blob/14bd6ab65f4a647302edbd6df27f6faa91bee8a8/src/gatt-database.c#L2894)：成功FD与 op->att 绑定；某些失败回退 StartNotify，后者返回值被忽略。
- [sock_io_read](https://github.com/bluez/bluez/blob/14bd6ab65f4a647302edbd6df27f6faa91bee8a8/src/gatt-database.c#L2634)：按 client->att 找设备，调用单设备发送。
- [append_options](https://github.com/bluez/bluez/blob/14bd6ab65f4a647302edbd6df27f6faa91bee8a8/src/gatt-database.c#L2361)：device/mtu/link 来源是实际 ATT 上下文。

这是所读上游版本的源码证据，不代表目标设备已安装此版本或已实机验证。
特别地，StartNotify 返回错误不能证明对方 CCC 被拒绝；因此候选实现必须**根本
不发送全局 Value 信号**，不能靠该错误阻止共享路由。

## 推荐的隔离候选接口

仅 AuthExchange/Result 增加标准可选 AcquireNotify 与只读 NotifyAcquired。
接口字段严格：`AcquireNotify({device:object_path, mtu:u16, link?:"LE"}) -> (unix_fd,u16)`。
实际 header sender/连续 BlueZ owner、私有 Device1 租约、当前 epoch/同一控制实例及
完全相同 device 均复核；MTU 必须来自该可信回调，至少23，帧预算仍封顶244。
未知字段/offset/prepare/错误类型、第二设备、重复获取、旧租约拒绝，不降级到共享通知。
StartNotify 保持不支持；即使上游忽略它的错误，也没有任何广播 Value 数据可发。

候选 Native 实现采用 AF_UNIX 包边界 socket pair（优先 SOCK_SEQPACKET），
NONBLOCK/CLOEXEC，一端给 BlueZ，另一端只由本 epoch/peer/特征所有。
每个特征最多一个 channel，共最多两个；发送共用一个在途槽，不能新建无限队列。
获取/转交FD中途取消须 fencing，不能把不明确的旧FD重贴到新peer。
实际 HUP/关闭、背压、部分/失败写、旧owner/peer、迟到均不广播、不自动重发。
已有 Outbox 4逻辑结果、每消息2048/256片/15秒、固定编号和MTU冻结预算保持。

`GattBackendV1.notify` 的 Ok 只表示：在同一5秒截止中通过本地门禁，完整一帧被
当前专属FD接受，返回前 owner/peer 仍有效。**不是 ATT/浏览器/应用收到证明**，
不能据此宣称 Core 事务保存成功或清除 Unknown。Core 的业务回执/持久查询继续独立。
超时/错误不自动重试；宿主保留原future和帧/许可直到实际I/O结束或有明确补偿，
不延长Core40秒/NM45秒，也不把通知发送结果代替安全会话验证。

## 实际最小样例

六项新增测试在真实私有 dbus-daemon 中运行：

1. PropertiesChanged 广播真实到达两个独立 D-Bus 订阅者（不是两个RF客户端实测）。
2. 私有总线完全没有其他连接时 emit_signal 仍成功，证明无接收确认。
3. 受现有生产 owner/Device1 租约门禁保护的**测试路径/接口**交付真实 Unix FD；
   内核 datagram 只到取得的端点，无关socket收不到。
4. 真实外来sender/第二device/重复获取拿不到该候选FD。
5. 原device路径断开重连，候选发送失败，旧FD没有数据。
6. 接收端关闭返回失败，没有 PropertiesChanged 回退。

测试候选只在 cfg(test)，路径 `/org/rustsimhub/test_notify`，接口
`org.rustsimhub.TestNotificationRoute.AcquireForPeer`，**不是实际 AcquireNotify**。
用 std UnixDatagram 验证FD传递/包隔离，不证明生产SOCK_SEQPACKET的HUP/预算/背压，
不证明 BlueZ 原生 ATT 路由、MTU互操作、目标版本、真实RF或业务授权。
当前生产路径仍没有通知实现、没有运行时测试开关/成功替身。

## 后续实现和主线验收

可在独立显式feature继续实现隔离FD候选；不改冻结应用wire或A密码格式，不接默认
服务/Root安装路径。若需要跨成员接口/预算变化，先单独提出具体提案。
主线仍须审核目标BlueZ版本/UnixFD协商/实际ATT路由、ARM及资源预算、D-Bus策略，
再与安全会话/原Transport/Core失效围栏集成验收。缺A端口仍不得启动完整服务。
不选择任何实际UID/GID、安装/重启、Power/Pair/Trusted/删除设备或修改系统总线。

上游C文件GPL-2.0-or-later，仅阅读并链接，未复制/编译/搬入其实现。
新增Rust测试AGPL-3.0-only，无第三方依赖/锁文件变化，没有密码、生产数据或设备访问。
