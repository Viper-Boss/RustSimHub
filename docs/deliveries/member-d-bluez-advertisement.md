# D22/24 局部交付：固定广播对象

2026-10-30。分支 `codex/gtw-bluez-advertisement-20261030`，目标 main，前置 #107。
前置完整 HEAD `241803809029dc7da20a611baef6b476cc3c901f`；本批执行代码
`6b0843331bb52bb9eec2125b24f092e00c55efe2`；当前完整 HEAD 见 PR 正文。
实际已正常合并最新 main `80db24e9b101dd3fb4b8c951aa7f1e0e1858e65d`，无冲突，不覆盖他人修改。

## 实现与边界

只改 ble 显式 feature、回调所需控制绑定内部复用、真实私有总线测试和本人文档。
[API/所有权/连接与测试边界](../ble/bluez-advertisement.md)。
固定两项只读广播属性；Release 从实际 header 校验 sender，复用当前 owner 连续性
检查，投递一次 epoch/owner 事件。接收方退出、错误 sender、换代和额外载荷失败。
固定路径保留退休对象直到连接关闭，不能重绑新 epoch；取消发布也 fail closed。
Weak 引用避免对象服务器与控制连接环；局部退休不宣称无线清理完成。
不新增依赖、锁文件、密码原语、wire/schema、Core 默认调用、服务或 Root 材料。

15 项新增真实私有 dbus-daemon 测试通过，包合计 68 通过、0 失败；
包严格 Clippy 及仅 bluez-control feature 的编译通过。
并入最新主线后的整仓受测 HEAD `6227515ee9b9bfe420b4207b0e0bc5ef6ba47e81`：
**872 通过、0 失败、6 项继承忽略**，整仓 all-targets/all-features 严格 Clippy、
默认库编译、默认正常依赖树（仅 ble 自身）、仅 control feature 严格 Clippy、fmt/diff 均通过。
总数是此 HEAD 的实际合计，包含主线本次新变化，不能与旧分支总数简单相加。
无新增 ignore，保留既有 proc-macro-error2 future-incompat 提示。
之后仅补本人文档，不改变执行逻辑或测试。

```sh
source /workspace/toolchains/activate-rustsimhub.sh
cargo test -p rustsimhub-ble --features bluez-advertisement --locked --offline
cargo test --workspace --all-features --locked --offline
cargo clippy --workspace --all-targets --all-features --locked --offline -- -D warnings
cargo check -p rustsimhub-ble --no-default-features --locked --offline
cargo tree -p rustsimhub-ble --no-default-features --edges normal --locked --offline
cargo clippy -p rustsimhub-ble --all-targets --features bluez-control --locked --offline -- -D warnings
cargo fmt --all -- --check
git diff --check
```

Linux x86_64、UID1000、Rust1.98、锁定离线依赖。
日志仅本地 work/bluez-advertisement，SHA256：

```text
workspace.log c134d3b1eba463033ce55b1de82f285f9fe686623bdde4a32475e286c810d3a3
workspace-clippy.log b016974abbd5dfc8cfbb3f6ee29811c698d8eb6b07cc593482d307fe27b2b776
ble.log 3f658eae0787fa7e2debddeea9736f51999f814dc2b5e33145fad919dcfbc79d
default-check.log a2abd81fd6b17ba0ef384487236f44ef2bc67242df9cd0b01af01f7b4949022e
default-tree.log 3fe01e227cbe88de37bae89a4f169908ec9d02503751507f4a35cb3a544aaeb0
control-only-clippy.log 4569d096a27a6ad469f035d232594114cc48087e0150636dc7bc66f35aaef332
```

未运行主机系统总线/BlueZ/HCI、真实 UID/Root/NSS/polkit/cgroup、ARM/native、
RF、密码/A 互操作、GATT 通知、Core/Wi-Fi 端到端。
不访问生产设备/数据库/NAS，不拨号/短信/蜂窝流量、不签名或部署。
所有私有 daemon 仅由对应测试创建并 kill/wait。

## 数量

完整步骤新增 0；仍 **21/26 离线包已提交、剩 5 步**。
固定 P0–P7 约 80%，不改全局台账或成员分配。
Codex 辅助实现及实测；新增源码 AGPL-3.0-only，原作者和许可证保留，
无工时虚报；本地 work 日志与构建产物不提交。
下一部分继续固定 GATT 对象、实际 callback 门禁与宿主生命周期接线；
缺 A 安全会话时仍不得启动完整恢复服务，设备/角色验收保持待办。
