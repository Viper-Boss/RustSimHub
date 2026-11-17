# A1：Owner换代故障矩阵与最新ARM复验

2026-11-17，继续本人草稿PR268。新增离线证据已交，完整A1仍未收口，草稿状态保持；整体验收仍 **81.5%**。

- 起始main：`de663743bd726e77f7c560139d0f91ea78e8fe57`，回读未变化。
- 本轮父提交：`924e4b7441cf61398423a4e3281608bfa3776215`，暂无新审核回复。
- `implementation_head`：`d3bb4d636ebb3c29e8d66b5f4e70faeca936afb7`。
- 受测tree：`ad4dd34206d5d0d7c45097d9f19f28992730b5d0`；源码tar SHA256：`9d35f216dd1be80743f3c488fa4a9ccf19272985c62b2bb4a83b352995e0faa8`。
- 报告为后续独立提交，当前完整HEAD写PR正文，不循环引用自身。

## 实际变化

仅改测试：`crates/core/src/nkn_root/check_in/tests.rs`、`crates/core/src/nkn_statistics/tests.rs`、`tools/offline-a/statistics_kernel_fixture.py`。生产Rust/Go、wire、持久格式、授权上限、启动入口及工作流均未改。自有测试沿用AGPL-3.0-only，原SDK来源/许可证保持，Codex辅助实现与核验，不估造工时。

新增4个Core/Auth/Machine/FileStore→原Root→固定SDK→本机TLS模拟节点的场景：准备后提交前Owner inactive、准备后恢复新generation、节点收到一次提交后Owner inactive、提交在途恢复新generation。前两者零提交，后两者仅原提交一次；旧授权不复活，原摘要/Pending保留Unknown。

身份变化只在私有Fixture数据库将Owner置inactive并增代次；恢复调用原 `Store::recover_owner` 的精确generation门禁。**不是新增生产Owner撤销API**，也未放宽原禁止在线撤销Owner的规则。

新增两个普通Rust测试函数：Owner inactive/恢复换代中断原Root静默startup读取；Core本地Instant/BOOTTIME已过期时不得写Root请求。每函数两个变体，原Auth authority deadline不变。这不是安装Root完整300秒实际到期验证。

原14个容器场景在新源码上全部重跑，加新4个合计18个均通过。没有删除Pending、原SDK PID/cgroup回收、Root manager或共享引用断言。

## 固定来源与实测

Ubuntu-24.04 WSL2、Rust1.98.0、locked/offline。Core专项ELF重新构建并落盘核SHA256。SDK/Root复用已核验的 `18cbd7c405d844d24de15eb115880217cf8f5fad` 固定构建，当前生产Go/prepare字节不变，SDK SHA256仍为 `643e580a321bd29a6021b3a804f2db076e58e30c4867eadbb53ddec82edd77b0`；不冒称本轮重新跑过固定Go链。

使用原专用Podman状态、固定本地image、`--pull=never --network=none --cgroupns=private`、只读输入和独立输出。夹具核mnt/net/pid/cgroup与调用者不同、容器cgroup根为`0::/`才设置临时角色/策略；没有在宿主机运行Root布局或绑定宿主cgroup。start_failure只证明Root启动前拒绝，bootstrap_failure另核实际已启动SDK的精确回收。

| 命令 | 实际结果 |
|---|---|
| `cargo test -p rustsimhub-core --lib --features usage-root-transport-candidate --locked --offline --no-run --message-format=json` | 0，复制专项ELF核哈希 |
| 容器 `/core-test --ignored --exact nkn_root::check_in::tests::statistics_transport_kernel_sdk --nocapture`，18次scenario | 全部0，退出后专用Podman状态无运行容器；同一函数18次运行，不是18个新增Rust函数 |
| Core lib、`--features usage-root-transport-candidate,email-production-candidate --locked --offline nkn_root:: -- --nocapture` | 25通过/0失败/2忽略，0 |
| 同组合 `nkn_statistics::tests::` | 5通过，0 |
| 默认Core bin `signed_session_continuation_round_trip_rotates_with_existing_overlap` | 1通过，0 |
| 生产Core bin `nkn_`筛选 | 8通过/3忽略，0 |
| Core同生产feature `cargo clippy --all-targets --locked --offline -- -D warnings` | 0 |
| Core `cargo check --all-targets --no-default-features --locked --offline` | 0 |
| `cargo fmt --all -- --check` | 0 |
| Core `cargo check --lib --bins --features usage-root-transport-candidate,email-production-candidate --target aarch64-unknown-linux-gnu --locked --offline` | 0；最新草稿ARM编译检查，不是完整链接/QEMU/设备执行 |

18场景为owner_inactive_before/owner_restore_before/owner_inactive_after/owner_restore_after，加原start_failure/bootstrap_failure/off_before/cancel_before/expired_prepare/late_reply/root_revoke/save_failure/save_full/startup/shared/lost_ack/off/cancel。旧off/cancel是提交在途，零提交证据属于before场景，原勘误保持。

逐场景摘要、计数与日志哈希见[公开脱敏证据](../offline-a/a1-owner-fences-evidence-20261117.json)。完整合成原始日志、命令和本机环境绑定复验脚本留本地测试包，不公开运行身份、密钥、原始网络/进程记录或设备数据。

## 交接与剩余

负责人核implementation_head与固定来源后审核。B可消费SHA、feature和命令自行构建，不执行不明二进制；Root kernel专项必须具备原隔离角色/命名空间夹具，不能把本机绑定脚本视作跨机器一键包。未改B的CI/打包器/发行格式；回退仅revert本测试与报告提交，无生产迁移或设备动作。

仍待负责人决策：可信链包含证明来源、确认深度/重组语义、长期参与权限及更新/撤销策略。query保持Unknown，生产上报OFF，原Owner/Root最长300秒不变；accepted/交易池/已有订阅不能冒充本次交易确认。

未穷尽完整300秒Root实际到期、Owner恢复与共享Manual/Remote引用、跨进程重启和所有跨阶段组合；本地时钟围栏不替代它们。没有新跑全仓/热点/蓝牙/声音/Go race/vet/真浏览器、公网链交易或设备。旧失败原样保留，本轮所选检查均0；云端CI按最终HEAD另读，不能用WSL成功代替。

下一步按负责人策略收口可信链查询和长期开启；其他原范围故障组合可继续独立补证。整个A1/A包不标100%，81.5%及原分母/权重保持，不自审、自合并、部署或签名。
