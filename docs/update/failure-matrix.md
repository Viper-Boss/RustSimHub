# C13 更新器真实失败矩阵

来源：已合并的[阶段一提案](failure-matrix-proposal.md)（PR #81）。实现基线main为
`b6c73c6a47e00654bff51ae2b221fdcd7e9458e0`。本包仅测试公开Store API，不修改生产逻辑。
本机最终执行结果、精确源码tree/提交和命令记入[成员报告](../deliveries/member-c-nas.md)。

## 入口与范围

Windows已安装的WSL2入口：

```sh
python tools/fixtures/backup-recovery/wsl_test.py --run --suite update --ref HEAD
```

默认去掉`--run`只列计划。入口先固定Git对象并归档，使用普通Linux UID、Linux原生source/TMPDIR/target、
固定Rust1.98.0及锁定离线依赖。没有缓存时显式`--fetch`准备公开依赖；不读取设备或生产备份。
调用方可明确传入暂存tree，日志会区分tree与commit，不能把两者互称。

Linux原生隔离源码中：

```sh
python3 tools/test-update-failure-matrix.py --run
```

驱动只执行固定Cargo集成测试`failure_matrix`，校验7个完整函数名与实际通过集合相等、零失败/零忽略。
每项覆盖format1/2；P07每格式另分同key低floor与异key高floor两种有效台账。
这是7个Rust测试函数的参数覆盖，不折算为新的验收步数。合成Ed25519密钥、A1/C2/C3和64字节ARM头都仅是测试材料，
ARM头从不执行，密钥不写入报告。无新依赖、真实服务、Root准入、硬件故障或C14内部中断结论。

## 真实故障与检查

| ID | 故障动作 | 断言与重开证据 |
|---|---|---|
| P01 | 普通用户将versions改0500，真实目录probe返回EACCES，原stage返回Io | 原确认组内容/state不变、无新候选目录；恢复测试权限后重开选择A1；另设可写独立对照完成stage/choose/confirm |
| P02 | 原Store仍打开时预置torn state.next，stage真实create_new冲突 | 原state与A1不变，完整孤儿C2保留；内存floor2/candidate C2与磁盘floor1分别核对；丢弃句柄重开后由产品清理torn next，仍选A1 |
| P03 | C2已暂存，预置torn next后choose持久化失败 | 本次返回Io而非可执行路径；磁盘attempted=false，句柄true；重开首次成功choose才写尝试，第二次重开回退A1、floor2不降 |
| P04 | C2成功choose后预置torn next，confirm持久化失败 | 磁盘仍confirmed A1/candidate C2/attempted=true；句柄已改confirmed C2不冒充持久成功；丢弃句柄重开choose回退A1；签名文件保留 |
| P05 | selected已指A1，C2已暂存；runtime改0500且通过原路径预检 | 独立probe真实EACCES；select_runtime返回Io，旧selected仍A1，无selected.next；磁盘已记录C2 attempted=true；恢复权限重开回退A1，floor2保持 |
| P06 | C2选择且确认成功后重复confirm | Denied；原state和完整组内容保持；重开仍选择已确认C2 |
| P07 | 写入真实有效的旧A1低floor台账，或另一合成key真实安装的floor3台账作为next | 两种输入均非损坏JSON；原open拒绝接纳并清理next，磁盘原台账、floor2、C2未尝试状态与所有组内容保持；不靠key错误同时低floor掩盖检查 |

所有故障前、错误返回后、重开前后均核对原state或组的字节内容，临时组很小，不输出内容。
原Store::open/choose有恢复/写台账副作用，所以先采样原磁盘，再调用重开选择。
权限RAII恢复只作用于本测试新建目录；从不删除state/floor或孤儿组来让断言通过。

## 失败语义与验收边界

P02/P03/P04如实刻画当前“先改内存再persist”的行为：错误返回后丢弃句柄并重开，
**不能复用句柄status当磁盘成功证明**。这不改变库的生产契约，也不替主线决定是否需要事务式内存回滚。
P05保留当前“先记录attempt再发布链接”的顺序，不重试同一候选或增加试运行预算。
上述差异通过真实API断言；未新增测试专用生产hook。

EACCES来自独立probe的真实errno13；Store仍只暴露Io，不能声称API保留底层errno。
state.next冲突不是磁盘写满，权限故障不是掉电，内核页缓存下重开不是fsync断电证明。
tmpfs ENOSPC、精确子进程异常退出及联合schema/outbox/回执兼容属于C14，尚未由本矩阵证明。
现有Root metadata ignored测试若未单独运行，报告必须保留未运行标记。

代码来源/许可：复用项目原公开API及合成ELF结构方法，保留AGPL-3.0-only与依赖原许可；
不复制或分发生产签名材料/固件。C13离线结果不等于Root/ARM/设备恢复或负责人验收。
