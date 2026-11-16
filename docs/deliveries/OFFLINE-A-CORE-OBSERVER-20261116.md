# A1 Core有限交易观察连接（2026-11-16）

本次交付完成默认关闭的Core→原Root有限观察连接子包：使用原Auth、独立参与许可和原单一AttemptBudget，
将固定用途签名交易绑定原Root注册后发送，并严格核对返回。它尚不是完整CheckInTransport，不能启用生产签到。
没有访问生产设备、NAS、公网NKN节点、真实通信数据或发送链上交易；没有部署、签名发行或自行验收合并。

## 固定来源和依赖

- 成员/任务：yh1030，离线A包A1；对应STAT/NKN相关动作的有限交易传输组件，非整个动作验收。
- 起始及本次回读main：`6e1cd19bc9dd05fd150157c422590c1a801cc0b0`，产品基线`2ae840fbffff04691ccf8cedd49d554093dd4032`保持。
- 本人分支：`Viper-Boss/RustSimHub:codex/offline-a-statistics-core-20261116`，不使用fork，不推main。
- 正常合并父HEAD：`20e0a8bdbd04d5423d2987b16d11a243aaeb2f8d`。
  双亲分别为PR266的`83ab91a557f1fe8bfd7827f527436ae54c724137`与PR265的`89a4b5c95958bc25a64d8ee61fa55eb8ad584939`。
  合并tree `714ab69242d4d5a1cda9c073ad2d87c757e74fa1`与原生Git merge-tree一致；保留两边历史和实际作者。
  两个依赖PR本次回读均仍开放、未合并；本分支包含它们，不冒称main已有这些代码。
- 实现HEAD：`13e9cd5c9c82783a7a4ddd03eac2689abddc5fc3`，tree `f80da4d0b7b4c22f785f0e5126c27f4f65b73027`。
  此tree的源码tar SHA256 `da8cc5b119e865d1e2afc71a520dddf3ea6e472adf3f92f762d7932a4def481a`。
- 最终受测代码/测试HEAD：`074ef6dfa6b61370ed588ab3d8cf795a8313da9a`，tree `ed084419c0c70ac2a05be8402952c4db406af6e2`。
  源码tar SHA256 `1e27e1dcbfd629d572298d3e5a6e69b32bd9356c9472e19e16e5c8beffdb8c26`。
  两个tree间仅增加第9项submit/query测试，生产字节完全相同。随后只补本包文档，最终PR完整HEAD在正文回读登记。

## 具体修改

本轮仅六个Core文件及A自己的两份文档；相对main的PR总差异还包括上述两笔依赖，不能混为本轮六文件范围。

| 路径 | 行为 |
|---|---|
| `crates/core/Cargo.toml` | 新增默认关闭的`usage-root-transport-candidate`，依赖原`usage-transaction-candidate`；无新依赖版本或锁变化 |
| `crates/core/src/nkn_root.rs` | 原策略保留原字节摘要；候选严格解析交易能力和独立统计公钥；Statistics引用加入代次，OFF/再获取不能复用旧观察操作 |
| `crates/core/src/nkn_root/transactions.rs` | 有限Preflight/Submit/Query、原注册绑定、严格有界单次观察I/O、持久Pending核对和撤权检查 |
| `crates/core/src/nkn_root/transactions/tests.rs` | 九项真实Auth/许可/原Machine预算与lease驱动的编解码及失败测试 |
| `crates/core/src/nkn_statistics.rs` | 当前许可核验与持原Auth/许可锁执行一次非阻塞本地写；原纯准备语义保留 |
| `crates/core/src/nkn_statistics/tests.rs` | 只扩大测试夹具的crate内可见性供上述测试复用，不增加生产绕过接口 |

外部入口不接受任意端点、方法或交易字节；Submit只接受原固定用途签名类型。
操作使用单独连接，保留原82字节RSCN头、87字节注册返回、generation/nonce/deadline/policy/PID及实际Root peer检查。
capability缺失/0时不可用，未知/null/重复/额外字段拒绝；必须原status能力和匹配独立统计公钥。
请求/响应正文最多2048字节，固定交易最多768字节；严格核原lease操作ID、许可代次、身份、意图和摘要。
Accepted不等于Confirmed，候选结论类型没有Confirmed。

预检要求所有窗口没有Pending；submit/query必须匹配已持久化的原period/window/digest。
每次I/O检查原budget、取消、真实Auth/realm/原有限Owner授权、当前许可、存储和Statistics引用代次。
本次观察期限取原总预算、原Root期限和2秒上限的最小值；读取以最多25ms间隔重查。
最终发送门禁在同一原Auth/许可和引用锁内做一次非阻塞Unix写，等待和读取不持这些锁。
这修正了初次实现的普通check→write间隙；完成OFF后不得再发请求字节。
必须读到单次观察EOF，额外帧拒绝；不在读返回前半关闭写端，避免被Root当取消。
退出仅关闭自己的观察FD，不启停SDK、不获取/释放其他业务引用，不延长原300秒权限。

## 真实验证与复测命令

WSL2 Ubuntu24.04，UID1000，Rust1.98.0；Linux原生精确源码，原Cargo.lock，TMPDIR=/tmp、TZ=UTC。
测试仅合成身份、临时文件和Unix socketpair。lease和AttemptBudget来自真实CheckInMachine，非自造公开证明。
socketpair返回使用合成注册，不能当作安装Root、真实cgroup/SDK或完整Core端到端验证。

```sh
cargo test -p rustsimhub-core --lib --features usage-root-transport-candidate --locked --offline nkn_root::transactions::tests:: -- --nocapture
cargo test -p rustsimhub-core --lib --features usage-root-transport-candidate --locked --offline nkn_statistics::tests:: -- --nocapture
cargo test -p rustsimhub-usage-stats --features transaction-candidate --locked --offline -- --nocapture
cargo clippy -p rustsimhub-core --all-targets --features usage-root-transport-candidate,email-production-candidate --locked --offline -- -D warnings
cargo check -p rustsimhub-core --no-default-features --all-targets --locked --offline
cargo fmt --all -- --check
cargo test -p rustsimhub-core --lib --features usage-transaction-candidate --locked --offline nkn_statistics::tests:: -- --nocapture
cargo test -p rustsimhub-core --lib --features usage-transaction-candidate --locked --offline nkn_root::tests:: -- --nocapture
cargo check -p rustsimhub-core --lib --target aarch64-unknown-linux-gnu --features usage-root-transport-candidate,email-production-candidate --locked --offline
```

| 检查 | 真实结果/来源 |
|---|---|
| Core观察连接 | 实现tree f80：8通过；最终tree ed：9通过、0失败/忽略；两组重叠不相加 |
| 原Core签名 | tree f80：5通过；关闭新观察feature再测同5通过，不加为10 |
| 原统计候选 | tree f80：92通过、0失败/忽略，含原回归，不是92项新功能 |
| Core严格Clippy及全仓fmt | f80和ed均退出0；默认无feature的Core all-target check在f80退出0，后续仅测试变化 |
| 原Root无新feature回归 | f80：2通过、0失败、1原有私有cgroup/UID夹具忽略；该忽略不计通过 |
| ARM64 | 最终ed的候选+email `cargo check --lib`退出0；不是链接程序、QEMU或设备运行 |

九项覆盖：碎片读和u64 nonce边界、原预算取消中断、读等待期间真实OFF、check/write间完成OFF零字节、
错误注册generation/nonce/policy/deadline/PID及超长/尾帧、严格回复null/重复/未知/confirmed/跨意图、
缺能力/缺引用/外来Auth拒绝且不启动、跨窗口Pending/原摘要门禁、真实纯签名交易的submit/query字段及回包。

四个独立受控候选mutation均实际断言失败退出101，无编译错误；不是旧生产实现失败证明：

| 受控错误 | 触发断言 | 变更文件SHA256 |
|---|---|---|
| 绕过最终写门禁 | completed OFF仍发送字节 | `8b55701c350e73b0a7fd7f16d60f8080103003fb0214558b4abb4cdf5206c9bb` |
| 不核观察PID | 错误PID被接受 | `d3e8e2f1cc18e6ee60c7295f29fbc2406c9e2a85f90818331f896bdd7a82cd78` |
| 不等单次EOF | 额外帧被接受 | `d31b2c8731a6a958cb6db41122a12e980917108d6f09b37a55d7987fac3c3c3b` |
| 跳过持久Pending | 无Pending被接受 | `a76a13b1d23f3e98492740fa84a8f9883dd859e62a73a48e08ddf027b6818ab9` |

初次ARM检查退出101：固定Rust缺ARM标准库；补齐原已安装manifest固定的1.98.0组件并验证SHA后，
再次退出101，缺aarch64交叉GCC。两次原失败日志保留。经授权只安装WSL交叉编译前置，
apt模拟确认0升级/0删除，实际使用no-upgrade/no-remove/no-install-recommends和仅列服务重启策略。
新增gcc-aarch64-linux-gnu、libc6-dev-arm64-cross及依赖；GCC13.3.0，未升级原Rust或项目依赖。
随后上述最终ARM check退出0。不得将这项有限检查写成正式ARM构建完成。

## 给B/主线的接入与剩余工作

从PR正文完整HEAD重新导出Git源码，核双亲和锁；候选显式feature才编译。
接口见[Core观察连接契约](../offline-a/statistics-core-observer-v1.md)，Go侧仍按PR266的固定SDK与四段补丁构建。
此次没有改B发行工具或工作流。回退本候选可关闭新feature，原签名feature和默认构建分别已验证；
不能把本机日志包里的源码或开发工具当作正式可安装制品。

尚需完成真实CheckInTransport：只取得/释放自己的Statistics引用，纯prepare，原Machine落盘后一次submit，
重启只查原摘要，贯通原单一预算；并验证其他Remote/Manual及既有统计查询引用继续运行。
当前public方法要求已有Ready Statistics引用，不能自动获取、启动或释放，不能宣称适配器已实现。
真实Core→安装Root→SDK的内核/cgroup/权限全链路仍待验证；当前测试不替代该证据。
链payload/区块/深度/重组的可信确认策略、长期授权及公网/设备验收保留负责人；不猜confirmed、不延长300秒，
不改reporting_available=false。后续仍有A0/A2/A3/A4/A5；整个A包未完成，热点专用延期，USB共享事务继续。

证据ZIP包含精确Git源码tar、本报告/契约、实际命令/退出码、正反例和原前置失败，不含二进制或生产秘密。
ZIP的SHA256和最终文档HEAD在PR正文登记，避免报告/压缩包循环自引用。
AI辅助：Codex；本人实际账号由GitHub提交回读核验，不重写作者。没有填写或估造工时。
云端CI按最终HEAD另读，本机通过不能写作云端通过。本轮整体验收保持81.5%、20/169、加权10.6%，不修改全局台账。
