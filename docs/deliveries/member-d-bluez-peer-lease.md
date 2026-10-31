# D22/24 局部交付：Device1 租约与 GATT 一次绑定

2026-10-30。Viper-Boss/RustSimHub，分支 `codex/gtw-bluez-peer-lease-20261030`，
到 main 的依赖 #113 草稿。起始完整 HEAD
`32a2c3dace7e0bd25a117670adba22c8a979b1e2`，已含 main
`80db24e9b101dd3fb4b8c951aa7f1e0e1858e65d`。
最终执行代码/整仓受测 HEAD：**`d8135f4b366c5edbd260d3b4809b06a7d20af468`**。
最终完整 HEAD 见 PR 正文，之后仅本人文档变化。

## 结果与范围

固定 GATT 元数据先导出/注册，真实 Device1 连接出现后一次性消费租约。
绑定前所有方法回调失败；完全相同控制实例/epoch 的有效租约才可绑定，
并发仅一方成功，取消不暴露半绑定，退休/成功后不能重绑新 peer。
实际 owner、Connected/Adapter 快照、订阅在先的属性变化/对象移除历史共同检查，
同设备路径断开再连、移除重建不复活旧请求。请求/后置检查/空闲入口复核租约，
每次检查与完整 GATT 回调5秒逻辑截止，迟到快照/宿主回复不能返成功。
Weak scope 避免 object-server/连接环；没有默认活租约、后台任务或自动重连。

只改 ble feature/控制元数据getter/Device1与GATT边界/私有总线测试和本人文档。
无 wire/schema/新Auth或密码格式、Core默认调用、服务/Root材料、第三方包/版本/
Cargo.lock 变化；仅已有 tokio/time feature 边。默认库无运行依赖不变。
[租约API/阶段/观察证据](../ble/bluez-peer-lease.md) ·
[当前GATT输入/宿主责任](../ble/bluez-gatt-callbacks.md)。

## 实测

新增 **22项**（12租约、4租约接线、5启动/绑定竞争、1迟到后置检查），
BLE **117通过**；整仓 **921通过、0失败、6项继承忽略**，等于#113的899+22，
不含未合并#101。整仓/仅peer/仅advertisement/仅control严格Clippy、默认库编译、
fmt/diff通过，无新增ignore，保留既有proc-macro-error2 future-incompat提示。
Linux x86_64、UID1000、Rust1.98、锁定离线缓存。

真实私有dbus-daemon/zbus：Device1 GetAll/属性信号/对象移除重建、实际header
sender、broker owner连续性、固定对象注册及宿主回调运行；只有外部BlueZ/Device1
与可信宿主回复合成。设备路径是假数据，不冒充RF连接或密码/业务正例。
开发中受控删除drain而仅查Connected快照的临时变体，原断开重连回归实际退出101
（Ok != StaleContext）；恢复历史检查同断言通过。该变体未进入任何生产提交，
不是宣称main曾含此缺陷。测试同步/未启动对象服务器的检查改为实际状态断言，
不靠增加期限或假造上下文通过。

```sh
source /workspace/toolchains/activate-rustsimhub.sh
cargo test -p rustsimhub-ble --all-features --locked --offline
cargo test --workspace --all-features --locked --offline
cargo clippy --workspace --all-targets --all-features --locked --offline -- -D warnings
cargo clippy -p rustsimhub-ble --all-targets --features bluez-peer --locked --offline -- -D warnings
cargo clippy -p rustsimhub-ble --all-targets --features bluez-advertisement --locked --offline -- -D warnings
cargo clippy -p rustsimhub-ble --all-targets --features bluez-control --locked --offline -- -D warnings
cargo check -p rustsimhub-ble --no-default-features --locked --offline
cargo fmt --all -- --check
git diff --check
```

日志仅本地 work/bluez-peer：

```text
workspace-delivery.log b78d7a7b02abb234b7dcb1568550fdb9eb720a8b1e63a125c360c5275688a42f
workspace-delivery-clippy.log a35c20f83f0a70378eea6ff31053288cfc4d5c49c2a13aec0659927215797fe9
deadline.log c28b8e293685ef1768621b2b66efdf991d902fbb58ee380045c4786f034cc359
peer-only-delivery.log 59639588a6f1826f129cbcd121edc07279dab94786f5fa5bde6b3439b2e845cc
ad-only-delivery.log 59639588a6f1826f129cbcd121edc07279dab94786f5fa5bde6b3439b2e845cc
control-only-delivery.log 59639588a6f1826f129cbcd121edc07279dab94786f5fa5bde6b3439b2e845cc
default-delivery.log a5f4c585ee974ca44916ac30a98bbc189e067a7e0a6bc6d2e8d6bc525be724af
snapshot-only-negative.log 5f1aef4218058bdf1837ed453f9a30d126cb1e0bd959d0b8eb2f91549a78f8f3
snapshot-positive.log eaa124d20142e47a3f992be4e36fcda70c73659f330c94dd265fa5405b78860c
```

## 验收边界与继续点

租约只表示观察到的传输新鲜性，不是配对/Trusted/用户批准/密码或Core能力。
宿主尚须驱动空闲检查及Lifecycle::Connection/Transport/SecurePort/Core channel
清理，没有完整daemon或默认服务启用。通知仍明确NotSupported，后续须保证按
实际peer路由，不能把BlueZ共享订阅或D-Bus发送成功当成特定对端收到。
本批不替代A，缺安全端口仍不得启动完整恢复服务。
没有主机系统总线/BlueZ/HCI/RF/真实Device1/角色UID/Root/NSS/polkit/cgroup、
ARM/native/A互操作/Core-Wi-Fi端到端；测试daemon仅自己创建并kill/wait。
无生产设备/数据/NAS、短信/拨号/流量、签名/部署，不改其他成员/全局台账。

完整步骤新增0；仍 **21/26离线包已提交、剩5步，固定P0–P7约80%**。
新增源码AGPL-3.0-only/Codex辅助，保留作者/许可，不虚报工时。
前置未合并前草稿，不自动合并，远程CI按最终HEAD回读。
