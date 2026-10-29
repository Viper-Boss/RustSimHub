# 成员D接续：D-21恢复授权接口提案

2026-10-29，OpenAI Codex辅助编写、执行和核对。用户要求接续RedamancyGTW任务，
以PR交付并逐批报告已做/剩余。本批只交接口与现状失败样例，D-21未标完成。

## 提交与范围

- 仓库：`Viper-Boss/RustSimHub`，分支 `codex/gtw-recovery-auth-contract-20261029`，目标main。
- 起始main：`e48072ba9760c18f10c44847a1c6d94863eafd91`。
- 实现/夹具提交：`8c35d23823ec8e575cb9396d67b0250edca4eb3f`。
- 审查后的提案提交：`f6a8dc3b28893413aba7d5323418a3cbbf7f23bd`，补充容量满也必须能撤销。
- PR当前完整HEAD由PR正文列出；报告提交仅补交付证据，不改变受测生产库/夹具。
- 文件：`docs/ble/recovery-authorizer-proposal.md`、本报告、
  `tools/fixtures/recovery-authorizer/boundary.rs`、`tools/test-recovery-authorizer-boundary.py`。
- 无生产库、scope、schema、wire命令、依赖/锁文件、其他成员分支、全局进度/名册或设备变更。

## 实际交付

现有network.manage客户端不等于独立恢复身份，原Owner离线恢复也不是蓝牙恢复。
提案使用独立grant、同一业务库writer和Auth/Core权威，保持旧10位scope不变，
给出schema6→7、回退门禁、Owner/realm/代次/会话绑定、不可反序列化内部能力、
浏览器非导出独立密钥、SO_PEERCRED与密码证明分工、原Wi-Fi事务复用和撤销/保存排序要求。
具体密码协议仍等待成员A审批，不把#51可注入端口当作生产认证。

六个夹具全部调用**现有真实**Auth/Store/wire，使用本轮新建临时SQLite和明确合成密钥：

1. 独立密钥登录、离线Owner恢复及替换既有Owner均拒绝，持久principals完整保持。
2. 正常network.manage Client确实能管理网络，但不是近期Owner；Nas即使有scope也不能走网络管理。
3. 撤销使token/challenge失效，重开后保持；重新批准提高generation，旧回复/token仍不能复活。
4. 真实Backup API和新目录restore：pin相同、realm不同，旧principal停用；只原Owner离线恢复可用。
5. 冻结全部旧scope值，真实Store与合成库SQLite约束均拒绝bit10，重开保留唯一Owner。
6. 真实wire严格拒绝payload的channel/uid/peer_proof/recovery_context和伪新增恢复命令。

这是**既有行为已复核和新功能缺口证据**。没有新授权器/迁移实现，
没有生产缺陷修复声明，也没有用测试状态机替代生产模块。

## 真实验证

环境：Linux x86_64，普通UID1000，Rust/Cargo1.98.0，工作区锁定缓存，
命令带 `--offline --locked`；不连接D-Bus/设备/NAS/提供商。
运行器默认只输出计划，显式`--run`才在仓库`work/`下新建0700临时目录，
构建本次真实rlib、用clippy-driver以`-D warnings`编译独立公开夹具、核对精确6项清单并运行，
退出后删除临时数据库/程序。没有新Cargo manifest或下载依赖。

以下已执行命令均退出0：

```sh
source /workspace/toolchains/activate-rustsimhub.sh
python3 tools/test-recovery-authorizer-boundary.py
python3 tools/test-recovery-authorizer-boundary.py --run
cargo test -p rustsimhub-auth -p rustsimhub-storage -p rustsimhub-wire --locked --offline
cargo clippy -p rustsimhub-auth -p rustsimhub-storage -p rustsimhub-wire --all-targets --locked --offline -- -D warnings
cargo fmt --all -- --check
git diff --check
```

结果：新夹具6/6通过，0失败/忽略；相关回归Auth44、Store43、wire19，合计106通过，
0失败/忽略；文档测试0项。严格Clippy与fmt通过。首轮夹具和回归均通过，
运行器补充fresh checkout创建work目录后，精确夹具最终再跑6/6通过。
最后提案提交只修改文档，未改变受测生产库/夹具。未运行整仓/ARM/Native音频或真实浏览器/设备测试。

可复核源文件SHA256：

```text
boundary.rs: edb71bb4c89ddef0a4fc6ed605acd02a38f088deef7c5ab105cc98b86199eebb
runner.py:   02fecdb00182c831abd5e63cfb542cd97e0417e2f487869503a01cd48d6de95b
```

本机日志保存在work/recovery-authorizer，未提交生成日志。最终夹具、106项回归和Clippy
日志SHA256分别为：

```text
5cff38daa34fe4688395bfc19f0eef01b1e610f35eef89327687b2eaf842e708
90324f04dabfd48a3277992040a2074dd8ccc255dadeec9e84a49aa299d3e1e8
c1242911c4ec24a35deb1e448a800e4798467fcb2d8048fa7981818157c59d9f
```

这些日志摘要只便于比对本机记录，不冒充GitHubCI、独立审核或部署证明。
远程CI状态以PR精确HEAD回读为准。

## 本轮数量与继续条件

GitHub实际回读：以下四条既有PR均open、未合并，保留原作者/审核与依赖关系，未修改这些分支：

| 已有离线交付 | PR | 回读HEAD |
|---|---|---|
| D-01～04 | [#34](https://github.com/Viper-Boss/RustSimHub/pull/34) | 8edb20334fa05e97e1d7efe33e1f87990fa5e7b2 |
| D-05～12（含D-11完整持久去重提案） | [#37](https://github.com/Viper-Boss/RustSimHub/pull/37) | 69773e8c95197f988702a7dfbb811a88e66ac860 |
| D-13～16 | [#50](https://github.com/Viper-Boss/RustSimHub/pull/50) | 2b13b25f4ffad0fed04643fedadbdec60fed9f64 |
| D-17～20 | [#51](https://github.com/Viper-Boss/RustSimHub/pull/51) | 9f4ff04373846bd1555b3f471bc92072eaf5c412 |

因此按既有**离线包已提交**口径20/26、剩余D-21～26共6步。
该数不是20步已合并/全功能验收，不能据此给原项目新增分数。
本PR新增D-21前置提案一份、6个边界夹具；**新增完整完成步骤0**，仍20/26提交、余6。
原P0～P7整体约80%；main固定台账19/169与既有交付分支保留SMS-05的20/169分别看待，
本PR不改任何全局done/权重/分母，也不声称Pages更新。

任务书T6执行规则明确：
“新包第一份小PR只固定接口/来源/失败样例，主线审核后再实现其余代码”；
D-21要求“提独立恢复能力/批准撤销/存储迁移提案，获审后在现有Auth/Core唯一权威实现”。
来源：[当日分工文档](../team/ADDITIONAL-OFFLINE-PACKAGES-2026-10-27.md#t6-maximum-offline-delegation)。
所以先审本PR的明确接口/迁移决定点，再实现D-21；D-22/23需A安全会话与本接口，
D-24账户/许可另审，D-25远程媒体另包，D-26随真实新增模块验收。
没有已获批准的安全会话或数据契约时保留拒绝，不用Mock成功填补。

## 集成、来源与回退

本PR独立从最新main建立，评审差异只包含本提案/夹具/报告；无需先合入#51才能运行现状夹具。
实际生产接线仍依赖#51及经审核的A端口，不绕过四条旧PR的审核/主线门禁。
审核者应按PR当前完整HEAD确认接口决定及未实现项，再安排下一实现PR。
回退本批提交即可撤销提案/夹具，无存储或设备恢复动作。

来源：本仓库上述Auth/Store/wire/Core源码、冻结T5/T6任务书与#51边界；
运行器复用成员C既有`tools/test-business-recovery-matrix.py`的真实rlib验证思路，未改C文件。
新增文件沿用AGPL-3.0-only，无新增第三方源码/依赖、虚报工时或Git作者重写。
没有访问生产设备/数据库/通信数据，没有真实发短信/拨号/转发/外网业务、Root安装、签名或部署。
