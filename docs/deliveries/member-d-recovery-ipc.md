# D-22前置交付：可信IPC接口与既有边界样例

2026-10-30。按用户“继续”接续；#84已实际合并，独立恢复grant以主线为准。

- 仓库/分支：Viper-Boss/RustSimHub / `codex/gtw-recovery-ipc-contract-20261030`，目标main。
- 起始main：`5238670246e7c6e43184534a7a6ee4b13c293ffd`。
- 第一批接口/5样例HEAD：`8eefeca2ddc5e3213e8deebee14d858c6372a365`。
- 最终代码/第6样例HEAD：`de5f4faa021d67a8b8b610623d4b4400fd04c435`。
- PR当前完整40位HEAD见PR正文，报告提交不改变受测源码。
- [具体提案与旧PR收敛表](../ble/recovery-ipc-proposal.md)。

## 实际交付与范围

补齐之前漏检的#60/#61/#63：三条仍open，与已合入#84的授权表/代次/回执结构存在重叠。
它们的实验schema7/8不能直接并入主线#84的同号schema7。提案保留旧证据，不改旧分支，
不关闭、合并或重写他人PR；建议以主线为唯一格式，复用#61私有能力及失败向量设计，
#63业务执行账本留D-23单独移植，#53 UID982候选不作为实际许可。

D-22具体候选：独立固定Unix端点、真实SO_PEERCRED、与其他角色分离的最小许可，
2050字节IPC运输正文（应用信封仍2048）、Core连接nonce和A证明上下文、私有能力、
验证器锁/超时后迟到清理责任。密码套件与A协议没有自行决定；不存在可接的已审A实现，
冻结文件仍crypto_wire_frozen=false。不安装Mock、不把UID/pid/bonding当业务授权。

生产代码只多一个`cfg(all(test,target_os="linux"))`模块声明；无生产服务器、socket路径、
协议/权限、Auth/Store/wire/模式或角色修改。6个样例调用现有真实Core、SocketRuntime、
Tokio UDS、SO_PEERCRED和Python子进程；没有测试替代状态机。

1. 真实子进程发送伪uid/pid/channel/认证标志，内核仍返回实际凭据且普通解析拒绝payload。
2. 两个真实连接可共享UID/GID/PID，不能据其生成连接身份。
3. 真实Core监听器拒绝与实际peer不同的配置UID，停止后socket清理、无principal写入。
4. 匹配UID可读公开Identity，但无Owner证明的恢复管理仍拒绝，真实Store无新身份。
5. 通用Auth接受2049字节合法JSON空白，不把其64KiB预算当BLE预算；这是需求证据，不是漏洞声明。
6. 用#61原始旧schema7 SQL建立真实临时库，当前7版入口明确Corrupt拒绝且源字节/版本/表保持。

旧SQL夹具逐字来自`bc9fb583debcf58d3bdebed9665ab1b631525328:crates/storage/src/recovery.sql`，
Git blob `3a13259fc2644076ca899334242275d8894c8ba1`，SHA256
`63d4c9f19f168185ed474817ec43431d07e2c77d15d6d77eaf1c033a3d9be0e1`。
只由cfg(test)引用，沿用本项目AGPL-3.0-only，不是第二份生产迁移。

## 真实验证

Linux x86_64、UID1000、Rust/Cargo1.98.0，全部新状态/密钥位于临时私有目录，
子进程只通过指定临时Unix socket发送合成数据。不连接真实D-Bus/BlueZ/设备或生产库。

通过命令均退出0：

```sh
source /workspace/toolchains/activate-rustsimhub.sh
cargo test -p rustsimhub-core -p rustsimhub-auth -p rustsimhub-storage -p rustsimhub-wire --all-features --locked --offline
cargo clippy -p rustsimhub-core -p rustsimhub-auth -p rustsimhub-storage -p rustsimhub-wire --all-targets --all-features --locked --offline -- -D warnings
cargo test -p rustsimhub-core recovery_ipc_contract_tests --no-default-features --locked --offline
cargo test -p rustsimhub-core --all-features --locked --offline
cargo clippy -p rustsimhub-core --all-targets --all-features --locked --offline -- -D warnings
cargo fmt --all -- --check
git diff --check
```

先完成5个新样例及四包297项回归（Core173/Auth49/Store55/wire20），0失败/忽略。
再补旧schema7碰撞样例后，Core完整174项与严格Clippy复验通过；其他三包未变、已通过124项。
本批是6个新增测试，不是新增298项功能验收。默认配置5项边界样例通过，旧schema碰撞
仅在显式recovery-grants feature下测试；未以ignore隐藏失败。首轮及最终检查均通过。
未运行整个workspace、ARM/Native、不同UID或Root/NSS/cgroup/组继承、A密码互操作或设备测试。
配置不匹配UID的拒绝试验不能冒称“真的以另一UID进程运行”。

本机日志保留在work/recovery-ipc，不提交生成日志。摘要只便于复核，不是远程CI或独立审查证明：

```text
regression.log       0a5ef710886edff968ef41165132222f5f49361e3674a9f44d60c2c73134710d
default-tests.log    34fb8c5c5f0510badbb035c7bd646a473ed149013bfa8582de19efec82197db0
clippy.log           57478897c1cf23595dc341b493d4487ea6c97d8733cd4bad45ae341612e3e4f0
core-final.log       d25ddea68641e4a282ec3f566543c522caf6ee40d57d641524a141a5dd6f912c
core-clippy-final.log 41eae9b1f7096aebc2d57dab38b484928d5f5ae0a811d4ca2e4996b76c2bedc9
```

## 进度与接续

仍按离线包已提交计**21/26、剩D-22～26共5步**。本轮交D-22前置接口和6个真实需求/拒绝样例，
**新增完整完成步骤0**，不把尚无A实现的D-22标完成；旧#60/61/63局部成果也不重复扣步。
原P0～P7约80%、main台账19/169不变，无全局看板/名册/分母/其他成员文件修改。

任务书[T6执行规则](../team/ADDITIONAL-OFFLINE-PACKAGES-2026-10-27.md#t6-maximum-offline-delegation)
要求“新包第一份小PR只固定接口/来源/失败样例，主线审核后再实现其余代码”；D-22还明确
要求“接A安全会话和本人Gatt适配库”。请按PR精确HEAD审本接口/收敛方案，再接实现；
缺A端口时继续unavailable，不能用合成证明冒充生产会话。D-24账户许可与生产安装另审。

回退本批提交只撤销测试/文档，无数据或设备恢复动作。OpenAI Codex辅助审计、编写与实测；
未新增第三方依赖/锁文件、虚报工时或改写作者，没有生产数据/设备/NAS/公网业务访问、
真实短信/拨号、Root账户修改、签名或部署。GitHubCI以实际HEAD回读为准。
