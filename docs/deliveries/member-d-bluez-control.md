# D-22/24局部实现：固定BlueZ控制端口

2026-10-30。用户要求自主连续推进；前置待审期间继续可独立验证的BlueZ控制部分。

- 仓库/分支：Viper-Boss/RustSimHub / `codex/gtw-bluez-control-20261030`，到main的依赖草稿。
- 前置#105完整HEAD：`22663d12f95a191b44188d60965d069def0d5606`。
- 起始main仍`6b5de5f3ca5ff706adacd8f25ed7f638546739a9`，#105当时未合并。
- 实现/整仓受测HEAD：`9e7796d77e185d04328b046199ace85ab904f100`。
- 当前完整40位HEAD见PR正文；之后仅澄清lib crate级说明和补本人报告，执行逻辑/测试未变。
- [API、依赖、期限和真实运行边界](../ble/bluez-control.md)。

## 代码与验证范围

仅ble显式feature下的控制端口/私有总线测试、其local Cargo依赖边和本人文档。
默认纯库不增加运行依赖，Core二进制没有调用；没有wire/schema、服务/Root安装器变化。
四固定方法只对捕获的unique owner、Hci0和固定manifest路径调用，options空、错误有限。
订阅broker固定NameOwnerChanged、队列32，读写调用前后核owner；瞬时丢失/取回也不复活。
新RPC端口不提供本地Gatt对象/回调或通知，因此不宣称已实现完整BlueZ/GattBackend。

新增11项实际私有dbus-daemon测试，zbus发送/接收与name变化真实，外部BlueZ manager合成。
初版owner快照不足的回归已实际退出101，修正后同测试通过；没有放宽断言。
包53项通过、严格Clippy通过；完整整仓 **827通过、0失败、6项继承忽略**，
整仓严格Clippy/fmt/diff通过；默认库编译通过、正常依赖树仅本地ble自身。
总数对应#105的816+本批11，不含其他待审分支。六项ignore继承C06/forwarder/Root/C14，
无新增ignore，既有proc-macro-error2 future-incompat提示保留。

日志仅本地work/bluez-control，未提交生成物，摘要：

```text
workspace.log        d0dd1f5aaf1f762d20f2abf42ad536d524d3ed1ce48771b78a59a687bc50e23e
workspace-clippy.log 6633e17a1b1b3be225b6dbdcd49528ed7e83aecb67834fcdb65fbf3dade0d8c0
ble-final.log        89d3e17afa7111c9a554f824e7981b8a4236f3ccc48e0b3bbfec6f4d8f244e23
default-check.log    5caca421c8d5c145be1248ccb6b26d01c4062c0c370443fa418ad5fd973b6d88
default-tree.log     3fe01e227cbe88de37bae89a4f169908ec9d02503751507f4a35cb3a544aaeb0
owner-gap-before.log 251a056532315ae7371e26f1205b13d68c2e6213c0122ff21581a613d03c8db1
```

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
