# D22/24 局部交付：空闲 owner 丢失等待

2026-10-30，仓库 Viper-Boss/RustSimHub，分支
`codex/gtw-bluez-owner-loss-20261030`，到 main 的依赖 #109 草稿。
起始/前置完整 HEAD `2d4b0c6e4e130fd07ff1c523c144e6df3fe87475`，
其中已合并 main `80db24e9b101dd3fb4b8c951aa7f1e0e1858e65d`。
执行代码与整仓受测 HEAD `038dc0d5c10db509bcd697b6dbfd612e44b6cfa9`；
最终完整 HEAD 见 PR 正文，之后仅本人说明文档变化。

只改 ble 控制端口/私有总线测试与本人文档，无第三方依赖/锁文件/协议/迁移/
Core 默认调用/服务/Root 材料变更。[API 与生命周期责任](../ble/bluez-owner-loss.md)。
没有 spawn；宿主显式 poll 一个有界订阅等待，不持 RPC 检查锁跨空闲 await。
原 receiver 保留 bind 后历史，独立 receiver 只用于空闲唤醒，单等待槽 RAII，
显式 retire/其他入口退休通过 Notify 唤醒，取消只释放等待槽，不退休活绑定。
没有跟随新 owner、自动重试或清理成功声明。

新增 8 项真实私有 dbus-daemon/zbus 测试，包 **76 通过**；整仓
**880 通过、0 失败、6 项继承忽略**，为前置实测 872 + 本批 8。
整仓/仅 control feature 严格 Clippy、默认库编译、fmt/diff 通过，无新增 ignore，
继承 proc-macro-error2 future-incompat 提示。

```sh
source /workspace/toolchains/activate-rustsimhub.sh
cargo test -p rustsimhub-ble --all-features --locked --offline
cargo test --workspace --all-features --locked --offline
cargo clippy --workspace --all-targets --all-features --locked --offline -- -D warnings
cargo clippy -p rustsimhub-ble --all-targets --features bluez-control --locked --offline -- -D warnings
cargo check -p rustsimhub-ble --no-default-features --locked --offline
cargo fmt --all -- --check
git diff --check
```

Linux x86_64、UID1000、Rust1.98、锁定离线缓存。日志仅 work/bluez-owner-loss：

```text
workspace.log 91f51af67081cdc52ef05c20ce74330663221959bb1eb743dcb2ae61a4613b0d
workspace-clippy.log 21d0e9bcce1ac2459aaa666c1fd6461a2903149096c15d3b9c99bba4dc9e83ab
ble.log a1bd56080ab337207e88c39fd56f17cc04c77586553c123b49b145a41c0f5759
control-only.log d695a1ad2bf1d16b88028e0324d87c23cb15f7cbe9a059a576278a43ad65ba8c
default-check.log a5f4c585ee974ca44916ac30a98bbc189e067a7e0a6bc6d2e8d6bc525be724af
```

真实 broker/发送者/名字变化/连接关闭，仅 BlueZ manager 为合成端点。
所有测试 daemon 本轮创建并 kill/wait。没有系统总线、主机 BlueZ/HCI/RF、
真实角色 UID/Root/NSS/polkit/cgroup、ARM/native、密码/A 互操作、Core-Wi-Fi
端到端或完整 GattBackend/daemon；没有生产设备/数据/NAS、短信/拨号/流量、
签名或部署。等待返回不证明无线清理或安全会话撤销，宿主必须驱动原生命周期。

完整步骤新增 0，仍 **21/26 离线包已提交、剩 5 步，固定 P0–P7 约80%**。
Codex 辅助实现及实测、新增源码 AGPL-3.0-only，保留原作者/许可、无工时虚报，
不改全局台账/成员范围。前置未合并前保持草稿，不自动合并。
下一批继续固定 GATT 对象与回调门禁；A 密码、主线权限/设备审批仍按原分工。
