# 棒子无法关机：排查结论与代码接手说明

日期：2026-10-23。适用项目：RustSimHub，UFI003 / MSM8916。整理依据为已保存的实机报告、固定内核源码审查和匹配内核的反汇编结果。本次仅整理交接，没有再次连接设备、加载驱动、关机或刷写。

## 1. 先说判断

**优先看 Linux 内核的高通电源管理、基带恢复和 SMD/rpmsg 驱动，当前没有证据把问题指向 Rust 业务代码。** 这里有两个需要分别解决的问题：

| 问题 | 当前判断 | 证据强度 |
|---|---|---|
| 正常关机后又启动 | 优先查 PMIC 的关机类型是否真正设置成功，以及 PS_HOLD 拉低后是否被 USB 供电/固件启动策略重新唤醒 | 已确认现象，最终原因未确认 |
| 返回的新系统维护会话异常、基带恢复失败 | 已抓到 `qcom_channel_state_worker` 的内核空指针异常；优先查 SMD channel/edge 的对象生命周期和恢复时序 | 异常位置已确认，导致指针异常的原因未确认 |

两者可能有关联，也可能是“关机返回新 boot”暴露了另一处恢复缺陷。**不能用第二个问题的崩溃位置，直接证明第一个问题的原因。**

此前“缺失 PMIC 驱动就是高可信关机原因”的判断需要收窄：驱动确实缺失，但实际补上后仍没有得到持续关机，单独补驱动不是已验证修复。

## 2. 目前设备和工作状态

- 最后一次已完成的只读复核：正常 USB、固定身份 SSH 恢复，原联通卡正常注册。
- 普通蜂窝数据关闭，且没有普通 gsm 连接配置；没有重新启用流量测试。
- 临时 PMIC 模块、暂存目录、电源键阻止租约及清理定时器均已消失。
- PCM 关闭，正式业务仍未初始化；Root 网络监测与灯控正常运行。
- 本轮没有永久安装 PMIC 候选，没有改 GPT、boot、基带、NV 或校准分区。
- 关机修复尚未通过。目前不需要你拔插；本交接不要求继续执行旧测试脚本。

以上是已保存的最后一次实机状态，并非本次整理文档时重新读取的现场状态。

整体进度仍为 **76.5%**。本次诊断和交接不增加已验收里程碑。

## 3. 已经实际排查了什么

### 3.1 用户态关机流程

持久日志确认系统进入了正常关机流程：服务停止、状态分区卸载、到达 `poweroff.target`、进入 `systemd-shutdown` 并同步文件系统，之后出现新 boot。

检查未发现把 poweroff/halt 改成 reboot 的自定义单元。RuntimeWatchdog 关闭，检查时 PID 1 没有打开 watchdog 设备。

因此当前不优先怀疑网页按钮、Rust 发错关机命令或 systemd 单元别名。但日志在 journald 退出后停止，**没有覆盖最后的寄存器写入和硬件电源动作**；也没有排除所有固件/硬件复位源。

### 3.2 确认 PMIC 关机准备驱动缺失

实际内核配置中：

```text
# CONFIG_INPUT_MISC is not set
```

设备树的 PMIC pwrkey 节点存在且启用，但没有对应模块、绑定及 `pm8941_reboot_notify`。PSHOLD 和 PON 驱动已经绑定。

容易遗漏的地方：`pm8941-pwrkey.c` 虽然位于输入驱动目录，还承担关机前的 PMIC 配置，不只是处理按键。物理 GPIO 按钮能用，也不能证明该 PMIC 驱动存在。

### 3.3 离线构建及一次临时实机试验

实验配置只增加：

```text
CONFIG_INPUT_MISC=y
CONFIG_INPUT_PM8941_PWRKEY=m
```

候选使用固定原源码和匹配工具链构建，版本、别名及 24 项强依赖导出核对通过。没有关闭严格符号检查。

实机临时加载一次后，确认模块加载、节点绑定、notifier 符号、新输入设备存在；三次非阻塞采样未看到电源键按下，加载后的即时日志没有 Oops。随后仅执行过一次已授权的正常 poweroff。

结果：USB 先消失后重新枚举，持久日志确认出现新 boot，**未实现持续关机**。新 boot 后的两次维护 SSH 身份认证成功，但开启远程会话超时。后续经用户物理恢复，最终只读健康复核通过。

中途一次观察到 9008 模式，未运行刷写器或发送 reset。不能据此断言按键卡住，也不能断言固件永久损坏。

### 3.4 找到基带恢复期间的内核异常

取回的返回 boot 日志显示以下顺序：

1. 基带发生 intentional reset。
2. remoteproc 进入恢复并重新初始化 MPSS。
3. 内核在 `qcom_channel_state_worker+0x268/0x31c` 发生 NULL 指针异常，访问地址为 `0x14`。
4. 随后出现基带初始化 watchdog，工作线程还留下中断/抢占状态异常记录。

这为维护会话失效提供了明确内核故障线索，不能只当普通网络超时处理。**异常所属 boot 的模块列表不含此次临时 PMIC 模块，尚无证据证明 PMIC 模块直接造成该异常。**

## 4. 最值得看哪些代码

固定源码仓库：`https://github.com/KyonLi/ufi003-kernel`。

固定提交：`93a71ee9468dd8d47a9e082e5d818608f08712e3`。

实际内核版本：`6.12.49-msm8916-g93a71ee9468d`。阅读和修改都应以这个提交为基准；其他版本行号、结构布局和补丁不一定适用。

| 文件 | 优先阅读的位置 | 要回答的问题 |
|---|---|---|
| `drivers/input/misc/pm8941-pwrkey.c` | `pm8941_reboot_notify`、probe、notifier 注册 | SYS_POWER_OFF 时到底写了什么？写操作是否成功？是否有后续覆盖？ |
| `drivers/power/reset/msm-poweroff.c` | 最终关机/重启回调和 PS_HOLD 写入 | 最终实际走的是哪条回调？PMIC 准备与 PS_HOLD 动作顺序是否正确？ |
| `drivers/power/reset/qcom-pon.c` | 重启模式与 PON 相关处理 | 此驱动负责什么，哪些行为仍由其他驱动或固件决定？ |
| `arch/arm64/boot/dts/qcom/pm8916.dtsi` | PON/pwrkey/resin 节点，结合实际板级覆盖 | 实际启用节点及硬件描述是否与板子一致？ |
| `drivers/rpmsg/qcom_smd.c` | 下节所列函数 | 恢复、移除和异步回调之间，是否还有代码在访问已失效对象？ |
| `drivers/remoteproc/qcom_q6v5_mss.c` | MPSS 启停与故障恢复路径 | intentional reset 的来源、SMD edge 注销和恢复次序是什么？ |

最后一项属于下一步需要串起来的调用链，尚未完成深入审计。不要把待查项目写成已定位原因。

本机 WSL 原始源码位置：

```text
/home/wzs11/.cache/rustsimhub/full-kernel-rebuild-20261021-second/source/linux
```

匹配反汇编参考：

```text
/home/wzs11/.cache/rustsimhub/full-kernel-rebuild-20261021-seventh/build/vmlinux
```

这两份是已有来源/复现证据，研究时建议另复制工作树，保留原件。

## 5. SMD 已经精确定位到哪里

### 5.1 故障指令已经核对

匹配 `vmlinux` 中该函数的 `+0x268` 为：

```asm
ldr x0, [x19, #64]
ldr w0, [x0, #20]   // 故障位置：x0 为 0，访问到 0x14
```

结合该固定版本的结构及代码，对应从 `channel->info` 读取接收侧状态。源码入口：

- `struct qcom_smd_channel`，约第 199 行；`info`、`info_word` 约第 210—211 行。
- `GET_RX_CHANNEL_INFO`，约第 283 行。
- `qcom_channel_state_worker`，约第 1272 行；状态读取在约第 1295、1319 行。

字节格式指针 `info` 和字格式指针 `info_word` 是二选一，不要求两个都非空。但一个有效、已创建的 channel 应具有可用的对应指针。

### 5.2 为什么优先怀疑生命周期或链表失效

`qcom_smd_create_channel` 在 `qcom_smem_get` 成功并确认结构大小后才设置对应指针，失败会清理；成功返回后才将 channel 加入列表。现有审查没有发现正常路径主动把这两个指针同时清零。

因此，比起简单加一个 NULL 判断，更应该先回答：worker 取到的是否仍是那个有效 channel？edge/channel 是否已经进入销毁？共享内存是否在远端重启后仍有效？链表是否被其他路径破坏？

目前只确认了 NULL 解引用的位置。**尚未证明是释放后使用、共享内存失效或哪一处写坏了对象，也未完成离线复现。**

### 5.3 当前最可疑的销毁时序，仍是推测

重点交叉阅读这些函数：

```text
qcom_smd_announce_create
qcom_smd_channel_scan_worker
qcom_channel_state_worker
qcom_smd_register_edge
qcom_smd_unregister_edge
qcom_smd_remove_device
qcom_smd_edge_release
```

已审查到的关系：

- IRQ、扫描 worker 及 `qcom_smd_announce_create` 等路径可以排队状态工作。
- `qcom_smd_unregister_edge` 先禁用 IRQ，取消扫描/状态工作，再移除子设备，最后注销父设备。
- `qcom_smd_edge_release` 会释放 channel 列表和 edge。
- 状态 worker 的源码注释依赖一个前提：“所有 channel 释放前，worker 已经停止”。

**待验证的问题：取消工作之后、子设备移除完成之前，正在进行的子设备 probe/announce 回调会不会再次排队状态工作？** 如果可能，并且父对象随即释放，就可能破坏上述前提。

这只是一个需要用驱动核心的 probe/remove 锁、对象引用计数和工作队列语义进一步证明的时序。尚未运行确定性交错复现、ASan/KASAN 验证，也没有生成或上机任何 SMD 修复补丁。另一个待审查点是成功申请 IRQ 后的注册失败清理，是否也完整停止了工作。

## 6. 建议你接下来怎么研究

1. **先分清两条线。** 电源线回答“为什么又启动”，SMD 线回答“为什么恢复后的内核出错”。即使修好了后者，前者也要单独验收。
2. **电源线先看结果是否可观测。** 核查 notifier 的实际返回与写入错误、最终关机回调、PMIC 启动/复位原因，以及 USB 持续供电时的板级启动策略。最终动作发生在普通日志退出后，可能需要已有串口或其他可靠末端日志；这些证据目前没有。
3. **SMD 线先画出生产者和销毁顺序。** 找出所有 `schedule_work`、`cancel_work_sync`、子设备 probe/remove、edge/channel 引用与释放。重点验证取消工作后能否再次排队，而不是只检查 worker 内部空指针。
4. **如果时序可达，再做离线复现。** 尽量使用固定版本的实际函数，控制“取消工作 → 迟到回调 → 释放 → 排队工作执行”的顺序；原版必须确实失败，候选版必须阻止同一失败且保留正常收发。模拟不能替代实际内核锁和引用关系的审查。
5. **有证据再决定改法。** 如果证实是停止后重排队，需要保证“停止标志检查与排队”同销毁路径正确同步，并审查所有生产者和错误清理路径。单独 `READ_ONCE(stopping)` 或在故障点加 NULL 判断，未必解决检查与排队之间的竞态。
6. **最后才考虑新内核实机验证。** 离线构建、启动成功、基带正常、恢复稳定和真正持续关机应分项记录，不能相互替代。

目前不建议直接永久安装 PMIC 候选并称作修复，也不建议通过关闭恢复机制、吞掉空指针或把 poweroff 替换为 halt 来判断成功。

## 7. 现有报告和制品在哪里

下面的私有文件包含原始诊断材料，只供本机查看，未放进公开仓库。

| 路径，相对项目根目录 | 内容及边界 |
|---|---|
| `docs/VALIDATION-POWEROFF-DIAGNOSIS-P6X.md` | 最初只读诊断与 PMIC 离线构建；其中较强的因果推断需结合后续 P6Y 阅读 |
| `docs/VALIDATION-PMIC-TRIAL-P6Y.md` | 一次实际加载/关机、失败与最终恢复事实；顶部为最新状态，下面保留过程历史 |
| `.private/poweroff-offline-candidate-20261023-third.json` | PMIC 模块成功构建索引，不是关机修复成功报告 |
| `.local/poweroff-driver-candidate-20261023-third/pm8941-pwrkey.ko` | 当时临时试验的模块，尚非可永久安装的已验证修复 |
| `.private/pmic-load-and-poweroff-20261023-first.json` | 一次实际加载/关机；`passed` 表示流程执行完成，`hardware_poweroff_verified=false` |
| `.private/pmic-reconcile-20261023-first.json`、`second.json` | 关机返回后两次只读会话超时记录 |
| `.private/pmic-final-health-20261023-second.json` | 最终恢复成功；含 `previous_boot_kernel`、`test_boot_shutdown`、`boot_list` 等原始材料 |
| `.private/smd-oops-map-20261023-first.json` | 已通过的故障偏移、反汇编和源码对应关系核验，未访问设备 |
| `.local/smd-oops-map-20261023-first.py` | 上一项核验脚本；输出文件已存在，不应原样覆盖重跑 |
| `.private/full-kernel-rebuild-20261021-seventh.json` | 匹配基础内核及模块的复现参考，不是 SMD 修正版 |

便于检查是否看对输入的摘要：

```text
qcom_smd.c SHA-256:
c5376d9d5f28805bc7c7b153379e1fbffdc4accbd1a00ec49c39916b47d55098

参考 vmlinux SHA-256:
14a82ba2e71e7f8b72d2ba4bc71e4ab5dd6b923260dce0f02b1cf11087f7063e

临时 PMIC 模块 SHA-256:
50d9015d66fc25866dc68489068948b33226f66319f2279badb0d258a4979f62
```

## 8. 给下一位接手者的准确起点

已有资料基线提交为 `7b1c1610e32d28108ffe1b74f7e4434bdb371148`；它是文档/代码接手基线，不代表设备刚刷入这一版本。

下一项尚未完成的工作是：**证明或排除 SMD 停止/销毁期间重新排队的时序，同时继续查清 PMIC 设置生效情况及关机后重新启动的硬件原因。** 当前没有 SMD 修补制品、没有 ASan/KASAN 复现结果、没有再次关机成功证据。

不要重跑已消费的一次性加载/关机或旧安装脚本。先看现有报告和固定源码即可；本文件不是新一轮硬件操作计划。

## 9. 关于界面“无法显示此内容”

截图表示平台对某条内容或请求显示了访问限制提示。仅凭截图无法确定触发的具体内容，也无法确认是否属于误判；此前设备恢复和代码诊断记录本身，不能解释平台的判断。

本交接只整理自有设备的关机故障和现有代码证据，不尝试绕过平台限制。你当前可以直接从上述固定源码与本地报告继续研究。
