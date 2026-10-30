# D-22 局部交付：断连失效与拥有型 Core 桥

2026-10-30，用户要求自主连续推进；完成本批后接下一独立包，不再等待用户逐批催促。

- 分支：Viper-Boss/RustSimHub / `codex/gtw-recovery-channel-20261030`，目标按真实前置状态提交PR。
- 前置 #97 完整 HEAD：`bfbfdaae25daf581689d885332a98f2b6d3d6f4b`。
- 开工时 main：`6b5de5f3ca5ff706adacd8f25ed7f638546739a9`，#97当时尚待审核。
- 实现/整仓受测 HEAD：`4fb7205aa79495e9a19fc4a235cd77109f4e41b9`。
- 当前完整40位 HEAD 见 PR 正文；源码实测后只补本人文档。
- [API、执行/取消语义、来源与未运行范围](../ble/recovery-channel.md)。

## 实际实现

Auth token 及当前连接共享私有 live 原子标记；Drop/invalidate 不需要全局 Auth 锁。
配置/关闭/Auth销毁使标记失效，能力/证明在Store复核前后复核存活；旧 token 不影响新连接。
失效不提前释放在途工作 Permit，下一连接仍须等待实际 drain，不能重置为不限并发。

Core 新 RecoveryChannel 只能接受实际 SourceGuard 的 PeerConnection。同步 attach/prepare，
外部有界 verify 无 Auth 借用，同步 finish/check 重新授权。拥有型 I/O 的取消/错误/Drop
随 token 失效；待交付结果先 fence，再清理结果/Permit，最后关 peer/放槽。
没有新 listener/CLI、生产 verifier 或 Wi-Fi 派发，不改 AuthCommand/表/版本/依赖/默认启动。

## 验证

新增5项生命周期测试，Auth70项全部通过；Auth/Core严格all-targets/all-features Clippy通过。
实际 `dropping_transport_token_fences_capability_even_while_auth_mutex_is_held` 在修正前
退出101：drop 后能力检查仍非 Forbidden；修正后通过，没有放宽断言。
其他覆盖重复invalidate、实际阻塞工作保留Busy、旧token迟到Drop、配置/Auth释放。
测试只合成外部密码返回；没有伪造 SourceGuard 合法 UID 或建一个测试专用状态机。

```sh
source /workspace/toolchains/activate-rustsimhub.sh
cargo test -p rustsimhub-auth --features recovery-grants --locked --offline
cargo clippy -p rustsimhub-auth -p rustsimhub-core --all-targets --all-features --locked --offline -- -D warnings
cargo test --workspace --all-features --locked --offline
cargo clippy --workspace --all-targets --all-features --locked --offline -- -D warnings
cargo test -p rustsimhub-auth -p rustsimhub-storage --no-default-features --locked --offline
cargo check -p rustsimhub-core --lib --no-default-features --locked --offline
cargo fmt --all -- --check
git diff --check
```

上述整仓 **798通过、0失败、6项继承忽略**，整仓严格Clippy/fmt/diff退出0；
默认无feature Auth/Store共87通过、0失败/忽略，默认Core库编译通过。
六项ignore继承C06/forwarder/Root/C14；本批无新ignore。保留既有proc-macro-error2
future-incompat提示。本地日志位于work/recovery-channel，未提交生成物，摘要：

```text
workspace.log        c56bfbe60a80319dcc4783ba4dbbabcc6029b8004c7ecf9df30b229791a80732
workspace-clippy.log 615625d6b2cf953941eb050cf1b98f38df1d5f1e4451c79981ba4db520ed3f46
default.log          8f256c4292402dc6ac3caf2d12395da5ac5b99518401bd4e40f95fe9ce824199
default-lib.log      cd8afdd142f9064d2b095f3db2250c15cb56308e561e2a9464f5278b00af9260
token-before.log     662724e6dee8905ea2b85532b88a98c02ed99722abd7d1713e1c5c3bea3c325b
```

PR以#97为前置；未合并前保持草稿到main并明确包含前置，不把同一包重复计数。
普通Linux UID1000、Rust1.98、锁定离线缓存；环境重启后已复核网络就绪和未提交改动保留。

**未运行**：不同UID合法SourceGuard→Core桥全链路、NSS/组继承/cgroup、Root、A互操作/
GATT/BlueZ、ARM/Native、release panic、设备/NM/实际Wi-Fi/远程媒体。桥本身经过编译/
Clippy，不能以Auth组件正例称桥全链路通过。没有 newuidmap，不尝试伪造内核UID补正例。

## 数量、后续与来源

本批完整步骤新增0，仍 **21/26离线包已提交，剩 D-22～26 共5步**。
原固定P0–P7约80%、main台账19/169不变；不改全局进度/名册/其他成员文件或分支。
D-22仍缺实际跨UID/A会话/GATT；接下来推进D-23唯一事务/持久Unknown的兼容接口和失败样例。

新增代码AGPL-3.0-only，复用本仓库#95/#97，无新第三方依赖/密码原语/锁文件。
OpenAI Codex辅助实现与实测，无作者改写或工时虚报。没有设备/NAS/生产数据、业务网络/
短信/拨号、Root安装、签名或部署。原日志仅本地work；远程CI按最终HEAD另行回读。
