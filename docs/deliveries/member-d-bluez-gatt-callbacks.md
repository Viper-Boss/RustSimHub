# D22/24 局部交付：固定 GATT 回调边界

2026-10-30，Viper-Boss/RustSimHub，分支 `codex/gtw-bluez-gatt-callbacks-20261030`，
到 main 的依赖 #110 草稿。起始/前置完整 HEAD
`fce922512fb52bb0eb5d12de951f30707390f1a8`，包含 main
`80db24e9b101dd3fb4b8c951aa7f1e0e1858e65d`。
执行代码与整仓受测 HEAD `dd07090a011de4fc4302ef82f061c663e1b5a839`；
最终完整 HEAD 见 PR 正文，受测后仅本人说明文档变化。

## 交付范围

仅 ble 显式 bluez-gatt feature、本地对象/回调/私有总线测试及本人文档。
[API、输入门禁、所有权和宿主待办](../ble/bluez-gatt-callbacks.md)。
固定 ObjectManager、service、四 characteristic，UUID/Flags 来自既有 manifest。
实际 sender 与连续 owner 验证；固定签名/严格 options/属性预算；公开 DeviceInfo
长读，不透明 Write/Result 交容量1宿主队列，单槽 permit 保留至真实工作完成。
5秒固定期限和私有请求 fence，截止后的 check_current 失败，但不丢弃在途工作；
错误/缺宿主/回复类别不符无成功替身。通知明确 NotSupported，无 Value 属性缓存。
本 API 不校验帧业务内容，不赋予密码或 Core 能力；非完整 GattBackend/daemon。

没有新增 wire/schema、密码原语、服务/Root 材料、Core 默认调用；
Cargo.lock 无变化，无新第三方包/版本/来源，只增加本地 feature 和已有 tokio/time
feature 边（MIT，已有依赖与许可保留）。默认正常依赖树仍仅 ble 自身。
保留原作者，新源码 AGPL-3.0-only/Codex 辅助，无工时虚报。

## 真实验证

新增 **19 项**真实私有 dbus-daemon/zbus 测试，包 **95 通过**；
完整 workspace **899 通过、0 失败、6 项继承忽略**（前置880+19）。
整仓/仅control/仅advertisement all-targets 严格Clippy、默认库编译/依赖树、fmt/diff
全部通过，无新增 ignore，既有 proc-macro-error2 future-incompat 提示保留。
初版实际退出101：82通过/8失败，暴露 UUID 属性命名及多参数 body signature
判定问题；保留原断言修正，没有扩大期限或放宽输入规则。
私有 broker、header sender、名字变化、对象属性、真实5秒期限、连接关闭实际运行；
只有外部 BlueZ manager 和可信宿主回复合成，不冒充密码/业务正例。

```sh
source /workspace/toolchains/activate-rustsimhub.sh
cargo test -p rustsimhub-ble --all-features --locked --offline
cargo test --workspace --all-features --locked --offline
cargo clippy --workspace --all-targets --all-features --locked --offline -- -D warnings
cargo clippy -p rustsimhub-ble --all-targets --features bluez-control --locked --offline -- -D warnings
cargo clippy -p rustsimhub-ble --all-targets --features bluez-advertisement --locked --offline -- -D warnings
cargo check -p rustsimhub-ble --no-default-features --locked --offline
cargo tree -p rustsimhub-ble --no-default-features --edges normal --locked --offline
cargo fmt --all -- --check
git diff --check
```

Linux x86_64、UID1000、Rust1.98、锁定离线缓存；日志只在 work/bluez-gatt：

```text
workspace.log 93d999f0f551f3916cf97c9af0e012125481a955ae2ce85a2d475a81d7fd38dc
workspace-clippy.log fc28d0ef426abde04ffb753f66a13d2faed6edac2c05f36060e99ba45fde306e
ble.log ebc9140286644961d748b9befc4e8d5145a27652297f4843288e0a9ac36d2748
control-only.log 976d02c464a5b01f76efc5c0aa328c9028d4a60b8257d4177832dd3bde873dde
ad-only.log 6317b24c4ba503dd373258de399dd12e7fcee0fe9af47c80c789067f623c03a0
default-check.log ba2d81e8ec06e79e86e227659f02b9e0b8098000ba0d86ea384558b7eee6da40
default-tree.log 3fe01e227cbe88de37bae89a4f169908ec9d02503751507f4a35cb3a544aaeb0
initial-boundary-failures.log 6ad4320194fdcaedff8a0f1ee6d14e36b8e76c2b75b732b594c805339589c283
```

## 未验收与继续点

物理 peer 连接跟踪尚未接入，队列中的 device 路径只有形状/来源校验，不能作为
连接代次证据。宿主必须在断开/状态不明先退休本导出，排空旧工作并关闭专用连接，
新连接/new epoch 再导出，不能重贴旧请求到同路径重连设备。仍需真实 Device1
事件、Lifecycle::Connection、原 Transport/安全会话/Core capability 接线。
缺 SecurePort 仍不得启动完整服务，通知尚不可用。
下一步在独立验证分支与 #101 的 Core 失效围栏一起合并回归，再推进实际连接事件入口。

未访问主机系统总线/BlueZ/HCI/RF、真实设备/角色UID/Root/NSS/polkit/cgroup、
ARM/native、A密码互操作、Core-Wi-Fi端到端；所有daemon由本测试创建并kill/wait。
无生产数据/数据库/NAS、短信/拨号/流量、签名/部署，不改其他成员或全局台账。
完整步骤新增0，仍 **21/26 离线包已提交、剩5步；固定P0–P7约80%**。
依赖未合并前草稿，不自动合并；远程CI按最终HEAD回读，不能替代离线/设备验收。
