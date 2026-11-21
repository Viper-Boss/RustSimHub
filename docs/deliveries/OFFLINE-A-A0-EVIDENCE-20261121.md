# A0 逐项证据汇总（2026-11-21）

本次为 A 包任务书第 6 节的 **93 个原始动作**补齐证据索引。每个动作都有唯一的明细来源和待验收边界。项目 P0–P7 整体验收记录为 **81.5%**。

起始 main 为 `de663743bd726e77f7c560139d0f91ea78e8fe57`。分支为 `codex/offline-a0-evidence-20261121`，当前完整提交 SHA 随 PR 正文提供。[A 包任务书](https://github.com/Viper-Boss/RustSimHub/blob/de663743bd726e77f7c560139d0f91ea78e8fe57/docs/team/OFFLINE-A-PRODUCTION-20261115.md#6-全部业务动作映射)

## 93 项证据索引

| 动作组 | 唯一动作数 | 逐项明细与证据状态 |
|---|---:|---|
| FWD／NAS | 20 | 12 项转发、8 项 NAS 的原审计已在 main。A3 补充真实 Go→Edge→Core 调用和剩余旧界面／旧 PCM 边界，PR 已批准，待合并。[主线审计](https://github.com/Viper-Boss/RustSimHub/blob/de663743bd726e77f7c560139d0f91ea78e8fe57/docs/offline-a/forwarding-nas-audit-20261116.md) · [A3 补证](https://github.com/Viper-Boss/RustSimHub/blob/c2a60903edb30a4bf4f3efb4dffa244da58d1cef/docs/offline-a/a3-business-matrix-20261117.md) |
| AP／NET／LED／WEB | 28 | 8 项 AP、6 项网络、7 项灯控、7 项网页的原审计已在 main。A4 补充普通和隐藏 USB 配网、重启后只读回执等证据，PR 已批准，待合并。AP 专用工作按要求暂缓。现场仍待外部 USB 工具接线、实机重启和真实网络／灯控验收。[主线审计](https://github.com/Viper-Boss/RustSimHub/blob/de663743bd726e77f7c560139d0f91ea78e8fe57/docs/offline-a/network-web-audit-20261116.md) · [A4 重启回执](https://github.com/Viper-Boss/RustSimHub/blob/43652dbc257011c76f796746d87e896d5e11ffde/docs/deliveries/OFFLINE-A-A4-RESTART-RECEIPT-20261117.md) |
| SMS／CALL | 18 | 8 项短信、10 项电话的原审计已在 main。A3 补充双身份 IPC、5 个实际临时文件系统写满边界和模拟 MM 满存储证据。真实 SIM、运营商和声音验收仍待现场。[主线审计](https://github.com/Viper-Boss/RustSimHub/blob/de663743bd726e77f7c560139d0f91ea78e8fe57/docs/offline-a/sms-call-audit-20261116.md) · [A3 补证](https://github.com/Viper-Boss/RustSimHub/blob/c2a60903edb30a4bf4f3efb4dffa244da58d1cef/docs/offline-a/a3-business-matrix-20261117.md) |
| NKN／REMOTE | 18 | 11 项 NKN、7 项远程接入，由 A2 的固定版本证据表覆盖，PR 已批准，待合并。公网 NAT／TURN、弱设备长稳和真实终端仍待验收。[A2 逐项证据](https://github.com/Viper-Boss/RustSimHub/blob/fd3358ef1cee1bc2305ca96d13f55ff1ac3fc4ae/docs/offline-a/nkn-remote-a2-20261117.md#18-个原动作的对应证据) |
| STAT | 9 | 下表汇总已入主线子成果、A1 草稿证据和未完成的生产前置。 |
| **合计** | **93** | 证据索引覆盖 93 个动作，功能通过和现场验收分别按各原记录确认。 |

A5 在多个独立固定提交上执行复验。其 PR 已批准，待合并，报告保留了受测版本和选择范围。A4 重启回执后续提交更新了 A4 的证据，跨 PR 主线组合复验仍待执行。[A5 固定来源报告](https://github.com/Viper-Boss/RustSimHub/blob/94200bbf10a262b4e160526707a3cbb82ded2ee2/docs/deliveries/OFFLINE-A-A5-VALIDATION-20261117.md)

## STAT-01～09 逐项复核

| 动作 | 已有实现与证据 | 待完成或验收条件 |
|---|---|---|
| STAT-01 | 主线冻结 v1 公开合同、独立统计用途和三个固定主题。固定 SDK 的 15 组 Go／Rust 交易向量比对规范化字节、签名和链摘要，另有拒绝用例。[协议](https://github.com/Viper-Boss/RustSimHub/blob/de663743bd726e77f7c560139d0f91ea78e8fe57/docs/usage-stats/PROTOCOL.md) · [固定向量与测试](https://github.com/Viper-Boss/RustSimHub/blob/de663743bd726e77f7c560139d0f91ea78e8fe57/docs/deliveries/OFFLINE-A-LOCAL-SIGNER-20261116.md) | 负责人确认正式链证据规则。24h／30d／90d 是统计窗口标签，区块间隔不保证精确墙钟覆盖。 |
| STAT-02 | 主线已有独立统计种子、私有 FileStore 和固定用途签名候选。真实 Auth／Ledger／临时 SQLite 复验了 OFF、未授权、错 realm、Owner 换代和错误身份拒绝。独立同意的保存不生成统计身份。[签名与持久证据](https://github.com/Viper-Boss/RustSimHub/blob/de663743bd726e77f7c560139d0f91ea78e8fe57/docs/deliveries/OFFLINE-A-LOCAL-SIGNER-20261116.md) | 正式安装角色及权限、设备真实恢复、独立统计身份交付仍待验收。 |
| STAT-03 | 主线已合入跨窗口 nonce 串行、固定用途签名、有限 Root／SDK 和 Core 观察。A1 草稿离线证据覆盖先落盘原 Pending／摘要再单次提交、结果不明时查询原摘要。观察查询保持 Unknown。[主线整合](https://github.com/Viper-Boss/RustSimHub/blob/de663743bd726e77f7c560139d0f91ea78e8fe57/docs/deliveries/LEAD-STATISTICS-CANDIDATES-20261116.md) · [A1 草稿证据](https://github.com/Viper-Boss/RustSimHub/blob/4e8ccc781f713fbb6f0a4a8dfe1e073061cd13d0/docs/deliveries/OFFLINE-A-A1-OWNER-FENCES-20261117.md) | 可信链包含证据、确认深度、重组语义和真实链结果。节点接受、交易池或已有订阅均不足以确认这笔原交易。 |
| STAT-04 | 主线已有显式只读汇总。按窗口限频，区别旧缓存、失败和真实零值，换 realm 清缓存。受限观察仅在已有统计引用及固定安装策略下运行。[查询接线及来源](https://github.com/Viper-Boss/RustSimHub/blob/de663743bd726e77f7c560139d0f91ea78e8fe57/docs/nkn/usage-query-integration.md) | 缓存限当前进程，真实安装及公网汇总待验收。设备自己的原交易状态属于 STAT-03。 |
| STAT-05 | 主线已有独立参与许可和隐私披露。原 Owner／realm、当前披露版本、CAS、原请求回执与重启查询已在私有文件、真实 Core UDS 及本机网页复验。[独立参与授权](https://github.com/Viper-Boss/RustSimHub/blob/de663743bd726e77f7c560139d0f91ea78e8fe57/docs/deliveries/LEAD-NKN-USAGE-CONSENT-20261115.md) · [隐私说明](https://github.com/Viper-Boss/RustSimHub/blob/de663743bd726e77f7c560139d0f91ea78e8fe57/docs/usage-stats/PRIVACY.md) | 当前许可只记录参与决定。长期后台授权及续订撤销规则仍待负责人审定。 |
| STAT-06 | 主线有限观察调用校验许可、原预算和原引用。A1 草稿的共享场景验证释放上报引用后保留应继续运行的查询、Manual、Remote 引用，关闭／取消保留 Pending，迟到结果受原授权约束。[Core 观察](https://github.com/Viper-Boss/RustSimHub/blob/de663743bd726e77f7c560139d0f91ea78e8fe57/docs/deliveries/OFFLINE-A-CORE-OBSERVER-20261116.md) · [关闭及共享场景](https://github.com/Viper-Boss/RustSimHub/blob/4e8ccc781f713fbb6f0a4a8dfe1e073061cd13d0/docs/deliveries/OFFLINE-A-A1-OWNER-FENCES-20261117.md) | 实际完整 Root 到期、Owner 恢复与共享引用的组合、真实服务停止与重启仍需验证。 |
| STAT-07 | 固定向量含错主题／摘要等拒绝用例，库测试覆盖时钟和持久状态。A1 草稿的 18 个容器场景含提交前／在途 Owner 失效、恢复换代、原票据到期、落盘失败和原摘要查询。Core 本地时钟围栏另有专测。[签名测试](https://github.com/Viper-Boss/RustSimHub/blob/de663743bd726e77f7c560139d0f91ea78e8fe57/docs/deliveries/OFFLINE-A-LOCAL-SIGNER-20261116.md) · [18 场景记录](https://github.com/Viper-Boss/RustSimHub/blob/4e8ccc781f713fbb6f0a4a8dfe1e073061cd13d0/docs/offline-a/a1-owner-fences-evidence-20261117.json) | 本机模拟节点与完整 300 秒 Root 授权实际到期、真正跨进程重启分别记证据，后两项仍待补测。 |
| STAT-08 | 离线前置已覆盖固定受审 HTTPS seed、原限权网络路径、可公开数据范围及合成节点试验。在控制台开启只读统计和显式刷新属于两次独立操作。公网试验前应核对正式制品及策略、负责人本次授权、预期原摘要和退回路径。[安装前置](https://github.com/Viper-Boss/RustSimHub/blob/de663743bd726e77f7c560139d0f91ea78e8fe57/docs/nkn/usage-query-integration.md#安装前置与默认行为) · [固定试验边界](https://github.com/Viper-Boss/RustSimHub/blob/4e8ccc781f713fbb6f0a4a8dfe1e073061cd13d0/docs/deliveries/OFFLINE-A-A1-OWNER-FENCES-20261117.md) | 公网查询、真实提交、实际链确认及现场清理均待负责人安排。 |
| STAT-09 | 主线页面呈现近期活跃安装估计、只读刷新和独立许可保存。网页和原主页三宽验证覆盖已保存许可、关闭、原回执、退出后迟到结果和缺模块。当前 `reporting_available=false`。[网页与授权证据](https://github.com/Viper-Boss/RustSimHub/blob/de663743bd726e77f7c560139d0f91ea78e8fe57/docs/deliveries/LEAD-NKN-USAGE-CONSENT-20261115.md) | 实际上报可用性、现场关闭结果和用户支持材料须结合最终接线验证。生产上报仍关闭。 |

## 校验与交接

- 通过 `git show <固定SHA>:<路径>` 读取任务书和四份明细，以 Markdown 行首动作 ID 提取各组记录。原四份 20＋28＋18＋18 等于 84 项，缺失集合恰为 STAT-01～09。加本报告九行后与任务书集合完全相等，93 行、93 个唯一 ID，缺失、重复、超范围均为 0。
- 用 `git cat-file -e <固定SHA>:<路径>` 校验本报告仓库文件链接，再跑 `git diff --cached --check`。本轮新增文件为报告，产品测试结果均引用原受测提交。本轮未执行产品、设备及公网测试。
- A1 当前草稿为 `4e8ccc781f713fbb6f0a4a8dfe1e073061cd13d0`。负责人仍需明确链确认、重组和长期授权。在此之前，独立离线工作可核跨进程重启及实际授权到期。A2～A5 待合入后，用组合版本及最新 A4 提交补跑主线复验。
- 本报告由 Codex 辅助核对已有 AGPL-3.0-only 仓库证据。未引入第三方代码、接口、部署或工作流变更。负责人审核索引与固定来源即可集成，回退只涉及本报告。全局验收记录由主线维护。
