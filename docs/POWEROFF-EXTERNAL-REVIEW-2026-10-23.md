# 对补充关机分析的核对

后续更新：用户已补齐pon_dump源码，已完成错误路径审查和真实离线编译，见 [P6Z](VALIDATION-PON-DUMP-REVIEW-P6Z.md)。下文“未收到”是此前状态；没有因此恢复实机电源调试。

2026-10-23。用户转来另一份源码分析。本文记录与固定源码/已有报告一致的部分、需要收窄的推论及尚缺的输入。按用户此前指示，电源/内核实机调试继续搁置；本次未连接 NAS/棒子、编译/加载模块、关机、改启动参数或刷写。

上一轮网页事件补同步修正已保存于 `cede3d7`，完整 CI 通过，尚未部署；最终文档保存点 `b102ad3`。本次不改变设备组成。整体仍 **76.5%**。

## 已重新核对的源码事实

本地读取固定提交 `93a71ee9468dd8d47a9e082e5d818608f08712e3` 的 PMIC、PSHOLD、SMD、三个 remoteproc 文件及 PM8916 设备树，PMIC/SMD SHA-256 与此前记录一致。私有索引 `.private/poweroff-external-review-20261023-first.json`。

1. `pm8941_reboot_notify` 约第86—146行确实按“清使能 → 等待 → 设置动作类型 → 恢复使能”处理；SYS_HALT/SYS_POWER_OFF 选择 shutdown。各次更新失败会报告错误，但函数最终返回 NOTIFY_DONE，不能据绑定和符号存在推定寄存器更新成功。
2. **使能寄存器存在 revision 分支。** revision=0 时使能位同动作类型位在同一控制寄存器；其他 revision 使用第二个控制寄存器。解释 `0x85a/0x85b` 时必须结合实际 PMIC revision 和 base，而非固定认为两者始终分工不变。
3. `msm-poweroff.c` 为 restart/poweroff 注册 sys-off handler，最终写零到 PSHOLD。源码路径合理，不等于已观察到本次末端硬件动作。
4. 固定 SMD 源码共5处 `schedule_work`：约673/675行 IRQ 的 scan/state、1042行 announce 的 state、1261行 scan 的 state，以及1515行注册后的 scan。销毁路径约1543—1552行先禁 IRQ、取消 scan/state，再移除子设备及注销父设备。生命周期修复的审查应覆盖全部生产者及注册失败清理，而非只修改故障读取点。
5. 固定三个 remoteproc 文件不包含字面串 `intentional reset`。这本身不能确定发起者：驱动也可能打印从固件取得的动态原因字符串。用户态主动触发和固件自行故障均不能只凭该字样判定。

固定源码入口：[PMIC](https://github.com/KyonLi/ufi003-kernel/blob/93a71ee9468dd8d47a9e082e5d818608f08712e3/drivers/input/misc/pm8941-pwrkey.c)、[PSHOLD](https://github.com/KyonLi/ufi003-kernel/blob/93a71ee9468dd8d47a9e082e5d818608f08712e3/drivers/power/reset/msm-poweroff.c)、[SMD](https://github.com/KyonLi/ufi003-kernel/blob/93a71ee9468dd8d47a9e082e5d818608f08712e3/drivers/rpmsg/qcom_smd.c)。网页取得 PMIC 源码超时，相关结论以实际读取的本地固定来源为依据；PSHOLD 网页内容也已核对。

## PON 原因寄存器：有参考价值，不能直接完成因果分类

另一个可访问的 Qualcomm 下游实现确实将 PON 原因位0解释为 hard reset、位4解释为 USB、位6解释为 CBL。其实现还区分 PMIC 代次，并有保存/清理 warm-reset 原因的逻辑。因此应先核型号、revision、读取位置和该 boot 之前的处理。[下游原因实现](https://android.googlesource.com/kernel/msm/+/866b728388bc6a1d774e8433482a08f11e8245d4/drivers/platform/msm/qpnp-power-on.c)、[较早的原因位说明](https://android.googlesource.com/kernel/msm/+/3521e061b51d717595667dc3741cfbea97f513e3/drivers/platform/msm/qpnp-power-on.c)。用户提供的 AICP 地址本次未能取到，不将它标为已独立核验。

需要收窄两条判断：

- 看到 USB/CBL 启动原因，不能单独证明前一次请求已经完整切断电源；看到 hard reset，也不能唯一归因为 PMIC notifier 写失败。还需组合本次 boot 身份、关机/复位原因、读取时间和末端路径证据。
- **必须区分“关机后返回的 boot”和“用户随后物理拔插恢复的 boot”。** 后者的原因寄存器即使读到 USB，也可能只反映那次人工恢复。应在待研究的返回 boot 中尽早捕获并保存；普通拔插后的即时读值不能回填为前一 boot 的证据。

目前没有这批寄存器实测结果，也没有收到所述 `pon_dump.c` 源文件附件。尚不能核对该模块选择的设备/regmap、revision 处理、寄存器副作用和引用生命周期，更不能仅凭描述将它当成可自动加载的候选。

## SMD：怀疑生命周期合理，但仍未证明具体机制

故障偏移和本地反汇编已确认。正常创建后入链的 channel 应有对应格式的有效指针；同时缺少可用指针，确实应优先审查陈旧对象、链表或其他内存破坏。**单独的 NULL 读取不证明对象已经释放或整体全零**，也没有给出该对象实际释放栈/分配栈。

状态读取在 channels_lock 保护中；异常后可能留下未释放的锁，进而阻塞其他访问，是合理的连锁反应假说。当前没有获得后续所有 CPU 的栈或锁诊断，不能将“某个硬中断必然卡死在该锁”写成实测结论。

单纯远端改共享内存内容不会直接清零内核结构体中的指针成员。仍需分别审查对象/链表破坏和映射生命周期；不能把这一点扩大为已经排除所有共享内存相关问题。

`cancel_work_sync` 的完成保证有前提：不能还有竞争中的排队。IRQ 与 scan 的停止顺序支持优先调查 worker 之外的迟到 announce/probe，但仍需结合驱动核心锁和对象引用证明交错可达。[内核工作队列说明](https://docs.kernel.org/core-api/workqueue.html)。

在子设备移除后再取消一次 state work，若改变结果，只能支持某些时序解释：它也可能改变时间窗口。应配合生产者/销毁跟踪或确定性交错证据；若使用停止标志，也必须将检查与排队同销毁路径正确同步。这些方案目前都没有在本项目实现、编译或实测。

## 调试配置与上游讨论的边界

已有匹配参考内核配置明确为：

```text
# CONFIG_INIT_ON_ALLOC_DEFAULT_ON is not set
# CONFIG_INIT_ON_FREE_DEFAULT_ON is not set
CONFIG_SLUB_DEBUG=y
```

这是**已有参考构建配置的读取结果**，本轮没有读取设备的 cmdline，不能代替运行时 override 的核对。

SLUB poison 和 debugobjects 可以提供诊断线索，但不能保证一定出现某条特定警告或某个特定故障地址。对象可能被重新分配、再次清零，或在不同字段先出错；保持 `0x14` 不能排除 UAF。配置本身也会改变时序和内存布局。[SLUB官方说明](https://www.kernel.org/doc/html/v5.19/vm/slub.html)、[debugobjects官方说明](https://docs.kernel.org/core-api/debug-objects.html)。

用户引用的 remoteproc 讨论确实涉及 stop 失败后再次恢复、子设备重复停止以及 NULL guard 的边界；它展示的是另一条 GLINK/子设备停止链，补丁中也包含 SMD 分支，不能直接当作本机 state worker 崩溃的现成修复。[原讨论](https://lkml.iu.edu/2512.0/02357.html)。仅查提交标题也不足以证明所有后续分支都不存在相关修复；本次不作“上游无修复”的结论。

## 留给后续恢复调试的输入

先审查实际 `pon_dump.c` 文件及适配依据，随后结合真正出故障的返回 boot 做时间对应。未来若核对模块探测与 SSR 时间，应使用同 boot 的单调时间并核对实际驱动绑定；`lsmod` 本身不能证明某个子设备 probe 正在与销毁交错。临时引导能力也尚不能假设可用，不能据“如果支持 fastboot boot”就启动旧操作计划。

本轮仅保存参考核对，不安排新的关机、自动加载或引导器修改。下一项开发继续使用当前 HANDOFF 中的软件任务入口。
