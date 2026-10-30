# D：Core、BlueZ、通知 FD 与角色材料的隔离联合验证

2026-10-30。本报告 PR 只修改本人交付文档，不合并实现到 main、不部署/启用生产服务、不改全局进度。报告分支 `codex/gtw-d-bluez-joint-report-20261030`，main 基线 `80db24e9b101dd3fb4b8c951aa7f1e0e1858e65d`；最终报告 HEAD 见 PR。

独立组合分支 `codex/gtw-d-bluez-joint-validation-20261030`，本轮实际受测完整 HEAD **`329927aab0883dbdd7b6750f3dc3354ee139cd48`**。从此前已测 `5680948a92af8343fab25a028dfb4322b0db4fa3` 正常 merge #125，再 merge #127；没有冲突或额外 runtime/测试修改。当前 crates/Cargo.toml/Cargo.lock 与 #125 `d504895db44c8ef2ec765cd880c3d1607f2a8bb0` 逐字一致，本次新增材料与显式手动政策实验。所有原作者/日期/来源保留，不修改他人分支。

## 组合输入与实际成果

| PR | 精确输入 HEAD | 本轮组合内容 |
|---|---|---|
| #101 | 51043fefcd323d6fd81f592da945f199249f5034 | Core owned channel、Auth 连接 Drop 失效 |
| #107 | 241803809029dc7da20a611baef6b476cc3c901f | 四项固定 BlueZ RPC、连续 unique owner |
| #109 | 2d4b0c6e4e130fd07ff1c523c144e6df3fe87475 | 广播对象、Release、路径墓碑 |
| #110 | fce922512fb52bb0eb5d12de951f30707390f1a8 | 空闲 owner 丢失 wait |
| #113 | 32a2c3dace7e0bd25a117670adba22c8a979b1e2 | 固定 GATT 树、有界不透明回调 |
| #116 | bdf2de4a08bad08d6d70d588bb3bbae8ce3714ef | Device1 历史租约、阶段发布和一次绑定 |
| #117 | a2732cc2a23693b16f209225b9463199192223cf | 通知路由提案及真实总线/FD 失败样例 |
| #122 | 663e138fcfce99a147d97ec2a59337d93f55f777 | 显式可选 Native AcquireNotify/SEQPACKET FD，无全局Value回退 |
| #125 | d504895db44c8ef2ec765cd880c3d1607f2a8bb0 | 真实不同 UID Core→SourceGuard→owned桥→Auth/Store手动正例/拒绝矩阵 |
| #126 | 2b69cf7594db318e181927c13e3e6dcb46491e33 | 无通用IPC组的待审角色材料和16项只读检查 |
| #127 | d94381502d08504209da40bcd0d56509536b627b | 未改候选策略的跨UID/libdbus/SCM_RIGHTS手动试验 |

#97/#103/#104/#105/#106 已在基线 main，后续 PR 的依赖包含前序，不把完整分支测试数相加。历史截面 `60f5057f67350fb853a4eb58cb7e0dbb56dfed70` 为904/0/6；`5680948a92af8343fab25a028dfb4322b0db4fa3` 为932/0/6；不是本次受测 HEAD。

## 本轮真实命令和结果

Linux x86_64 / 普通宿主 UID1000 / Rust1.98 / 锁定离线缓存。**整仓950通过、0失败、6项继承忽略**；严格整仓 all-targets/all-features Clippy、fmt/diff 通过。#122新增18项实际Rust FD测试，不能把950称为本批新增测试。默认Auth/Store87和默认Core库编译在此前568截面运行通过；本轮没有冒称重新运行它们。

- 普通材料检查：**16通过、0失败、0跳过**，包括实际临时 systemd root verify、私有 D-Bus broker 和现有 Root NSS 函数合成输入。
- Core 手动容器：**1项通过/0失败、13组场景断言**；真实 Core UID1001、合法来源1002，Root/Core/network/edge/forwarder/未知UID拒绝；合法来源无verifier拒绝，cfg(test)合成verifier后真实桥/授权/写回/占槽/Drop/Close/重连nonce通过。不是A实际密码正例。
- 策略手动容器：**1个试验、9组断言通过**；daemon1001/BLE1002/合成BlueZ1003/foreign1004。未改候选policy，单FD回复真实读包、双FD/输入FD/错误路径/方法/foreign UID/全局Value拒绝、固定元数据signal允许。正文为合成字符串/空元数据，只验证头权限和FD数量；不是Rust GATT跨UID/标准BlueZ互操作。

这三项分开统计，手动不计入950；Core target=test=false/required-features，普通suite没有启动Docker/切UID，不新增ignore。继承6个ignore和proc-macro-error2既有future-incompat提示保留。两类容器均固定公开镜像、断网/只读rootfs/只读输入/有限tmpfs与CPU内存PID；只有setup少量cap，实际角色0cap/no-new-privileges。策略容器使用只读合成NSS，不修改宿主或生产NSS；数字不是生产角色选择。无真实系统总线/主机BlueZ/HCI/RF。

```sh
source /workspace/toolchains/activate-rustsimhub.sh
export CARGO_TARGET_DIR=/workspace/RustSimHub/target
cargo test --workspace --all-features --locked --offline
cargo clippy --workspace --all-targets --all-features --locked --offline -- -D warnings
cargo fmt --all -- --check
python3 -B firmware/system/ble/test_inspect.py -v
python3 -B crates/core/tests/run_recovery_kernel_uids.py
python3 -B firmware/system/ble/run_policy_kernel_uids.py
git diff --check
```

运行于独立 worktree，受测 tracked_dirty=false，真实日志及输出metadata只留本地 work/bluez-joint3、work/recovery-kernel-uids、work/ble-policy-kernel-uids；逐二进制/fixture/宿主公共库/镜像信息见两手动结果，未上传二进制或私有材料。

```text
work/bluez-joint3/workspace.log bcf7b20db2adf84451a7ff380d9b38b16741192e56ddaa06a7891dc6d0a7a5fb
work/bluez-joint3/clippy.log 1a0fdd7a8f1060a0194dc0b34e7b24c56437a6e1e56cfec9439271cf3630649d
work/bluez-joint3/material.log 16a9834435909c03b04ee77d9de5aca7673672f7da227172426f5a77392dc96b
work/bluez-joint3/core-uids.log f032869cbc804473d46f7e13894ce5679989889dcfad251dc60ccdbfa8b28d94
work/bluez-joint3/policy-uids.log 980ef492611f4a38b25f50264c7d9257ce328ad21c843b1eed9f21e46387dafd
work/recovery-kernel-uids/result.json 1074b7f59553dbc40d55204348dac2fe3a03ae924cfb2330209d27af9cfcc4e6
work/ble-policy-kernel-uids/result.json a5247f5ead5e53ba331c5c0f4ce10dd470943fb3ad6ec70646e83dc9ded30713
```

受控失败证据在独立输入报告保留：#116 的snapshot-only变体漏过断连重连；#122 已发送后撤权返回错误仍可能已收到包，不能自动重发；#126旧检查器接受通用IPC组；#127放宽max_fds=2/PropertiesChanged规则后原断言分别失败。没有删断言、延长期限/队列或把 send Ok 说成业务收到。

## 剩余五步与审核边界

| 步骤 | 已有可复核成果 | 未完成 |
|---|---|---|
| D22 | 实际Rust通知FD/peer生命周期、真实不同UID Core桥正例 | A实际标准安全会话/冻结转录互操作、真实GATT→Core服务host/物理设备闭环 |
| D23 | main上的schema9/claim/Unknown提案及失败样例 | 审核后实现迁移/独立私有指纹/一次claim、同一Wi-Fi actor Recovery所有权/持久回执；发行/回退9兼容 |
| D24 | 新只读角色材料拒绝旧通用组，跨UID真实policy/FD验证 | 实际NSS UID/GID/Root许可批准、完整服务host/安装、组合系统策略/资源/命名空间/空口关闭清理 |
| D25 | 正常媒体撤权修复已合入；#56接口/依赖候选仍待审 | TURN/ICE/DTLS/Opus准入与实际适配、私有强制中继/双向媒体及设备听音 |
| D26 | 实际组件矩阵和本次组合构建/手动故障场景 | A/业务/服务集成后的综合迟到、撤销、断线、退出/重启和设备/TURN验收 |

任务书明确“新Auth/密码/Root/迁移/发布格式先交接口与最小失败样例，负责人审核后实现”；D23提案还要求claim线性化点/私有域审批，D25要求依赖准入后接入。不能以合成port/假Principal或默认成功抵消这些前置。当前可见 main/PR 仍没有可接的 A 标准会话实现；未接管 A 分工，也未批准候选 UID982/生产Root/迁移9。普通生产默认schema6/恢复服务未启，release panic=abort不宣称线程panic隔离。

本轮完整步骤新增0；仍 **21/26离线包已提交、剩5步，固定P0–P7约80%，main台账19/169**。提交/合并不替代完整验收，不按测试数/分支数改权重。无全局台账/名册/他人分支修改、生产数据库/秘密/NAS/设备/拨号/短信/蜂窝、签名/部署；AGPL-3.0-only/原作者保留、Codex辅助，无实耗虚报。

## 远端CI

实现PR最终HEAD的Actions均已回读并在各PR记录：runner未启动，steps=[]，账户付款失败或spending limit。最新#125 run41079493571、#126 run41081161508、#127 run41082555568；不是已执行断言失败，也不计CI通过。独立组合和本报告按各自最终HEAD另外回读；未反复重跑/绕过门禁或自行合并main。
