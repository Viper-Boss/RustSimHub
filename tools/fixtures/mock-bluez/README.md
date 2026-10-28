# 有界 GATT 外部端口夹具

本目录是入口索引，不安装 fake bluetoothd，不连接系统总线/适配器。实际可执行夹具在 `crates/ble/src/lifecycle/tests.rs` 与 `transport/tests.rs` 的 cfg(test) 目标，运行的是生产 Lifecycle/Transport，不是夹具重写的状态机。

- 固定合成 owner/peer、临时内存日志和手工可控 future，测试正常返回、失败、超时后迟到，以及顺序/调用次数。
- lifecycle夹具验证注册/广播/撤销/Release/关闭/代次；transport夹具验证只把完整消息交注入端口、结果种类、确认后推进、旧连接拒绝和清理。
- 安全端口回复是**测试合成 opaque 数据，不是密码实现或认证证据**；没有生产 Mock 实现、默认 fallback、配置开关或设备服务。

运行：`cargo test -p rustsimhub-ble --all-targets --locked --offline`。真实 D-Bus 选项类型解码、BlueZ owner/peer凭据、密码/权限、设备和ARM另行验收，见 `docs/ble/gatt-adapter-integration.md`。
