# 成员D连续交付：五个PR的隔离联合回归

2026-10-30。用户已明确要求自主持续推进、不等逐批“继续”；本轮已串行提交#101/#103/#104/
#105，并把前置#97一起作实际联合合并回归。没有直接推送/合并main或部署。

## 可复核截面

- 主线基线：`6b5de5f3ca5ff706adacd8f25ed7f638546739a9`。
- 独立验证分支：`codex/gtw-d-joint-validation-20261030`，只用于复核，不代替各PR审核。
- 联合完整HEAD：`fa18e540bbfded35fc2e5072563032aa8d239451`。
- 本报告PR仅新增本人文档，不把联合代码混入主线或改变各PR状态。

| PR / 完整输入HEAD | 交付 / 来源 | 本轮数量口径 |
|---|---|---|
| #97 `bfbfdaae25daf581689d885332a98f2b6d3d6f4b` | 当前grant、锁外证明和私有能力前后gate；本轮开始前已提交 | 前置，不重复计 |
| #101 `51043fefcd323d6fd81f592da945f199249f5034` | token Drop无锁失效、拥有型Core桥；依赖#97，仍草稿 | 新5项组件测试，完整D-22仍缺外部前置 |
| #103 `f59cf9efc6ce1f3eab1c37416ab4fe9a179621ab` | D-23候选7→9/claim/Unknown兼容接口，4项真实SQLite样例 | 提案，不是迁移/执行实现 |
| #104 `2c84766800562a8b27b9fe1ebda3afd8a88cf8a1` | 旧#55媒体观察撤权修复独立接main | 原作者保留，6入口/16故障向量不重复计D步骤 |
| #105 `22663d12f95a191b44188d60965d069def0d5606` | 旧#51纯GATT库+旧#59清理责任修复独立接main | 原作者保留，原38+4测试不重算D-17～20 |

联合顺序为上表次序，使用正常git merge。前两步快进，后三步自动正常合并，没有冲突、
人工改动产品逻辑、删测试或忽略来获得通过。GATT单独接回时的workspace成员冲突已在#105
保留全部main成员含mdns，仅加ble；原Author/AuthorDate和cherry-pick -x来源均保留。

```sh
git switch -c codex/gtw-d-joint-validation-20261030 6b5de5f3ca5ff706adacd8f25ed7f638546739a9
git merge --no-edit bfbfdaae25daf581689d885332a98f2b6d3d6f4b
git merge --no-edit 51043fefcd323d6fd81f592da945f199249f5034
git merge --no-edit f59cf9efc6ce1f3eab1c37416ab4fe9a179621ab
git merge --no-edit 2c84766800562a8b27b9fe1ebda3afd8a88cf8a1
git merge --no-edit 22663d12f95a191b44188d60965d069def0d5606
source /workspace/toolchains/activate-rustsimhub.sh
cargo test --workspace --all-features --locked --offline
cargo clippy --workspace --all-targets --all-features --locked --offline -- -D warnings
cargo test -p rustsimhub-auth -p rustsimhub-storage --no-default-features --locked --offline
cargo check -p rustsimhub-core --lib --no-default-features --locked --offline
cargo fmt --all -- --check
git diff --check
```

## 实际结果

Linux x86_64、普通UID1000、Rust1.98、锁定离线缓存。全部命令退出0：
联合workspace **850通过、0失败、6项继承忽略**；默认Auth/Store共87通过、0失败/忽略，
默认Core库编译通过；整仓all-targets/all-features严格Clippy和fmt/diff通过。
六项ignore继承C06 worker、forwarder私有满盘、updater两项Root/capability、C14 worker/私有满盘；
本轮没有新增ignore。proc-macro-error2既有future-incompat提示保留，未冒称已修复。

原日志仅本地work/d-joint-validation，未上传生成物；摘要：

```text
workspace.log   d841ebdf2f80e0ed50ac954b283c16faa26f6088a1bf2514bcc6701ae35c9b68
clippy.log      d54c34d8c3805692fc21f254592f3181dcb7565a28a1c1f2c55bd27ca6bf607a
default.log     c49db7401139b5d9f1190b334258fbe50c7bc07fc0dd906909ddde750826c86e
default-lib.log dbca94f4a60eab329318dc404b0d1e7c718550c71bce1974c7979c4530a91b9a
```

各独立分支测试总数有不同截面：#101为798，#103为778，#104为780，#105为816。
联合850是main774 + #97新增19 + #101新增5 + #103新增4 + #104接回6 + GATT接回42，
不是把各分支总数相加，也不是850项新增或设备测试。
各报告保留了实际失败基线：Drop token旧能力未失效；媒体取消/计数耗尽仍有效；
GATT未确认安全关闭误报quiescent。修正后相同回归通过，没有放宽断言/延长期限。

## 五步剩余的真实接手条件

| 步骤 | 已有可评审成果 | 尚需的前置/未完成 |
|---|---|---|
| D-22 | 运输/有界工作、私有能力、无锁断连、Core桥、纯GATT库 | A标准安全会话/冻结转录与真实互操作；真实不同UID合法SourceGuard→桥；BlueZ运行端/服务接线 |
| D-23 | 当前Store边界/旧8拒绝、显式9候选、唯一claim/Unknown/指纹接口 | #103迁移/指纹/线性化点审核后实施；同一Wi-Fi actor中的Recovery所有权/保存/持久回执；发行回退9兼容 |
| D-24 | 旧#53候选服务材料，以及当前独立BLE socket/角色提案 | 实际NSS UID/GID/许可合同审核；不能占用旧候选982或加入通用IPC组；服务运行端、真实cgroup/BlueZ退出清理 |
| D-25 | 正常Core媒体许可及本轮取消/计数耗尽撤权修复，旧#56 TURN提案 | 受限TURN/ICE/DTLS/Opus适配与依赖准入、C的中继材料、私有TLS/强制中继与两端真实媒体验证 |
| D-26 | 实际Auth/Store/GATT/媒体组件失败矩阵和联合构建回归 | A/真实UID/GATT/Wi-Fi执行/媒体集成后跑跨模块迟到、撤销、退出与重启矩阵，设备/TURN/听音项保持未验收 |

任务书 `MEMBER-D-SMS-RECOVERY.md` 的T5/T6明确：
“新Auth/密码/Root/迁移/发布格式先交接口与最小失败样例，负责人审核后实现”。
`ADDITIONAL-OFFLINE-PACKAGES-2026-10-27.md` 又明确密码/数据迁移未获接口审批阻止生产接线。
因此不自行选A密码套件、批准UID或直接把候选schema9接入生产来扣完步骤；所有可复核成果
已具体落实在上述PR。当前可见main/PR没有可接的A标准会话实现，本轮未接管A的分工。

普通UID1000、无newuidmap；没有用假PeerContext/UID、shape合法的Peer/Epoch、channel字符串
或合成verifier来声称真实密码/内核运输成功。Root/NSS/cgroup、ARM/native、设备/NM/BlueZ、
真正Wi-Fi/远程通话/TURN/DTLS/听音均未运行。联合编译通过不代表这些端到端能力已存在。

## CI、进度与范围

#101/#103/#104/#105的精确HEAD Actions均已回读：test/native-audio因近期付款失败或spending
limit未启动，无步骤/日志，不是已执行的断言失败，也不能称远程CI通过。具体run/check及注解
证据在各PR正文；未反复重跑、绕过门禁或自合并。联合验证分支也不代表CI已通过。

本轮完整步骤新增0；仍 **21/26离线包已提交、剩5步**，固定P0–P7约80%、main19/169不变。
没有改全局台账、权重、成员/贡献数据或他人分支；旧#51/#55/#59由负责人决定收敛/关闭。
默认生产schema6，Recovery功能/后端/服务未启用；候选9只在测试副本，无迁移/部署。
没有生产秘密/设备/NAS、真实业务网络/电话/短信/流量、Root安装/账户修改、签名或发布。
AGPL-3.0-only/原署名保留；Codex辅助收敛/验证/报告，无工时虚报或作者改写。
