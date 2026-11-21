# A1完整签到适配器在研记录（2026-11-16）

最新补充：[十四场景故障矩阵](OFFLINE-A-A1-FAILURE-MATRIX-20261116.md)已实际通过，含真实写失败、磁盘满、迟到及部分启动清理；
生产代码未因本轮测试改变。原始快照保留，不能把旧“尚未跑”段落当成最新测试状态。

后续更新：真实Root/固定SDK五个隔离场景已通过，查询开关误报已修复，新产品字节ARM64库检查通过；
完整来源、失败保留和范围见[内核集成报告](OFFLINE-A-STATISTICS-KERNEL-20261116.md)。
下面为8bd原始在研快照，“尚未完成”的测试状态以新报告为准；完整故障矩阵/可信链确认仍未完成，PR268保持草稿。

本笔为**草稿交付，未完成完整验证**。已经实现原CheckInTransport候选及有限调用入口，
但真实Core→安装Root/cgroup→完整SDK→本机TLS/RPC成功链路、故障矩阵及新代码ARM检查尚未完成。
不将原组件测试当作这条全链路证据，不启用生产上报，不自行验收或合并。

## 当前固定来源

- 实际账号yh1030，本人分支`codex/offline-a-statistics-transport-20261116`，同仓库PR到main。
- 原产品基线`2ae840fbffff04691ccf8cedd49d554093dd4032`；本轮回读main仍为`6e1cd19bc9dd05fd150157c422590c1a801cc0b0`。
- 起始HEAD为PR267的`5a79e6119925f63323dab6ee7a2a72ec9064af62`；包含未合并PR265、266、267，保留正常历史和作者。
- 本轮实现/测试HEAD：`8bd77cc3cc7d28fc83f664f5a3ed7af7b1105611`。
- 最终当前受测tree：`e79bbd2a0b99d312feb1d2dee8bac8b7b97d06a8`。
  原生Git完整源码tar SHA256 `f42bd4d253643587bed11df76913b2c1c0907b2b63e54045a155b93a73f7ac01`。
- 补报告后仅文档变化，最终草稿HEAD在PR正文回读登记，避免循环引用报告自身提交。

## 已实际写入的候选代码

1. `nkn_root/check_in.rs`实现原CheckInTransport，不另造Pending状态库或身份。
   SharedStatisticsStore将原Machine和适配器指向同一个已锁定FileStore。
   构造无Root IPC/启动/预检/身份创建，当前真实Owner/独立许可由原SubscriptionSigner逐边界核验。
2. acquire只占本次真实Machine lease对应的签到使用权。已有Ready范围只加入引用，不替换原Root证明，
   Starting/不明或能力不足明确拒绝；没有运行范围时使用原authorized_ports和真实Supervisor启动。
   新启动中的原Root读写贯通本次原预算、许可及取消，25ms读轮询，最终一次本地非阻塞写持原Auth/许可锁。
3. 新范围的启动限额不超过原总预算与原10秒上限，至多一次引擎启动；没有永久Root授权或300秒扩展。
   部分失败释放精确本次统计引用，原Supervisor保留已持有范围供Core后续drive清理，不重新授权/启动。
   清理不要求已失效的Owner权限。当前release仅做本地精确引用释放，原Core监督worker负责实际资源退出核对。
4. 保留原Manual/Remote/Statistics三类Supervisor引用。Core分别记录汇总查询和签到使用权，
   两者共享底层Statistics票据但不共享关闭权：查询OFF保留签到；签到释放保留已启用查询及Remote/Manual。
   汇总查询仍必须有自己显式启用的使用权，不能因为签到引用存在就隐式开放。
5. 只有所有窗口无Pending时，acquire才调用原有限Preflight取得nonce；重启有Pending直接query-only。
   prepare没有任何Root/RPC调用，只消费预检nonce并调用原固定用途纯签名器。
   原Machine先持久化真实digest，再进入下一步submit；临时签名对象在I/O前take，错误/Unknown不恢复该对象。
   原观察连接在初始、轮询和最终写门禁同时核对这个原lease使用权及原Pending。
6. query只请求原意图和摘要；现有Root/SDK只给Unknown，没有可信链确认，适配器明确Unavailable，
   让原Machine保留Unknown和Pending，不伪造verified Pending或Confirmed，不清原摘要。
7. host-only `run_once(window)`使用原Machine和真实SystemClock完成一次显式有限调用：
   若第一次是Prepared，立刻由同一Machine完成下一步submit，再Drop释放自己的引用。
   后续显式调用从同一FileStore恢复，Pending时只查原交易；无定时器、自动重试、续权或后台引擎。
   长期授权与确认策略未获准，reporting_available仍false，无网页/API生产签到开关。

原CheckInTransport方法和Machine持久语义没有改。`AttemptBudget`新增只读Clone，复制的仍是
**相同截止Instant、原Cancellation Arc及原context**，没有新构造器、重置、续期或替代取消。
仅用于实际Root启动端口保留原调用的精确预算；有独立测试核对取消同时生效与deadline完全相等。
本地prepare与随后submit还受首次预检原预算约束，不能用下一次advance生成的更晚deadline延长提交窗口。

## 已运行证据

WSL2 Ubuntu24.04、UID1000、Rust1.98.0、Linux原生固定源码、原锁、TZ=UTC、TMPDIR=/tmp。

```sh
cargo test -p rustsimhub-core --lib --features usage-root-transport-candidate --locked --offline nkn_root::check_in::tests:: -- --nocapture
cargo test -p rustsimhub-core --lib --features usage-root-transport-candidate --locked --offline nkn_root::transactions::tests:: -- --nocapture
cargo test -p rustsimhub-core --lib --features usage-root-transport-candidate --locked --offline nkn_statistics::tests:: -- --nocapture
cargo test -p rustsimhub-usage-stats --features transaction-candidate --locked --offline -- --nocapture
cargo clippy -p rustsimhub-core --all-targets --features usage-root-transport-candidate,email-production-candidate --locked --offline -- -D warnings
cargo check -p rustsimhub-core --no-default-features --all-targets --locked --offline
cargo fmt --all -- --check
```

| 证据 | 范围与实际结果 |
|---|---|
| 最终tree e79新适配器测试 | 7通过、0失败/忽略；真实Machine预算/lease、真实Auth/许可；两项启动socketpair取消/OFF、两项原Supervisor引用记账、原预算Clone、OFF禁止run_once、Root不可用保留重启Pending |
| 最终e79严格检查/fmt | 全目标Core Clippy与fmt均退出0 |
| 前一tree 59e7417063e1cd1282bd29db3086190ec5aa6226 | 原观察连接9、签名5、统计92通过，默认Core all-target check、严格Clippy、fmt均0；随后仅本次新测试夹具修改，生产字节一致，不累计重叠测试 |
| 原始tree 8ce40b43c2b0e3358bde9a3a56ed6f9c74e0a716 | 同9/5/92及严格/默认检查0；run_once补入后未重新fmt，格式检查1，修正并在上述后续固定tree通过 |
| 新测试首次失败保留 | tree59新测试6通过/1失败：OFF夹具直接open原许可文件遇Store(Busy)，并发操作尚未完成；改用原门禁下300ms有界等待，不删锁或改产品断言，e79的7项通过 |

引用测试没有创建Ready证明，也没有启动假Root/SDK，只证明实际Controller/Supervisor引用记账。
socketpair直接提供测试FD，只证明Core有界读写和原授权，不证明已安装Root的内核peer/cgroup/SDK来源。
重启缺Root测试证明原Pending保留，**没有证明当前适配器成功提交或真正query的完整链路**。
原PR267 ARM通过不能用于本次新增代码；本次ARM、完整内核隔离验证、当前候选受控mutation尚待跑。

## 下一步与真实前置

- 补真实Core→Root/cgroup/nft→固定SDK→本机TLS/RPC成功提交与重启查询，核一次提交、原摘要和字节。
- 验证真实共享SDK进程在查询/签到/Remote/Manual分别关闭时继续或精确退出；现有引用单测不能替代进程验证。
- 补完整原预算到期、完成OFF/撤权、迟到回复、持久化失败和部分启动失败后的清理矩阵及受控错误对照。
- 新代码ARM及无新观察feature回归；从精确提交生成最终证据包再转非草稿。
- WSL只读盘点：没有docker/podman/nft，已有unshare/nsenter/ip，cgroup2可写；原内核夹具要求真实私有容器cgroup。
  apt只读模拟podman+nftables为0升级/17新增/0删除；**没有安装这些包或启动服务/修改宿主网络**。
  后续准备真实隔离环境，不能创建假容器标记、跳过cgroup门禁或把mock ready当通过。

原上游/AGPL许可证和PR265/266固定SDK来源保持，无依赖版本/Cargo.lock变化。
本轮仅Core nkn模块/usage-stats原预算的最小API和本包说明；不改B发行/工作流、coordination、BLE/AirSIM/声音或全局台账。
AI辅助Codex，未估造工时。没有访问设备/NAS/公网NKN、真实消息/链上交易、生产身份/密钥、签名发行或部署。
全部A包仍未完成；整体验收81.5%、20/169、加权10.6%保持。草稿CI另按实际HEAD回读，本机通过不冒称云端通过。
