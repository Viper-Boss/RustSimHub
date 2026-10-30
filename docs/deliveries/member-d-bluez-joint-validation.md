# D：当前主线、Core 失效围栏与 BlueZ 局部组件联合验证

2026-10-30，用户要求持续自主推进。此 PR **只加本报告**，不自动合并实现 PR、
不启动系统总线/蓝牙、设备或服务，不改全局进度。

报告起始 main 完整 HEAD：`80db24e9b101dd3fb4b8c951aa7f1e0e1858e65d`。
报告分支 `codex/gtw-d-bluez-joint-report-20261030`，目标 main；报告最终完整 HEAD
见 PR 正文。实现组合在独立已推分支 `codex/gtw-d-bluez-joint-validation-20261030`，
实际受测完整 HEAD：**`5680948a92af8343fab25a028dfb4322b0db4fa3`**。

## 精确组合

| PR | 实现末端 HEAD | 本次状态/范围 |
|---|---|---|
| #101 | `51043fefcd323d6fd81f592da945f199249f5034` | Core owned channel/无锁失效 fence，前置#97已合并，待审 |
| #107 | `241803809029dc7da20a611baef6b476cc3c901f` | 四固定BlueZ RPC及连续owner，前置#105已合并，待审 |
| #109 | `2d4b0c6e4e130fd07ff1c523c144e6df3fe87475` | 固定广播对象/Release，依赖草稿 |
| #110 | `fce922512fb52bb0eb5d12de951f30707390f1a8` | 空闲owner丢失wait，依赖草稿 |
| #113 | `32a2c3dace7e0bd25a117670adba22c8a979b1e2` | 固定GATT树/单槽不透明回调，依赖草稿 |
| #116 | `bdf2de4a08bad08d6d70d588bb3bbae8ce3714ef` | Device1租约、分阶段一次绑定与完整截止，依赖草稿 |
| #117 | `a2732cc2a23693b16f209225b9463199192223cf` | 通知路由提案/六项真实FD样例，只有cfg(test)，依赖草稿 |

组合先以#113正常merge#101（旧受测截面`60f5057f67350fb853a4eb58cb7e0dbb56dfed70`，904/0/6），
再正常merge#117（含#116）。**无冲突、无额外执行代码/测试修改**。
#113已正常包含当前main；#97、#103、#104、#105、#106由主线此前合并。
所有原作者/提交来源保留，不重写作者。具体局限以各实现 PR/本人报告为准。

## 实际结果

**整仓932通过、0失败、6项继承忽略**，与前一组合904加#116的22项及#117的6项相符，
不是把各分支完整测试数相加。整仓 all-targets/all-features 严格Clippy/fmt/diff通过。
默认 Auth/Storage **87通过、0失败、0忽略**，默认Core库编译通过。
6项ignore继承C06/forwarder/Root/C14，没有新增；proc-macro-error2既有
future-incompat提示保留。Linux x86_64、UID1000、Rust1.98、锁定离线缓存。

```sh
source /workspace/toolchains/activate-rustsimhub.sh
cargo test --workspace --all-features --locked --offline
cargo clippy --workspace --all-targets --all-features --locked --offline -- -D warnings
cargo test -p rustsimhub-auth -p rustsimhub-storage --no-default-features --locked --offline
cargo check -p rustsimhub-core --lib --no-default-features --locked --offline
cargo fmt --all -- --check
git diff --check
```

当前追加验证在独立worktree运行，日志仅本地 work/bluez-joint2，SHA256：

```text
workspace.log 2668be4e59986a722995d6784b951c6da3a680bfa54aa843d42583b97f006427
clippy.log b25440461ca7ade24fe58b27f044efd97efac0a9f83b59ec07d212c0eecdf0d4
default-auth-storage.log 5bf97d455c8a8a07ba494eb5dd48eba388bf0121da660c63b037361415069a97
default-core.log c7e2613f0e329bedd387b028c350035be6c2bab39f2fa667c45b0b82e0273733
```

实现分支CI按精确HEAD回读，#107 run41008612237、#109 run41030467095、
#110 run41031093231、#113 run41032414407、#116 run41036876574、
#117 run41038841757均启动前失败：runner空、steps=[]，
annotation为账户付款失败或spending limit。不是代码断言失败；未重跑/绕过，
离线通过不冒充CI通过。组合分支与此报告的远程CI同样按各自HEAD单列回读。

## 数量和剩余门禁

本轮四个实现交付包（#109/#110/#113/#116）、通知路由提案/样例#117及联合证据；完整步骤新增0，
仍 **21/26离线包已提交、剩5步，固定P0–P7约80%**。
“已提交”不等于已按完整任务验收，合并也不替代设备验收。

| 剩余步骤 | 本次推进 | 尚未闭环 |
|---|---|---|
| D22 | Core围栏/Device1租约/GATT回调能同组合测试，FD提案有真实总线样例 | A审核安全会话、真实UID桥接/物理事件驱动/按peer通知Native实现、实际Core-GATT端到端 |
| D23 | 已合并的事务/schema9提案与失败样例保持 | 依审核契约实现真实单次Wi-Fi事务及持久Unknown，不以管理回执代替业务幂等 |
| D24 | 固定BlueZ控制/导出组件与角色隔离接口 | 实际NSS UID/GID/Root策略批准、服务材料集成及设备资源/权限验证，候选982尚未预留/批准 |
| D25 | 主线媒体失效边界保持 | TURN/ICE/DTLS/Opus和真实relay/媒体联通验证 |
| D26 | 多组件组合测试通过 | 带实际会话/业务执行/权限的综合故障注入和设备验收 |

真实私有D-Bus只证明broker/对象/发送者/有界队列/期限层；外部BlueZ及可信宿主回复
是合成端点。Core正向真实不同UID桥接仍未运行，不伪造PeerContext或内核凭据。
不把GATT peer路径形状、epoch或Accepted帧回复当成认证。
缺安全端口不得启完整恢复服务；生产通知未实现，FD仅cfg(test)候选。
下一批继续独立显式feature下的按peer Native FD实现，不冒充实际RF/业务接线。

[D任务书](../team/MEMBER-D-SMS-RECOVERY.md)要求“新Auth/密码/Root/迁移/发布格式先交接口与最小失败样例，负责人审核后实现”。
本次不增加这些格式或安装路径，不代替A，不部署/签名，不读取生产设备/数据库/NAS，
没有短信/拨号/蜂窝流量、系统总线/主机BlueZ/HCI/RF、真实角色Root/NSS/ARM/native验收。
Codex辅助实现/实测，来源与许可见各PR，不虚报工时、不改他人分支/全局台账。
