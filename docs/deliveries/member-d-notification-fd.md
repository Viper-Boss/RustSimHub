# D22/24 局部交付：专属通知 FD 候选

2026-10-30。Viper-Boss/RustSimHub，分支 `codex/gtw-bluez-notify-fd-20261030`，
到 main 的依赖 #117 草稿。起始完整 HEAD
`a2732cc2a23693b16f209225b9463199192223cf`，含 main
`80db24e9b101dd3fb4b8c951aa7f1e0e1858e65d`。
执行代码/整仓受测完整 HEAD：**`dfa31099feb40e812f6a026a744cfd63664cc4ee`**。
最终完整 HEAD 见PR正文，受测后仅本人文档变化。

## 交付行为

显式Linux `bluez-notify-fd` 为AuthExchange/Result实现真实标准AcquireNotify，
按实际header sender/同一私有Device1租约/epoch/peer交付专属SEQPACKET FD；
固定MTU/包预算、NONBLOCK/CLOEXEC、每特征一次获取及全局单在途发送。
保留静态元数据先导出再一次性绑定peer的启动顺序。取消/失败/退休不能重用旧channel，
始终没有全局Value信号/共享通知回退，反向FD字节不进入命令或安全后端。
完整5秒前置/可写/提交/后置预算；错误可能发生在数据已提交后，不自动重发或清Unknown。
这只证明本地专属FD提交，不证明ATT/浏览器/业务接收。
[详细API、资源与实际边界](../ble/notification-fd.md)。

只改ble显式feature、nativeFD边界/私有总线测试、最小Cargo依赖边及本人文档。
重用锁定socket2 0.6.5（Rust1.70）与libc 0.2.190（Rust1.65），许可证实际读取均
MIT OR Apache-2.0；libc仅用MSG_NOSIGNAL常量，无本crate unsafe。
Cargo.lock只给本地ble加2行依赖，无第三方包/版本/来源变化，另启用已有tokio/net。
默认正常依赖树仍仅ble自身，无默认服务启动/Root材料/wire/schema/密码格式变化。
新Rust源码AGPL-3.0-only/Codex辅助，未复制上游C实现，原作者/许可保留，无工时虚报。

## 实际验证

新增 **18项**真实生产Rust路径的私有D-Bus/SCM_RIGHTS/内核SEQPACKET测试；
BLE **141通过**；完整workspace **945通过、0失败、6项继承忽略**。
本分支不含#101；前一联合932的精确5680948也不含本FD实现，不能混算。
整仓严格Clippy、仅gatt/peer/control严格Clippy、默认库编译/依赖树、fmt/diff通过。
无新增ignore，继承proc-macro-error2 future-incompat提示。

覆盖标准FD返回/实际type/flags/MTU、两包/双特征隔离、真实外来sender/第二peer/
重复/错误输入、缺FD/错误epoch/peer/kind/超预算、HUP/反向数据、未绑定阶段、
未poll/取消/单发送许可、retire/Drop、device/owner gap、获取中退休、内核小缓冲
真实背压截止，以及**对端已收到包而发送方后置查询超时仍返回Deadline**。
外部BlueZ/Device1与宿主回复合成；没有用假UID/PeerContext/VerifiedSession证明正例。

```sh
source /workspace/toolchains/activate-rustsimhub.sh
cargo test --workspace --all-features --locked --offline
cargo clippy --workspace --all-targets --all-features --locked --offline -- -D warnings
cargo clippy -p rustsimhub-ble --all-targets --features bluez-gatt --locked --offline -- -D warnings
cargo clippy -p rustsimhub-ble --all-targets --features bluez-peer --locked --offline -- -D warnings
cargo clippy -p rustsimhub-ble --all-targets --features bluez-control --locked --offline -- -D warnings
cargo check -p rustsimhub-ble --no-default-features --locked --offline
cargo tree -p rustsimhub-ble --no-default-features --edges normal --locked --offline
cargo fmt --all -- --check
git diff --check
```

Linux x86_64、UID1000、Rust1.98，锁定离线缓存；日志只在work/bluez-notify-fd：

```text
workspace.log 87dfe03a6823501f4a02009087360e93f6c6bab68265f9bf8da1ade448956326
workspace-clippy.log aa83b893b650b51dc2df38be3cbcc1c5e8d45d8bf8056eb786b159dd601f93ee
gatt-only.log ec862205af2b81338829234a985a3bb3d8472cb96e96daafd30f54baa47bb88f
peer-only.log a525f3e014efd4f3db9947fceef79c958b06621b5ca84e92e9d9e73d89f10201
control-only.log b0dac1751c7b14adf0c06b2f879c9562a3f2d71e81628606fa7469c1e022cf28
default-check.log 8fd2f238e6e311a084c56e2a641c8b18ce631a72ebadb9ed425a35375e2cbb4e
default-tree.log 3fe01e227cbe88de37bae89a4f169908ec9d02503751507f4a35cb3a544aaeb0
```

## 未完成与继续点

未启动系统总线/主机BlueZ/HCI/RF，未验证目标版本/UnixFD协商/真实ATT/浏览器/
ARM/native设备、实际资源预算/角色UID/Root/NSS/polkit/cgroup、A互操作或Core-Wi-Fi端到端。
还需可信宿主处理Connected与owner事件、原Transport/Lifecycle/SecurePort/Core bridge
接线和清理，缺A端口仍不得启动完整恢复服务。本地shutdown不证明radio quiescence。
所有私有daemon自建并kill/wait；无生产设备/数据/NAS、短信/拨号/流量、签名/部署。

完整步骤新增0；仍 **21/26离线包已提交、剩5步，固定P0–P7约80%**。
不改其他成员/全局台账，依赖未合并前草稿，不自动合并；CI按最终HEAD回读。
下一项检查隔离容器能否补真实不同UID的Core来源/owned-channel验证，不选择生产UID。
