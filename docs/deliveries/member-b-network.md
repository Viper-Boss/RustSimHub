# 成员 B 第一批交付：网络计量失败边界

成员稳定 ID：`member-b`。旧账号 `wnfhsj`，本次执行账号 `wnfhsj88`（用户确认且 GitHub `user` 回读一致）。未修改名册、授权记录或全局进度；账号映射由负责人复核更新。

任务来源：`docs/team/MEMBER-B-NETWORK-ACCOUNTING.md` 第一批 B-01～B-04（NET-01/02 离线子范围）。遵守一次一个包、PR 审核后串行接下一包。

- 起始 main：`7257cfa8d82bac8972158ef79ea081d9be12af75`
- 实现与测试提交（完整 HEAD）：`48c6bb7d95cd0c3451da12fe2561b3e842971ace`
- 分支：`codex/wnfhsj88-network-accounting`
- 仓库：`Viper-Boss/RustSimHub`，PR 当前最终 HEAD 见 PR 正文。
- 状态：离线实现/自测完成，等待负责人逐 HEAD 审核；不宣称所有 B-01～32 或 NET 实机验收完成。

## B-01：已有行为和数据流

原实现已提供 Policy、按 SIM 的日账本、月结、nft 计数与持久化。本 PR 没有新增计费后端。

`nft JSON → counters/parse_counters → State::sample → State::usage → rules/status → publish(meter.json)`。

- Policy：reset_day 1～31，limit_bytes 1～100 TiB。禁用限额并不清账本，也不等同启用蜂窝数据。
- 时间：Unix 秒转 Asia/Shanghai（UTC+8），有效年份 2024～2200；每月结算日超过月长时夹到该月最后一天，窗口为 `[start,end)`。
- 单位：nft IP 层 RX/TX 累计字节，usage 为两者之和，非运营商账单或速率。
- 同计数源用增量；源/boot 变化后从新累计值计入；SIM 切换时最后增量归旧 SIM，新 SIM 后续单独计量。
- 持久化：私有文件有限读取、owner/mode/单硬链接/常规文件检查，拒绝 symlink；create_new pending、fsync、rename、目录 fsync。异常 pending 保留，未发现“读坏账本后自动新建”的正常运行路径。
- 原测试 12 项（库 6、语音域 6）；新增 16 项（计量 10、Linux 私有读写/计数解析 6）。

## 已复现并修复

### 方向溢出导致部分计费

输入：同日旧账本 RX=10、TX=u64::MAX，新计数源 RX=1、TX=1。

修复前：RX 先变成 11，TX 加法失败，检查点未推进；重试还会重复增加 RX。

修复后：先验证两个方向的加法，任一溢出都保留原日记录和检查点，只设置 uncertain。测试覆盖 RX/TX 两方向溢出、连续失败和序列化重开。

### 满目录失败导致不可重开的账本

输入：保留窗口内已有 3200 条不同 SIM 的日记录，再加入新 SIM 的非零样本。

修复前：先插入第 3201 条再报错；State::valid=false，若按运行循环保存，重启将拒绝加载。

修复后：插入/修剪前核对保留条目容量；失败只设置 uncertain，保留有效可重开的旧账本。过期记录可按既有 400 天保留规则正常释放容量，不清空当前账本。

不改变 schema、对外接口、SIM 归属、账期规则、语音/IMS 策略、网络命令或数据默认关闭行为。Linux 仅将既有计数 JSON 解析抽为私有函数供测试，命令调用不变。

## B-02～B-04 覆盖与验证

所有输入均为合成数据，只使用真实 State/Policy/period/usage/parse_counters/checked/publish 和临时目录，未调用真实 nft、NetworkManager、ModemManager、NAS 或设备。

| 检查 | 命令 / 结果 |
|---|---|
| 修复前反例 | 起始 main 的原 lib.rs 加入本次测试模块，`cargo test -p rustsimhub-network-agent --lib accounting_tests --locked`：退出 101，8 通过/2 失败；之后恢复实现并复跑 |
| 包回归 | `cargo test -p rustsimhub-network-agent --locked`：退出 0，28 通过/0 失败/0 ignored；doc-test 0 项单列 |
| 严格检查 | `cargo clippy -p rustsimhub-network-agent --all-targets --locked -- -D warnings`：退出 0 |
| 格式 | `cargo fmt --all -- --check`：退出 0 |
| 网页既有回归 | `node tools/test-web-network.mjs`：退出 0，单位/未知无线字段/退出迟到响应/不重复提交/隐藏页不轮询通过；未改网页 |
| diff | `git diff --check`：退出 0 |
| Python 条件 | 任务书建议的 `python3 -m unittest discover -s tools/system -p 'test_network*.py'` 在此 HEAD 匹配 0 项，退出 5，不计通过；真实私有 IO 验证由 Linux Rust 测试完成 |

测试环境：Debian 13 x86_64、Rust 1.98.0、Node 24.19.0。精简失败证据：原 RX 断言实际 `(11,18446744073709551615)`，期望 `(10,18446744073709551615)`；满目录反例原 `s.valid()` 断言失败。

矩阵覆盖：reset_day 1/28/29/30/31、小月/闰年/跨年与边界前后 1 秒；无效/未同步时间、跨日回拨、同日回拨与远期样本现有行为；单调计数、重复样本、RX/TX 分开、恰好限额/单字节超额、两方向及聚合溢出、换源/换 SIM/空 SIM、反序计数；缺失/畸形/外来计数 JSON；真实发布/加载/替换、截断/错误版本/超大文件、错误权限/owner、软硬链接/非普通文件、pending 冲突和 rename 失败。

## 保留边界与主线补验

- 同日回拨只保存 last_day，无法判断同日时间倒退；合法年份内的远期跳变仍采用原保留窗口，可能丢弃旧日期行。测试明确锁定现有行为，不宣称这两种时钟异常已防护。若要新增持久时间/时钟跳变规则，请先审接口与迁移。
- State::sample 输入没有“来源代次”字段，不能区分一个新 boot 与旧源迟到的有效样本，也不能只凭相同 boot 中的较大计数证明接口未重建；本 PR 不伪造该能力。实际接口消失/重建、源读取失败后的 Root 守卫、SIM 身份来源仍需主线集成/设备复核。
- 文件测试覆盖故障返回及证据保留，不冒充掉电、真实 ENOSPC、目录 fsync 失败或文件系统崩溃恢复验收。
- 未运行 ARM64 交叉构建、整仓回归、真实数据卡/计费误差/限额断连/切网/换卡/掉电，也未连接设备或开启普通 APN。这些不能由本 PR 计为完成。
- 负责人补验：核对真实 nft 源/双向共享限额、源代次和重启重建；卡 A/B 分账与实际运营商误差；故障重启后的账本/uncertain/普通数据门禁；网页剩余额度及时间异常提示。

## 来源、集成与回退

新增代码/测试由本次 Codex（OpenAI）辅助编写，复用仓库生产实现和既有依赖；未复制第三方代码，沿用 AGPL-3.0-only SPDX。作者使用经确认的新 GitHub 账号的 noreply 地址，不改写旧提交作者。有效工时未估造。

主线先核对两个反例、允许路径和该 HEAD 的测试，再决定合并。无需依赖/迁移/部署配置变化；回退实现提交即可恢复旧行为（旧缺陷也会恢复）。B-05～08 与其他追加包留待本批审核后串行交付；产品百分比和看板状态由负责人维护。
