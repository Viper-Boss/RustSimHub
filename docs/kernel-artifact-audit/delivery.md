# C19/C20 · 固定合成样本与离线交接

本页交接同一UPD-07包的来源清单、C18检查器、C19固定样本和C20补验边界。
PR #143 由成员yh1030提交，实际被审HEAD和本机完整源码标识以PR正文及[本人报告](../deliveries/member-c-nas.md)为准。
本页的“通过”只表示合成样本得到预期结果，不能解释为真实内核/固件、设备或发行支持通过。

## 交付文件

| 入口 | 用途 |
|---|---|
| [C17来源清单](README.md)、source-index/source-checks | 区分原锁事实与实际字节证据；来源截面保持原样 |
| [C18工具及输入说明](usage.md) | 默认计划、显式隔离制品、固定源锁/合成两模式、0/1/3/4退出与资源边界 |
| [固定样本](corpus.json) | 基线Image/DTB/模块/固件的合成字节hex及22个有稳定ID的差异；不是发布manifest |
| [回放器](../../tools/kernel-artifact-audit/run_matrix.py) | 每样本启动真实C18 CLI，核对实际退出/原因/报告及输入保持；不执行样本ELF |

## 复跑

需要普通Linux用户和Python3.11+；本项目使用WSL2。仅公开/合成资料，不能传入生产备份或私有设备固件。

```sh
# 计划：不读取样本或创建输出
python tools/kernel-artifact-audit/run_matrix.py

# 临时样本回放，stdout返回结果，临时输入在结束后清理
python3 -B tools/kernel-artifact-audit/run_matrix.py --run

# 推荐审核方式：在当前隔离源码下新建输出并保留所有合成输入/报告
python3 -B tools/kernel-artifact-audit/run_matrix.py --run --output new-c19-output

# C18必要回归
python3 -B tools/kernel-artifact-audit/test_audit.py
```

new-c19-output必须不存在，父目录已存在且不通过链接访问。不会覆盖旧结果；需要重跑时另选新名字。
每个case子目录保留manifest.json、artifacts中的合成文件、C18报告（非法输入通常没有新报告）及result.json。
顶层matrix-results.json包含实际checker/corpus摘要、UID、逐case退出码、原因、输入摘要和报告保持结果。
文件生成仅限新隔离输出，固定22case、4类基线及至多4096字节的单个样本hex，语料总量128KiB。
每次checker子进程10秒截止、空闲stdin、固定argv及最小环境，不调用shell/编译器/网络/设备。

完整回放的退出0表示22个实际结果与预期相符；case实际退出1/3/4是正常的阴性证据，不能称为checker崩溃。
任何case不符、输入变化、覆盖旧报告、子进程异常或输出异常都会使回放失败并保留证据。
回放器不是另一套制品判定实现；它不重写C18结果，也不返回伪造的observed/Unknown结果。

## 已执行的固定矩阵

以下记录来自本轮WSL2固定树的实际结果。四类输入均为明确合成文件；更改hash后仍错板、错架构或混版的样本继续核对真实字节元数据。

| case ID | 类别 | 预期退出 | 实际退出 | 结果 |
|---|---|---|---|---|
| `matching` | control | 0 | 0 | 符合预期 |
| `wrong-board` | F04 | 4 | 4 | 符合预期 |
| `missing-module` | F08 | 3 | 3 | 符合预期 |
| `missing-firmware` | F08 | 3 | 3 | 符合预期 |
| `changed-digest` | F06 | 4 | 4 | 符合预期 |
| `mixed-release` | F06 | 4 | 4 | 符合预期 |
| `mixed-vermagic` | F06 | 4 | 4 | 符合预期 |
| `wrong-elf-arch` | F05 | 4 | 4 | 符合预期 |
| `truncated-elf` | F05 | 4 | 4 | 符合预期 |
| `conflicting-metadata` | F05 | 4 | 4 | 符合预期 |
| `missing-manifest-kind` | F08 | 3 | 3 | 符合预期 |
| `unknown-license` | F08 | 3 | 3 | 符合预期 |
| `unresolved-dependency` | F07 | 3 | 3 | 符合预期 |
| `dependency-cycle` | F07 | 4 | 4 | 符合预期 |
| `duplicate-json-key` | F01 | 1 | 1 | 符合预期 |
| `unknown-schema` | F01 | 1 | 1 | 符合预期 |
| `escape-path` | F02 | 1 | 1 | 符合预期 |
| `symlink-input` | F02 | 1 | 1 | 符合预期 |
| `duplicate-path` | F02 | 1 | 1 | 符合预期 |
| `preserve-existing-report` | F10 | 1 | 1 | 符合预期 |
| `declared-budget` | F03 | 1 | 1 | 符合预期 |
| `untrusted-passed` | F09 | 1 | 1 | 符合预期 |

F01～F10是C17提案的失败分类；相同分类可有多个样本，不新增项目动作或改变权重。
完整字节/结构预算等还由C18的21项单测覆盖，两组测试可能验证相同语义，不相加当成43个独立业务验收。
单测/回放的执行环境、源码tree/提交、日志摘要和证据包均见本人报告。

## 来源、格式与隐私

corpus.json的schema=1、synthetic=true；baseline每项含固定id/kind/path、合成hex和metadata。
cases含稳定id、提案分类、有限mutation及expected；expected只规定断言，实际结果来自独立checker进程。
样本改变（如release/vermagic、DTB板型和ELF架构）会按场景重新计算输入hash，以避免仅靠摘要错误提前退出掩盖结构校验。
输入快照采用字节数/hash/文件模式或链接目标，结果不含主机绝对路径、真实SSID/凭据或载荷正文。
证据包仅放固定hex语料、逐case结果/命令/来源摘要及日志；实际合成ELF可在隔离输出中复现，不分发真实模块/固件。
原项目SPDX和GPL解析器来源保持；新脚本/样本按项目AGPL-3.0-only，无新依赖。

## 主线接手与补验清单

1. 按PR当前完整HEAD复核固定源锁是否仍与C17截面相符，读差异后在普通用户原生Linux副本运行上述两项命令；核对样本集合和实际结果，而非只读passed字段。
2. 复核pinned-input.json映射，再由负责人提供明确的公开/获准隔离制品：未压缩Image、音频DTB、22项模块；区分这些对象与boot.img/压缩归档/原装模块清单，禁止混用摘要。
3. 681原装模块及外部APR需要实际模块/来源清单补齐；当前pinned模式仍显式报告stock和固件缺口，不修改原锁的历史标志。
4. 设备厂商固件的实际摘要、来源与再分发许可由主线核验；C17/C18/C19的合成声明或GPL内核来源不能提供此证明。
5. 真正boot构造、重建、签名、模块加载、板级启动、恢复入口与掉电测试由主线在获准环境执行；本包不访问/dev、不写分区/boot/DTB/驱动/NV、不签名或加载模块。
6. Root/ARM/实机观察、健康确认和REL-01/正式发行支持仍须精确版本的实际证据与负责人签认；authorization_verified=false及boot/recovery/release_support/firmware_license未核验保持。
7. PR143前一HEAD的CI rootfs/polkit精确字节测试失败已在开发前基线ce9ccb8单独复现，相关文件本包未改；由主线处理，不放宽校验。新HEAD的CI以实际回读为准。

本次C19/C20完成的是固定合成矩阵及离线交付材料，不代表主线审核/合并或真实制品及硬件验收已经完成。
回退本包只撤工具、样本、说明和本人交付记录，不改变设备、用户数据或更新状态。整体固定P0～P7仍约80%。
