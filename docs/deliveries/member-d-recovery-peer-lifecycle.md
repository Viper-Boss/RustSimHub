# D-22 局部交付：超时/取消后的验证工作保留

2026-10-30。用户要求继续，核对 #92 已由负责人合并后，从最新 main 建立独立分支。

- 仓库/分支：Viper-Boss/RustSimHub / `codex/gtw-recovery-peer-lifecycle-20261030`，PR 到 main。
- 起始 main：`b7a5dfa8d0d8c762c2284164e9d4371fce3c373f`。
- 前置 #92 合并：`d4d1673c58049f54c4de94b3f3adee6e41379bb2`。
- 初始实现 HEAD：`c88e04e7d4b96123f0678dd24e4755109119bc99`。
- 最终代码/回归夹具 HEAD：`c4a978a7a93ff83271faf5634c3abf89ea83e23f`。
- 当前完整40位 HEAD 在 PR 正文记录，后续报告提交只补证据。
- [实现与完整限制](../ble/recovery-verifier-lifecycle.md)。

## 修改和授权边界

仅 Core 的显式 recovery-transport 子模块及本人文档；不修改 Auth/Store/wire/A 密码库、
其他成员实现、全局进度或原有分支。默认 Core 不调用，仍无恢复监听器/验证器配置或
Wi-Fi 派发。无新依赖、锁文件、迁移或 Root 安装材料。

以线程拥有连接/Packet 的方式实现 verify_blocking：固定五秒截止，入口/线程开始/交付
均复核期限；失败为有限错误；超时/取消只停止等待，原资源一直保留到工作真的
结束，迟到输出丢弃。不自动重试、不伪造业务未执行或关闭完成。
独立线程不占 Tokio 的共享阻塞池；成功输出仍需要后续 Auth/grant/连接复核，不是能力。
永久卡住则该 Guard 一直占用；没有强杀、生产监督/cgroup 或可配置任意插件。
仅 unwind 构建的 panic 能映射 Worker；release 的 panic=abort 仍终止整个进程，
普通 panic hook 仍运行，不能记录秘密。本批没有更改 profile/hook 或声称线程是进程隔离。

## 实测

Linux x86_64 / 普通 UID1000 / Rust1.98，锁定离线依赖。新增10项工作所有权测试，
加上原14项运输测试共24通过、0失败/忽略，Core all-targets/all-features 严格 Clippy通过。
没有构造假的 PeerContext/PeerConnection。正例直接测试本生产模块 run_owned，以真实
UnixStream/Slot/线程检验所有权；外部适配器仅合成返回/阻塞/panic，不能证明密码验证成功。

迟到输出析构回归 `late_output_cleanup_finishes_before_the_connection_slot_is_released`
在初始 tuple(resource,result) 顺序实际退出101：`late output cleanup released slot too early`。
修正 Completed 字段顺序后通过；不放宽拒绝/保留断言。最后代码之后没有生产源码修改。

实际执行命令：

```sh
source /workspace/toolchains/activate-rustsimhub.sh
cargo test -p rustsimhub-core --lib --features recovery-transport --locked --offline
cargo clippy -p rustsimhub-core --all-targets --all-features --locked --offline -- -D warnings
cargo test --workspace --all-features --locked --offline
cargo clippy --workspace --all-targets --all-features --locked --offline -- -D warnings
cargo check -p rustsimhub-core --lib --no-default-features --locked --offline
cargo fmt --all -- --check
git diff --check
```

在最终代码 HEAD 上整仓 **774通过、0失败、6项继承忽略**；整仓严格 Clippy、默认无
feature 库编译、fmt/diff均退出0。源码相对该 HEAD 无后续变化，后续只补本人文档。
六项 ignore 是 C06 worker、forwarder 私有 tmpfs 满盘、updater 两项 Root/capability、
C14 worker 与私有 tmpfs 满盘，本批没有新增 ignore。既有 proc-macro-error2 future-incompat
提示仍存在，不作为代码测试失败或已修复项。未重跑浏览器：本批没有 JS/网页改动。

本地日志位于 work/recovery-peer-lifecycle，未提交生成物；摘要只供精确比对，不代替 CI：

```text
workspace-final.log        0270cdb3293097c6436ddd7d03b003cfc97ffdf5fd0fbebb814946a0ed90c358
workspace-clippy-final.log 0de99f512f7a1afaa290185aea36050647a372b9a778d51cd5189769c4c39236
library-final.log          13b9369c2cbf5aebcb536c7fee87e1d1afdb3226c96cf89a2c8bef8be9629097
core-clippy-final.log      7ab339545404b5eed3343a13abc98ef1f765a7c8757fd4651b7af73756206da6
default-check-final.log    a41051563a4cb5fc1b1bcdd9aa0ba28ca80f7736d629d90cc138218756e8e3a9
cleanup-order-before.log  b48932168e1ed25722b67ff6f8011255140fa3ee95b43ec2c5d1e9973b772e73
```

新增测试覆盖：成功返回同一流；适配器错误；unwind panic；超时；已 poll 取消；未 poll
丢弃；入口过期；实际耗尽 Tokio 单个阻塞池工作线程后仍可执行；等待者过期；迟到输出
析构阻塞期间仍持有连接/占用。没有扩大产品五秒截止或依赖不受控无限等待。

未运行合法跨 UID SourceGuard 准入/真实 nonce 路径、安装组继承/NSS/cgroup、ARM/Native、
release panic 实测、A 互操作、真实 GATT/BlueZ/Wi-Fi/设备。release abort 行为按实际
Cargo profile 明确保留；unwind 测试不表示生产 panic 隔离。

## 数量、未完成和交接

本批新增完整步骤0；累计仍 **21/26 离线包已提交，剩 D-22～26 共5步**。
D-22 增加实际工作保留机制，A 安全协议/互操作、真实跨 UID 准入、GATT 接线仍缺；
D-23 唯一 Wi-Fi 事务/持久 Unknown、D-24 安装许可、D-25 远程媒体和 D-26 联合验收未冒称完成。
原固定 P0–P7 约80%，主线19/169口径不变；不自行更新全局台账或验收。

单 UID user namespace 只读探测成功；尝试多 UID 映射实际因缺 newuidmap 退出127。
没有以不同命名空间内的相同底层 UID 冒充两个角色。没有安装助手/修改主机账户/NSS，
没有设备/D-Bus/NAS/生产数据或真实网络业务/短信/电话、签名或部署。

沿用 AGPL-3.0-only，复用标准库及现有 Tokio/Zeroizing，无第三方源码复制。
OpenAI Codex 辅助实现与实测，无工时虚报/作者改写。源日志仅保留本地 work，不提交生成物。
远程 CI 按最终 HEAD 单独回读，不能以本地通过代替 CI/审核/设备验收。
