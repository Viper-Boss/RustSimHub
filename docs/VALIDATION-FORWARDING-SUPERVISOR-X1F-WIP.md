# X1F 转发服务管理：未完成检查点

后续：本文保留验收前历史。当前PID1方案已经通过419项Rust、真实systemd/凭据/cgroup及ARM64验证，见[后续验证](VALIDATION-FORWARDING-SUPERVISOR-X1F.md)；未安装/部署，Core自动协调及网页仍缺。下文的额度限制和未运行状态不覆盖后续结果。

2026-10-25。**WIP，尚未通过完整回归或服务验收，不可据此部署。** 已发布且验证通过的基线仍为`c76c825a4ff7ca09a1107b424e6273de299b9a39`（X1E、416项Rust、[CI40109167277](https://github.com/Viper-Boss/RustSimHub/actions/runs/40109167277)成功）。当前修改仅保存在工作区，原P0–P7进度仍80%。

## 当前源码

- `crates/forwarder/src/supervisor.rs`及`supervisor/{files,network,system}.rs`：Root有限reconcile/status/retire动作；固定生产单元和路径；Root监测报告的boot/新鲜度/明确Wi-Fi或USB接口；持久开机标记、实际单元/cgroup/专用UID进程检查、Root原子状态报告。
- `worker.rs::read_service_settings`和`main.rs --service-worker`：读取PID1提供的有界、私有只读credential，沿用X1E的真实Core fence及绑定出口。
- Core/worker协议新增Root专属条件Retire，以当前能力摘要匹配后才撤销；旧进程清理不能误撤销替换后的能力。Root State增加当前Core实例、Owner绑定与能力摘要。已写竞争回归，完整测试尚未运行。
- `firmware/system/forwarding/`：待安装的worker、有限管理/条件启动单元、polkit、接口选择及账户/目录候选；不在现有自动overlay中，未安装或启用。
- `tools/test-forwarding-supervisor.py`：只用回环网络、合成消息及唯一临时systemd单元验证真实启停与cgroup回收。目标包括不重复启动、收到两次HTTPS确认、额外子进程拒绝假报正常、强制回收和OFF后的启动守卫。**这些目标尚未全部通过。**

## 已观察到的结果

1. 早期开发夹具构建通过；最新PID1凭据方案的forwarder开发构建（tenth）通过，之后又有静态修改，尚未重建。不能拿旧构建结果证明现在的完整工作区。
2. 原Root启动器路径的实际测试失败，已保留first至eighth日志。实际systemctl空闲Job输出为空字符串，已兼容；Core受理前Busy使用有限重查。没有把失败写成成功。
3. 短命独立探针确认，本机部分systemd隔离属性组合会使Root进程的有效CAP_SETUID缺失。增加Root环境能力的候选编辑被自动审批拒绝，**该编辑未执行**。未移除隔离或放松权限检查。
4. 代码已改成PID1直接创建非Root、空能力worker，Root只准备有限配置/启停，使用LoadCredential。旧Root降权/常驻启动器及其调试入口已从当前源码移除。
5. 新方案的tenth服务测试在PID1解析夹具数字UID时失败（Unknown user，尚未启动worker）。夹具已改用现有NSS nobody身份，不新建账户；生产候选仍要求独立锁定账户。**修正后的测试尚未执行。**
6. 下一次服务测试未获执行：自动审批服务报告使用额度限制，提示3:26 AM后可再试。这是审批服务未完成检查，不能解释为测试失败或“不安全”。不得改用其他执行入口绕过。

最后一次已执行服务夹具terminal exit为1，finally收尾未报告另一个异常；没有本轮完整的最终进程/PID再核对。旧构建句柄后续查询返回Unknown process id。没有因观测结束重启测试。未访问NAS/棒子、未安装生产服务、未发短信/电话、未启用普通数据；单条CXYE授权仍已消费，剩余发送次数0。

## 接手顺序

1. 从`.private/forwarding-supervisor-checkpoint-20261025-first.json`核对实际文件摘要、失败日志和审批状态。保持并行README任务留下的HANDOFF/PROGRESS笔记，不把它们混入本任务提交。
2. 审批额度恢复后，正常走审批重建当前Core/forwarder及开发夹具；不复用旧Root降权架构。先检查临时fixture专用单位是否已清理，再运行PID1凭据方案。若nobody已被其他进程占用，不能杀掉其他进程来让专用UID检查通过。
3. 完成真实systemd/cgroup/零能力/credential只读交付、OFF后运行时配置和整个进程组清理、额外进程和触发器拒绝、受理前Busy及迟到清理不影响新能力的回归。
4. 完整workspace测试、严格Clippy、生产feature检查、ARM64/QEMU与精确SHA CI；当前新增三个Rust用例仅已编写，不能计入通过数量。补充credential/文件元数据失败路径、启动时Core就绪与网络报告时序验证。
5. 再接真实Core管理单元协调及新鲜Root回执、Owner网页开关/规则/回执、一致性备份和签名安装。独立账户、安装门禁、设备OFF/冷启动仍缺；不得据本机条件启动片段宣称完整冷启动通过。

参考：[systemd私有credential](https://www.freedesktop.org/software/systemd/man/latest/systemd.exec.html#Credentials)、[整组退出语义](https://www.freedesktop.org/software/systemd/man/latest/systemd.kill.html)、[systemd授权详细字段的上游源码](https://github.com/systemd/systemd/blob/v257/src/core/dbus-util.c)。设计参考不能替代当前实现验收。
