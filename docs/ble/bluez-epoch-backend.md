# 单 epoch 的真实 BlueZ GattBackendV1 适配

2026-10-31（Asia/Shanghai）。显式 `bluez-backend` feature 把现有 BluezControl、固定广播导出、GattExport、PeerLease 和 Native FD 适配成真正的 `GattBackendV1`，可交给已有 Lifecycle/Transport。默认 feature 空、没有隐式连接总线/启动守护进程/安全端口或生产安装。

## 主机接点

`BluezBackend::new(control)` 消费已绑定当前 unique owner 的控制端，返回 backend 和必须保留的 BluezHost。调用者必须提供**专用连接**，每个实例只运行一个 epoch；固定对象路径关闭前保留墓碑，停止后本 backend 不接受新 epoch。再次运行需要新的专用连接及新的受信生命周期域，不复用旧对象/nonce/连接。

- backend 实现四个固定注册/撤销方法和按 peer 的 FD notify。注册先发布本地固定对象，再调用捕获 owner 的固定 manager RPC；没有任意路径或命令入口。
- host `take_owner_loss()` 只领取一次，须持续poll；owner变化后交给原Lifecycle.owner_lost/停止逻辑，不跟随替代owner。
- `take_callbacks(epoch)`、`take_release(epoch)` 各领取一次，保留receiver并处理关闭。旧epoch不能领取当前队列，回调容量/队列及deadline沿用原组件。
- `bind_peer(epoch,device)` 内部完成真实Device1/适配器/owner/连接历史证明，再一次绑定导出对象；返回的Peer是运输新鲜度，不是Core能力。主机只用该结果通知Transport.connected，并持续 `check_peer`。
- host必须核对每个CallbackRequest的当前性、连接及Transport状态，再交给真实分帧/安全/Core链；检查前后跨await仍不可省。类型允许注入的安全端口没有默认/Mock生产实现。

既有 `Lifecycle::new(..., None, ...)` 实际测试仍在任何导出/manager RPC前拒绝。完整服务仍缺A会话和Core业务接线，本适配不提供伪造Principal或“认证成功”标记。

## 期限、取消和清理

manager副作用槽一次只允许一个拥有型future；重叠尝试返回Busy，不能越过迟到注册去清理。原Lifecycle的5秒截止只把状态置Unknown，继续持有/轮询真实future，直到真实注册完成后补偿；没有spawn、隐藏重试或抛弃迟到副作用。主机必须同时按暴露deadline和后端wakeup轮询。

停止广播先禁止新回调/通知及迟到host ack，但仍保留合法Release的确认接点，直到对应撤销完成。应用撤销完成后关闭专用连接。未完成future被放弃、失败的注册/撤销、漏掉广播清理、无法证实的资源均保留Unknown；应保留清理责任，不能改标为quiescent。未poll的注册future没有副作用，也不消耗epoch。

Drop backend或host只立即使本地I/O失效，**不异步卸载、不宣称远端广播或业务回滚成功**。`BluezHost::close_connection()` 是紧急本地围栏/关闭，不把之前失败的Lifecycle改成成功。正常停止仍由原Lifecycle依次撤销广播、关闭安全peer、撤销应用；owner替换时不向新owner撤销旧上下文。

产品已有 release panic=abort，RAII只证明普通取消/Drop边界，不是线程panic或进程崩溃隔离。

## 实际测试

17个新增真实私有D-Bus组件测试：完整Lifecycle四RPC/安全peer关闭及连接关闭；无安全端口零RPC；once-only回调/Release及旧epoch；单epoch墓碑；错epoch清理不破坏当前peer；并发Busy/真实迟到完成；取消后Unknown；失败注册和清理；真实owner丢失wait；host Drop；漏广播撤销；未poll零副作用；真实Release回调；5秒截止继续持有迟到注册；owner释放后同unique重获不清理替代上下文；注册中host Drop；停止后实际在途WriteValue不能被旧host ack转成功。

另将实际backend放入#130的跨UID标准GATT试验：固定四manager RPC、原policy、标准a{sv}→hq、Auth/Result分别实际FD收包、写回、畸形/MTU22/重复/foreign拒绝、断连Stale/EOF和清理。新显式选项：

```sh
python3 -B firmware/system/ble/run_policy_kernel_uids.py --rust-gatt --rust-backend
```

没有放宽候选policy。Backend模式ObjectManager严格核对5个GATT对象加同一固定根下的1个LEAdvertisement1对象(Type/唯一ServiceUUID)，原直接GATT模式仍严格5个对象；不是笼统放宽对象清单。实际外部BlueZ识别该组合及真实RF仍需验收。

**实际整仓967通过、0失败、6项继承忽略**；整仓严格all-targets/all-features Clippy、fmt/diff、默认BLE无feature编译通过。新backend模式手动1编排试验/3角色各1通过，原直接GATT模式也3角色通过，旧头策略9场景和材料16项重跑通过；手动不计入967，普通suite未运行Docker/切UID。精确受测HEAD与日志见[交付报告](../deliveries/member-d-bluez-epoch-backend.md)。

## 边界和下一步

外部BlueZ和测试安全/host端点仍合成；测试没有真实密码成功、浏览器/Core-Wi-Fi/运营商、实际BlueZ/HCI/RF/生产NSS/ARM/实机预算或Root安装。容器有限资源只约束本试验，不代替服务48M预算验收。没有新增依赖/升级Cargo.lock、改wire/schema/签名/生产默认值、设备或其他成员分工。

完整步骤新增0，仍21/26离线包已提交、剩D22–26五步，固定P0–P7约80%。本包补掉真实BackendV1接线；A标准安全会话、同一Wi-Fi actor/迁移审批、生产Root角色/完整host及实机、TURN依赖准入和实际适配仍保留。接下来核对固定TURN候选API与资源/鉴权边界，为D25准入提供可审证据，不先把未批依赖接进产品。
