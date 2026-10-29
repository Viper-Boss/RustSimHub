# D-22 局部交付：同 Auth 的 grant/连接/私有能力关卡

2026-10-30，用户继续授权推进。新独立分支从 main 建立，保留全部其他成员提交。

- 仓库/分支：Viper-Boss/RustSimHub / `codex/gtw-recovery-capability-gates-20261030`，PR 到 main。
- 起始 main：`b7a5dfa8d0d8c762c2284164e9d4371fce3c373f`。
- 实现 HEAD：`513dafaa2ea65990e9e85444a4c6dee773e57971`。
- 接续 main：`6b5de5f3ca5ff706adacd8f25ed7f638546739a9`，包括负责人已合并的 #95。
- 正常合并/整仓受测 HEAD：`144d6daad794dac4b9e11152a7b4812b3be3fc2b`。
- 当前完整40位 HEAD 见 PR 正文；后续报告提交只补本人文档。
- 接口依据：已合并 #84 固定 grant 与 #88 私有能力/锁边界提案。
- 语义来源：旧 #61 `bc9fb583debcf58d3bdebed9665ab1b631525328` 的 authority.rs，
  同仓库 AGPL-3.0-only；实际按当前字段/两阶段工作/并发占用重新适配，没有复制旧迁移。
- [实现、API边界与未完成项](../ble/recovery-capability-gates.md)。

## 实现范围

仅 `crates/auth` 独立 recovery_capability 模块、最小 cfg feature 状态接入、
Store 既有 recovery_grants 的只读 gate/对应测试及本人文档。不改 policy scope、wire/
Core 派发、A 密码库、默认启动、schema/迁移、Root 或其他成员文件/分支。
新 API 只在既有显式 recovery-grants feature 中存在，验证器默认 None。

prepare 取得当前数据库快照和单工作 Permit；同步 work.verify 不借用 Auth，供可信宿主
在独立有界线程执行；finish 返回短锁后二次复核，才生成私有、动作/正文绑定能力。
Store 读取使用同一 read transaction，历史 active 回执、错误设备/realm/Owner/gen拒绝。
工作尝试含失败/丢弃最多128次；待消费结果同样保留 busy，断连/替换配置不能重置。
所有证明处理有五秒逻辑截止，会话≤300秒不延长；没有以恢复能力创建普通 Client/Owner。

该库不启动线程/超时器，也不证明来源 UID；trusted_transport 方法不可从 payload 派发。
尚未配置 A verifier、绑定 SourceGuard/#95 宿主或 GATT；不能声称完整 D-22。
能力复核不是 durable claim 或长期授权保证，D-23 的 save/claim/Unknown 仍需单独接线。

## 实际验证

Linux x86_64、普通 UID1000、Rust1.98、锁定离线缓存；合成身份/消息/库。
新增16项 Auth、3项 Store，共19项真实生产模块测试；合成的只是外部 verifier，
没有复制状态机或在生产装入 Mock。目标 Auth65 / Store58，共123项全部通过。

实际命令：

```sh
source /workspace/toolchains/activate-rustsimhub.sh
cargo test -p rustsimhub-storage -p rustsimhub-auth --features rustsimhub-auth/recovery-grants --locked --offline
cargo clippy -p rustsimhub-storage -p rustsimhub-auth --all-targets --all-features --locked --offline -- -D warnings
cargo test --workspace --all-features --locked --offline
cargo clippy --workspace --all-targets --all-features --locked --offline -- -D warnings
cargo test -p rustsimhub-auth -p rustsimhub-storage --no-default-features --locked --offline
cargo fmt --all -- --check
git diff --check
```

正常合入 main 后在 `144d6daad794dac4b9e11152a7b4812b3be3fc2b` 完整整仓测试：
**793通过、0失败、6项继承忽略**；整仓 all-targets/all-features 严格 Clippy、fmt/diff退出0。
默认无 feature 的 Auth44 / Store43，共87通过、0失败/忽略；未开启此模块也不改变原行为。
六项 ignore 来自既有 C06 worker、forwarder 私有满盘、updater两项Root/capability、C14
worker/私有满盘；本批不新增 ignore。proc-macro-error2 既有 future-incompat 提示仍保留。
这些是本机实际回归，不代替远程 CI、密码验收或实机。

日志保留 work/recovery-capability 未提交生成物；摘要供本机精确比对：

```text
workspace.log        15c38e59e64fd3d3f41880ae3938f607555135f8d1dbb5de43daf66751b17f7e
workspace-clippy.log 5821c033e272e901caf154947ca3978601c92857fc8df8bc09e8986bfc58c94b
default.log          39c303fd0cd3a065d94c677223901b2bc82594f6456e02daaa2ab8c6760cb082
gates-final.log      69b783f88e3a9bc06678432a838aec7f8786830709e9047d9fff64f0684c74a3
clippy-final.log     6d99d417e9d02ca5c26e918e1c837bbb14c2d189706d0ec23134d92962878abf
```

受测 HEAD 后生产源码/测试未再变化，仅补本人报告。
测试具体覆盖见实现说明，包括真实线程持有验证时 Auth try_lock、管理读取/撤销和迟到拒绝。
期限用私有单调时钟字段构造过期夹具，不放宽产品期限或用长 sleep 模拟正确性。
Storage 故障矩阵只在真实单 writer 上修改合成元数据，验证每个单独变化都拒绝；
只读 gate 前后 SQLite total_changes 保持，真实 Backup/restore 后 active 原行也不能放行。

未运行真实密码互操作、跨 UID 合法准入、SourceGuard/Host/GATT 接线、release panic、
ARM/Native、设备/BlueZ/NM、Wi-Fi业务、Root/NSS/cgroup、真实网络媒体或部署。
没有新增依赖/锁文件、生产数据访问、电话/短信/流量测试、生产签名或安装。

## 数量与接手

新增完整完成步骤0；累计仍 **21/26离线包已提交，剩D-22～26共5步**。
D-22 现增加可独立验证的私有能力关卡，仍缺 A 安全会话和真实运输/GATT宿主。
P0–P7固定口径约80%、主线19/169不变；不修改全局台账、权重或他人验收。

后续在同一 Core 短锁 prepare → #95 线程持有 work/peer → 短锁 finish/check，
实际断开同步关闭 Auth token。A 关闭/迟到清理和原生互操作须结合该宿主继续验证；
D-23 在 save/claim 前使用同一 writer 复核，不直接引入旧 #63 的实验 schema8。

新增源码 AGPL-3.0-only，复用既有依赖/摘要函数/密钥校验，无新密码算法或第三方源码。
OpenAI Codex 辅助审计/实现/实测，无作者改写或工时虚报。生成日志只放 work 本地。
远程 CI 按最终 HEAD 单独回读，本地通过/合并不代替审核或设备验收。
