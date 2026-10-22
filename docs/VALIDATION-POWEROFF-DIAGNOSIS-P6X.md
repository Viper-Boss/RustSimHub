# P6X：关机后重启的只读诊断与离线候选

2026-10-22开始，2026-10-23收尾。用户选择“排查关机后自动重启”小任务。本轮完成现场只读诊断及最小驱动候选的真实离线编译，**没有安装/加载候选、再次请求关机或刷写**。整体保持76.5/100，尚不能宣称关机修复通过。

## 已确认的事实

1. 定位到R11实际poweroff那次boot的持久日志：业务/维护服务正常停止，NSHSTATE分区卸载，达到poweroff.target，进入systemd-shutdown并同步文件系统；约两秒后有新boot。不是只根据USB重枚举猜测重启；日志在journald退出后停止，不能证明最终电源寄存器值。
2. poweroff/halt单元保持系统标准定义，没有发现把关机改成reboot的自定义配置。RuntimeWatchdog关闭，PID1当时检查没有打开watchdog设备；这些不能排除所有固件/硬件复位机制。旧系统也有poweroff返回新boot的历史记录，本次不重复消费旧停机helper。
3. 当前实际内核为固定上游93a71ee的6.12.49-msm8916；读取`/proc/config.gz`确认`CONFIG_INPUT_MISC`未开启，`pm8941-pwrkey`模块不在模块目录，驱动未绑定，内核符号中没有其reboot notifier。实时DT的PMIC pwrkey节点存在且未禁用；resin节点禁用，实际按钮是单独GPIO按钮，不能据按钮可用推定PMIC驱动存在。
4. 实际PSHOLD及PON驱动已经绑定；固定源中的`qcom-pon.c`处理重启模式，`msm-poweroff.c`在最后拉低PSHOLD。缺失的`pm8941-pwrkey.c`还承担PMIC关机准备：在SYS_HALT/SYS_POWER_OFF时设置PS_HOLD shutdown类型，再由最后的断电路径执行。

因此，**缺少PMIC关机准备驱动是已确认的配置缺陷，也是此次poweroff变重启的高可信解释**。仍需加载后确认绑定/输入行为并进行一次有恢复条件的受控实测，才能确定因果并排除USB供电唤醒或固件行为；本轮没有读取/写入PMIC控制寄存器，不能将推断写成实机修复成功。

源码依据为固定上游的 [PMIC驱动](https://github.com/KyonLi/ufi003-kernel/blob/93a71ee9468dd8d47a9e082e5d818608f08712e3/drivers/input/misc/pm8941-pwrkey.c)、[PSHOLD驱动](https://github.com/KyonLi/ufi003-kernel/blob/93a71ee9468dd8d47a9e082e5d818608f08712e3/drivers/power/reset/msm-poweroff.c) 和 [PM8916节点](https://github.com/KyonLi/ufi003-kernel/blob/93a71ee9468dd8d47a9e082e5d818608f08712e3/arch/arm64/boot/dts/qcom/pm8916.dtsi)。实际读取已有固定来源的本地源码；网页抓取超时，随后GitHub API独立核对PMIC源文件blob为`bab710023d8f1c2bbd04ca02d1571fddafc89006`，与本地编译输入一致。

## 最小候选及实际离线验证

新增独立 [配置片段](../firmware/kernel/pmic-poweroff.config)：`CONFIG_INPUT_MISC=y`和`CONFIG_INPUT_PM8941_PWRKEY=m`。不修改用于逐字节复现旧基础内核的base-build-profile，也不默认接入系统组成、模块自动加载或安装器。

在新本机WSL输出中，从已验证原配置运行olddefconfig，有效配置只改变上述两项。使用原固定源码、Debian GCC14.2/binutils2.44及已复现基础内核的真实vmlinux.o执行严格modpost，实际生成17,344字节AArch64模块，SHA256为`50d9015d66fc25866dc68489068948b33226f66319f2279badb0d258a4979f62`。内核版本标记、DT别名及24项强未定义符号所需的原内核导出均核对通过；所检查源码、原配置和原导出表保持。没有关闭未定义符号检查、伪造符号或使用warning放行。版本标记和导出匹配不替代实际加载/ABI与硬件行为验收。

首次失败是私有bison数据目录未指定；第二次已完成配置/对象编译，但缺少vmlinux.o导致严格modpost拒绝。两次失败材料均保留。第三次只补齐新输出的正确构建输入后通过，不改原参考输出。

唯一成功候选索引`.private/poweroff-offline-candidate-20261023-third.json`；制品`.local/poweroff-driver-candidate-20261023-third/`。三个只读报告`poweroff-readonly-20261022-first/second/third.json`和最终`poweroff-final-readonly-20261023-first.json`均通过。所有原始日志、boot标识与私有状态只留.private/.local。

## 现场与下一步

最终只读复核同一boot、原卡普通gsm断开且无gsm连接配置、PCM关闭、正式业务仍未初始化；本轮无新电话/短信或互联网流量测试。设备仍使用此前已安装组成，**本候选未上机、关机自动重启仍待实测解决**。

后续作为独立维护任务：先核恢复材料/维护通道和空闲状态，审核新增PMIC输入设备及电源键事件策略；选择与实际内核一致的模块交付方式，确认加载、绑定与notifier，再安排一次正常poweroff及NAS端观察。实际断电/持续停止与仅CPU halt分开验收；若仍重启，继续查PMIC启动原因、USB供电唤醒及串口最后日志。不能把旧系统halt后SSH不可达等同于新系统真正断电，也不把poweroff偷偷替换成halt掩盖问题。
