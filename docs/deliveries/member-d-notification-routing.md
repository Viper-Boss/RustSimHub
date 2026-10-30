# D22/24 通知接口提案与真实失败样例

2026-10-30。Viper-Boss/RustSimHub，分支 `codex/gtw-bluez-notify-routing-20261030`，
到 main 的依赖 #116 草稿。起始完整 HEAD
`bdf2de4a08bad08d6d70d588bb3bbae8ce3714ef`，含 main
`80db24e9b101dd3fb4b8c951aa7f1e0e1858e65d`。
样例/包受测完整 HEAD `7e8fede4a2871ed3226802cd59e9f8488e482271`；
最终完整 HEAD 见PR正文，受测后仅本人文档变化。

仅新增 cfg(test) 通知路由样例及本人提案/报告。**生产执行代码、依赖、Cargo.lock、
协议、默认值、服务和Root材料均无变化**。没有实现真实AcquireNotify/notify。
[具体接口、上游不可变证据、失败样例及验收边界](../ble/notification-routing-proposal.md)。

新增六项真实私有总线/内核 Unix FD 样例，BLE **123通过、0失败**；
包 all-targets/all-features 严格Clippy、整仓fmt/diff通过。
此批只有测试/文档，未重复整仓测试；前置#116真实整仓受测HEAD
`d8135f4b366c5edbd260d3b4809b06a7d20af468` 为921/0/6，
**不把921+6算作本批已运行927，也不计未合并#101**。

```sh
source /workspace/toolchains/activate-rustsimhub.sh
cargo test -p rustsimhub-ble --all-features --locked --offline
cargo clippy -p rustsimhub-ble --all-targets --all-features --locked --offline -- -D warnings
cargo fmt --all -- --check
git diff --check
```

Linux x86_64、UID1000、Rust1.98、锁定离线缓存，日志仅 work/bluez-notify-routing：

```text
delivery-ble.log d48cf81ff1693b246e6d90741a693655d6354dbc9ee39cd889ecd62102e2de16
delivery-clippy.log fecede7c2157efe2f16c8c8b9dce94b0fb4dd384dca8207046c8bf7636eb0ffe
```

真实broker/属性信号/FD跨D-Bus传递/内核包隔离，外部BlueZ/Device1/宿主回复合成；
测试FD接口为独立test路径，不是生产AcquireNotify，不伪造PeerContext/UID或VerifiedSession。
没有主机BlueZ/HCI/RF/真实ATT、SOCK_SEQPACKET/HUP/背压、目标版本/ARM/native、
Root/NSS/polkit/cgroup、A互操作/Core-Wi-Fi端到端。所有daemon自建并kill/wait。
无生产设备/数据/NAS、短信/拨号/流量、签名/部署或全局台账变更。

完整步骤新增0，仍 **21/26离线包已提交、剩5步，固定P0–P7约80%**。
Codex辅助、AGPL-3.0-only新测试、原许可/作者保留，无工时虚报或复制上游实现。
前置未合并前草稿，不自动合并；CI按最终HEAD回读。
后续继续独立feature下的按peer FD候选，生产/Root/密码与实际验收仍按原门禁。
