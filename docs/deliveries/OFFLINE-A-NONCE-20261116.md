# OFFLINE-A · 统计窗口nonce竞争前置修复

日期2026-11-16；账号yh1030。起始main `b15ea3f561bf9e2a1268f8df5b6f0973cc83bbbd`（包含任务基线2ae840f）；
个人分支codex/offline-a-20261115-yh1030，同仓库PR到main，不自行合并。
实现完整HEAD `50dfb13b0508e5f409971c97ccd33050ee829a92`；实际产品测试tree `477deaa95efbc0e537676ad32523df74994246e8`。
后续只补初始盘点/报告，最终完整HEAD在PR正文单列。源码追加后必须重新绑定测试。

## 问题与最终行为

对应任务书A1第5项及STAT-03/STAT-06/STAT-07的一部分。
原CheckInMachine的三个窗口可分别prepare，即使同一身份已有准备中或提交不明的Pending；真实链适配可能重复使用同一尚未消费的nonce。
本包采用任务书允许的串行策略：当其他窗口仍有Pending时，返回原Due结果，不再acquire/prepare/submit或保存该窗口状态；
只有按原摘要确认或明确拒绝清理Pending后才能准备新交易。过期、关闭/重开或进程重启均不作为nonce已释放证据。
原窗口继续原准备票据的单次submit/原摘要query，已确认窗口仍可返回缓存成功，不改变三个逻辑窗口/主题/更新周期。
这是一道生产状态机门禁，不是实际SDK/Root或链上签名接线，也不能协调非法复制出的独立存储副本。

代码仅改crates/usage-stats/src/check_in.rs及tests/check_in.rs，无trait/持久格式/签名/预算/Cargo变化。
旧独立窗口测试改为逐笔确认后再处理下窗，保留窗口状态独立与跳过catch-up的断言；原测试允许并行准备的行为与新nonce约束不再兼容。
新增2个回归函数：prepared未提交阻止其他窗口但不丢原ticket；提交不明时三个窗口轮换、同进程/真实FileStore重开、跨两周期仍阻止新prepare，原摘要确认后允许下一窗。
外部Transport明确合成digest，真实CheckInMachine和FileStore负责状态；没有把意图hash称为真实交易hash。

## 实际验证

普通UID1000 Ubuntu-24.04 WSL2，Rust/Cargo1.98.0；固定Git archive，source/target/TMPDIR为原生Linux目录，测试locked/offline。
初次因工作区解析所需snow0.10.0不在旧缓存而未运行；已按原Cargo.lock补取依赖，没有改锁或升级版本，失败日志保留。

| 命令/检查 | 真实结果 |
|---|---|
| `cargo test -p rustsimhub-usage-stats --locked --offline -- --list` | 明确列出各target名称，仅清点不算通过 |
| `cargo test -p rustsimhub-usage-stats --locked --offline -- --nocapture` | 退出0；25 check-in、9 participation、22 persistence、13 protocol、21 query，共90个不同测试通过/0失败/0忽略；零项lib/doc组不累加 |
| `cargo clippy -p rustsimhub-usage-stats --all-targets --all-features --locked --offline -- -D warnings` | 退出0；该crate没有额外生产feature分支 |
| `cargo fmt --all -- --check` | 退出0 |
| 旧生产负对照`a715a023c243ed860107a7a0244db41563be46e4`，两个新回归选择器分别`--test check_in <name> -- --exact --nocapture` | 各实际运行1项，均行为断言失败/退出101，非编译错误；最终tests文件与正向tree逐字节相同，生产文件严格为原b15代码 |
| 初始差距表及来源核验 | 93个唯一动作，32个明确候选源码/测试/历史报告，Git字节/长度/摘要与固定base相符；逐动作语义复核仍待后续，不算93项通过 |

两个选择器：unresolved_transaction_serializes_other_windows_before_any_prepare、prepared_but_unsubmitted_digest_also_blocks_other_window_preparation。
成功与负对照仅差原生产文件，测试/预算/输入相同；所有断言原样保留。没有通过延长Root权限、跳时或重发来取绿。

## 证据与交付范围

`OFFLINE-A-NONCE-477deaa.zip`，27524字节/32条目，SHA256 `3c9a45ae6089df24eca88c6c1c2adba78a3b7557d183f0af8e080f86994f4852`。
ZIP CRC及各条目与原文件核对通过；含两份规范产品源码、源摘要、原生产负对照和90项正向命令/日志/环境/退出码。
路径前缀脱敏，合成seed不输出，无真实身份/消息/设备资料；包仅保存在本机供用户转交，未代发负责人。

新增docs/deliveries/OFFLINE-A-INVENTORY-20261115.md与docs/offline-a/action-inventory.json为全93项的初始候选差距表；
它们尚未证明每个动作已实现/复验，A0完整语义核对及A1～A5全部工作仍继续，不能因首个修复PR宣布A包完成。
任务范围以OFFLINE-A-PRODUCTION-20261115.md为准，旧C成员分工不能限制本次新A授权；A/B也不是历史作者账号。

## 未运行、风险、集成和下一步

未运行Core完整生产组合、固定Go1.27.3/SDK交易构造、真实签名向量、ARM、真实浏览器或设备/公网/链上。
本包只证明原状态机/FileStore对合成外部Transport的串行约束，不证明真实nonce分配或链上确认；未知Pending可能长期阻止新窗口，必须按原摘要解决，不能自动丢弃或换身份绕过。
给B：消费本实现HEAD，按上述命令回归和既有manifest/源码构建契约构建；无新依赖或载荷，不需改发布格式。CI接线归B，本包未改workflow。
给主线：按最终HEAD审查门禁位置、原Pending保持与拒绝/重开/恢复正向控制，正常合并；回退只撤本次门禁和测试变更，不改已有持久数据。
下一步继续固定SDK纯prepare/真实digest独立向量、有限Root协议提案、许可绑定与持久Pending到单次提交/确认接线，同时推进A2～A4独立业务复验；缺外部依赖仅记录该项，不停止其余任务。
蓝牙/AirSIM/native/B专属文件、冻结coordination、设备、全局看板/名单和Git作者未改；无生产签名、安装、电话/短信或真实交易。
来源本仓库AGPL-3.0-only既有实现与测试，SPDX保留；无第三方算法复制。Codex辅助实现/验证/报告，实际有效工时未估造。
整体验收仍81.5%，A包尚未全部完成，真实环境验收留负责人。
