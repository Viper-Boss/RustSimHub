# D-22/24局部实现：固定BlueZ控制端口

2026-10-30。用户要求自主连续推进；前置待审期间继续可独立验证的BlueZ控制部分。

- 仓库/分支：Viper-Boss/RustSimHub / `codex/gtw-bluez-control-20261030`，到main的依赖草稿。
- 前置#105完整HEAD：`22663d12f95a191b44188d60965d069def0d5606`。
- 起始main仍`6b5de5f3ca5ff706adacd8f25ed7f638546739a9`，#105当时未合并。
- 实现/整仓受测、当前完整40位HEAD在提交后记录于PR正文。
- [API、依赖、期限和真实运行边界](../ble/bluez-control.md)。

## 代码与验证范围

仅ble显式feature下的控制端口/私有总线测试、其local Cargo依赖边和本人文档。
默认纯库不增加运行依赖，Core二进制没有调用；没有wire/schema、服务/Root安装器变化。
四固定方法只对捕获的unique owner、Hci0和固定manifest路径调用，options空、错误有限。
订阅broker固定NameOwnerChanged、队列32，读写调用前后核owner；瞬时丢失/取回也不复活。
新RPC端口不提供本地Gatt对象/回调或通知，因此不宣称已实现完整BlueZ/GattBackend。

新增11项实际私有dbus-daemon测试，zbus发送/接收与name变化真实，外部BlueZ manager合成。
初版owner快照不足的回归已实际退出101，修正后同测试通过；没有放宽断言。
包53项通过、严格Clippy通过；最终整仓结果/日志结束后补充。

```sh
source /workspace/toolchains/activate-rustsimhub.sh
cargo test -p rustsimhub-ble --features bluez-control --locked --offline
cargo clippy -p rustsimhub-ble --all-targets --all-features --locked --offline -- -D warnings
cargo tree -p rustsimhub-ble --no-default-features --edges normal --locked --offline
cargo test --workspace --all-features --locked --offline
cargo clippy --workspace --all-targets --all-features --locked --offline -- -D warnings
cargo check -p rustsimhub-ble --no-default-features --locked --offline
cargo fmt --all -- --check
git diff --check
```

Linux x86_64、UID1000、Rust1.98，锁定离线缓存；默认正常依赖树实测仅rustsimhub-ble自身。
锁文件只增加本地ble包的4条已有依赖，无第三方包版本/来源变更。
许可实读缓存Cargo.toml：zbus/tokio MIT，futures-lite与测试tempfile为MIT/Apache-2.0。
无新密码原语、外部代码复制或依赖下载，保留原库作者，新增源码AGPL-3.0-only。

未运行主机BlueZ/D-Bus/HCI、真实权限/不同UID/Root/NSS/polkit/cgroup、ARM/Native、
实际BLE/密码/A互操作、Core/Wi-Fi端到端。所有daemon由本测试创建并kill/wait，不访问系统总线。
没有生产数据/设备/NAS、业务网络/短信/拨号/流量、Root账户/签名/部署。

## 数量与继续点

完整步骤新增0；仍 **21/26离线包已提交、剩5步**，原固定P0–P7约80%、main19/169保持。
本批推进实际控制调用，#105依赖未合入前保持草稿；不改全局台账或他人分支。
下一部分是同一连接固定GATT对象导出/回调与owner/epoch门禁，再结合#101及A会话接线；
接口/Root角色审批、实际设备验证保持。RPC迟到/超时不表示未执行，不自动重发未知注册。
Codex辅助实现/实测，无工时虚报或作者改写；原日志仅work/bluez-control，CI按最终HEAD回读。
