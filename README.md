# RustSimHub

面向 MSM8916 / Snapdragon 410 棒子的独立通信固件：Linux + Rust 用户态，
设备自身管理电话、短信、授权和数据，支持 USB/NAS、Wi-Fi 与蜂窝网络。
手机客户端另行开发。

当前已有棒子实机开发系统，原P0–P7进度约80%；仍未形成可供其他设备直接刷写的正式发行版。已验收和未完成的范围以进度与接手说明为准。

- [详细开发计划](docs/development-plan.md)
- [进度](docs/PROGRESS.md)
- [接手说明](docs/HANDOFF.md)
- [剩余任务与 Claude 接手清单](docs/REMAINING-WORK-AND-CLAUDE-HANDOFF.md)
- [新增短信转发、NKNGuard与使用统计](docs/FEATURE-EXTENSIONS-2026-10-24.md)
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
