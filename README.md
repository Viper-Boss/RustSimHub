# RustSimHub

面向 MSM8916 / Snapdragon 410 棒子的独立通信固件：Linux + Rust 用户态，
设备自身管理电话、短信、授权和数据，支持 USB/NAS、Wi-Fi 与蜂窝网络。
手机客户端另行开发。

当前为开发初始阶段，不能作为可刷写发行版。已有系统继续承担实际通信。

- [详细开发计划](docs/development-plan.md)
- [进度](docs/PROGRESS.md)
- [接手说明](docs/HANDOFF.md)
- [首阶段接口边界](docs/architecture-foundation.md)
- [SQLite 持久化与恢复](docs/storage.md)
- [设备身份、配对和鉴权](docs/auth.md)
- [所有者恢复、客户端 CLI 与 core 生命周期](docs/recovery-clients.md)
- [SIM 观察与短信收件箱](docs/sim-inbox.md)
- [事件订阅、重连与短信对象核对](docs/observer-events.md)

```sh
cargo test --workspace --locked
cargo run -p rustsimhub-core -- --self-check
```

Linux 上 `rustsimhub-core --probe` 读取系统与 ModemManager 状态，输出仅包含
硬件能力及计数；不会拨号、发送短信或切换无线电。原始私有调试材料不进入仓库。
