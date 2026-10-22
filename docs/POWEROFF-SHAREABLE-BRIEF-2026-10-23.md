# 可转发：MSM8916 关机返回重启，现有证据

日期：2026-10-23。以下是 RustSimHub 已保存的排查结果，供另一个会话或开发者阅读。本文不依赖本机绝对路径，不包含凭据、设备身份、SIM 信息或原始硬件日志。

后续补充：已对用户转来的分析作 [固定源码与证据边界核对](POWEROFF-EXTERNAL-REVIEW-2026-10-23.md)，包含 PMIC revision 分支、原因寄存器对应 boot、参考配置及 UAF/工作队列推论的限制；尚未恢复实机电源调试。

## 设备、系统和源码

- 自用 UFI003 / MSM8916 棒子，由 NAS 的 USB 持续供电。
- Linux + systemd；Rust 程序是用户态通信业务，不是 Rust 编写的 init 或内核。
- 实际内核：`6.12.49-msm8916-g93a71ee9468d`。
- 固定源码提交：`93a71ee9468dd8d47a9e082e5d818608f08712e3`，仓库 `KyonLi/ufi003-kernel`。
- [完整项目及文档](https://github.com/Viper-Boss/RustSimHub)。本次新增文档和正在修改的网页代码尚未同步时，公开仓库可能没有最新文件。

固定内核源码入口：

- [PMIC 电源键及关机准备](https://github.com/KyonLi/ufi003-kernel/blob/93a71ee9468dd8d47a9e082e5d818608f08712e3/drivers/input/misc/pm8941-pwrkey.c)：`pm8941_reboot_notify`、probe、notifier 注册。
- [PSHOLD 关机/重启](https://github.com/KyonLi/ufi003-kernel/blob/93a71ee9468dd8d47a9e082e5d818608f08712e3/drivers/power/reset/msm-poweroff.c)。
- [PON 重启模式](https://github.com/KyonLi/ufi003-kernel/blob/93a71ee9468dd8d47a9e082e5d818608f08712e3/drivers/power/reset/qcom-pon.c)。
- [PM8916 设备树](https://github.com/KyonLi/ufi003-kernel/blob/93a71ee9468dd8d47a9e082e5d818608f08712e3/arch/arm64/boot/dts/qcom/pm8916.dtsi)。
- [SMD 驱动](https://github.com/KyonLi/ufi003-kernel/blob/93a71ee9468dd8d47a9e082e5d818608f08712e3/drivers/rpmsg/qcom_smd.c)。
- [基带 remoteproc 驱动](https://github.com/KyonLi/ufi003-kernel/blob/93a71ee9468dd8d47a9e082e5d818608f08712e3/drivers/remoteproc/qcom_q6v5_mss.c)。

## 已确认的关机现象

正常请求是 `systemctl poweroff --no-block`。持久日志显示服务正常停止、状态分区卸载、达到 poweroff.target、进入 systemd-shutdown 并同步，随后存在新 boot。旧系统也有类似现象。

已检查 poweroff/halt 标准单元，没有发现关机被改为 reboot 的自定义别名。RuntimeWatchdog 关闭，检查时 PID 1 没有打开 watchdog 设备。尚未排除其他固件/硬件复位机制。

journald 退出后没有最终电源动作日志；没有完整 UART 关机到重启日志，没有关机瞬间 PMIC 寄存器值或 PON/POFF 原因证据。

## PMIC 线：已完成一次候选实测，但没有修好

实际配置明确存在：

```text
# CONFIG_INPUT_MISC is not set
```

PMIC pwrkey 节点存在且启用，但驱动、模块和 `pm8941_reboot_notify` 原先不存在。PSHOLD/PON 驱动已绑定。

用同一固定源码和匹配构建输入，仅增加 `INPUT_MISC=y`、`INPUT_PM8941_PWRKEY=m`，构建了临时模块。版本、别名、24 项强依赖导出检查通过；一次实际加载后绑定、notifier、新输入设备均确认，未观察到 KEY_POWER 按下或即时 Oops。

之后只执行了一次已授权的正常 poweroff，仍重新启动。没有永久安装模块或自动加载配置，没有刷写 boot、GPT、基带/NV/校准区。

**结论：缺驱动是事实，但仅补齐它不足以证明关机问题解决。** 仍需查 notifier 实际写入结果、最终 PSHOLD 路径、USB 持续供电下的启动策略或其他复位原因。

## SMD 线：已确认的新 boot 故障

重要时间边界：下述异常属于 poweroff 后返回的**新 boot**，目前没有证据证明它发生在前一 boot 的最终关机路径。

取回的持久日志顺序为：基带 intentional reset → remoteproc 恢复/MPSS 初始化 → `qcom_channel_state_worker+0x268/0x31c` NULL 解引用、访问地址 `0x14` → 后续基带初始化 watchdog。

该 boot 的模块列表不含此次临时 PMIC 模块。新 boot 的维护 SSH 能完成认证，但开启远端会话超时；物理恢复后正常 USB、SSH 和原卡注册恢复。

匹配的原内核 `vmlinux` 已反汇编核对，故障偏移对应：

```asm
ldr x0, [x19, #64]   // 取得 channel->info
ldr w0, [x0, #20]    // 读取 rx.state；这里 x0 为 NULL
```

源码 `qcom_smd.c`：`struct qcom_smd_channel` 约第199行，`GET_RX_CHANNEL_INFO` 约第283行，`qcom_channel_state_worker` 约第1272行，接收状态读取约第1295/1319行。

创建函数在共享内存取得成功且大小匹配后才设置 `info` 或 `info_word`，成功返回后才入链表。有效 channel 应至少有一个对应格式的可用指针。优先需要弄清 channel 本身、edge 或共享内存是否仍有效。

## 尚未验证的时序推测

`qcom_smd_unregister_edge` 先禁用 IRQ、取消 scan/state work，再移除子设备，最后注销父设备；`qcom_smd_edge_release` 释放 channel 和 edge。

`qcom_smd_announce_create` 等路径能够排队 state work。需要结合驱动核心 probe/remove 锁和对象引用，验证：**取消工作之后、移除子设备完成之前，已有回调能否再次排队工作，并使它活到父对象释放之后？**

这只是源码审查得到的疑点，还没有确定性离线复现、ASan/KASAN 证据或修复补丁。也未证明它与实机异常是同一根因。NULL 判断本身无法证明修复了对象生命周期。

## 希望下一位审阅者回答什么

1. 对固定提交，PMIC notifier 的关机类型设置和最终 PSHOLD 回调是否存在遗漏或失败路径？临时加载成功为何仍不足以关机？
2. 怎样利用现有驱动可提供的原因信息区分 USB 再唤醒、实际 restart 与其他复位？寄存器地址/位定义应核对固定源码和 PMIC 资料，当前没有读寄存器实测。
3. 对固定提交，SMD 停止后重新排队的交错是否真的可达？有哪些锁或引用能够证明/排除它？
4. `remoteproc` 恢复期间共享内存或 channel 列表的失效是否还有其他解释？

本材料没有确认 UART 电平，也未将外部回答中的板型电平、寄存器偏移、SysRq 强制关机或 bootloader 修改建议纳入已验证方案。当前按用户指示暂时搁置电源/内核调试，继续其他固件任务。
