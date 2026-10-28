# D-24 候选服务材料与真实拒绝边界

状态：**可离线检查的待审材料，未批准/安装/启用；D-24未完成，不减账。** 日期2026-10-28。基线PR #51的9f4ff043；新增材料位于 `firmware/system/ble/`，没有进入rootfs overlay，也没有改Root安装器或创建账号。用户本轮明确“A尚未完成，保持原分工”，A的密码库不接管，D-21～23生产接线继续等待其真实接口。

## 一、发现的实际兼容冲突

`tools/system/setup_ap_install.py::identities` 实际检查四组固定账号：Core986、Edge985、Media984、Forwarder983；共享IPC组987仅允许这四名成员。原型提议新的BLE UID/GID **982**，只是一份**未保留的候选号**，本轮没有读取设备实际NSS或断言该号可用。

真实隔离测试先把合成完整旧身份清单交给现有 `identities`，通过；再加入BLE982并将其加入IPC组，旧检查明确报 **foreign IPC group member**。不是猜测，也没有放宽旧检查。故候选sysusers不能直接应用，否则后续AP维护可能被拒绝。

主线/Root材料审查需要明确一个**新的有限角色契约**，同时绑定账号名/UID/GID、只在新模式允许的IPC成员、Core有限路由及确切应用源/版本。不能全局把旧模式的4人检查改成“任意用户均可”，不能因为模板写982就宣称系统已分配982。C只在接纳新契约后消费并组装材料；旧Root模式继续保持旧许可表。

## 二、这次真正实现的文件

| 文件 | 用途 | 运行/权限边界 |
|---|---|---|
| `contract.proposal.json` | 固定proposal角色、候选ID、四份材料及T5引用 | production_ready/default_enabled恒为false；没有approved捷径 |
| `rustsimhub-ble.service.in` | 有限服务候选 | 仅模板，无Install/WantedBy，不注册或启动服务 |
| `rustsimhub-ble.sysusers.in` | 982和IPC成员提议 | 不执行sysusers、不改NSS，现有AP旧角色检查会拒绝 |
| `rustsimhub-ble.policy.in` | 仅BLE用户的固定D-Bus方法集合 | 只面向BlueZ/总线元操作；无全局策略、own、Properties.Set或网络管理权限 |
| `material_plan.py` | 实际只读检查器与四文件摘要计划 | 拒绝扩大权限/命令/路径/启动行为、未知字段/布尔冒充批准、文件替换/链接/长度、缺失/别名身份；无安装API |
| `test_inspect.py` | 13项边界/实际系统工具测试 | 只在临时目录、独立Unix总线与合成身份中运行 |

服务候选以新低权角色运行，空capabilities、AF_UNIX、PrivateDevices/Network、只读系统、不可见业务状态目录，只从Root的临时桥策略使用LoadCredential。候选桥策略只说明传输配置，**不是Core授权**，不允许把其文件当恢复能力。RuntimeDirectory私有0700，不持久化业务/Owner私钥。

预算提议48MiB、16任务、CPUQuota10%、无core dump；这些是待测上限，不是已测性能结论。Restart=no；关闭使用cgroup退出，无任意Root清理命令。25秒停止窗口考虑多次5秒调用及调度余量；**不是保证所有后端一定25秒内证明清理成功**。迟到/未确认仍需GATT owner保留未知状态，最终强杀后的真实BlueZ资源回收必须设备验收。

默认不启用；Core没有反向依赖BLE，无BLE应继续运行Core。Condition只是启动前条件，不是文件归属/策略真实性的检查，也不能替代未来程序的私有文件校验。正式Root入场必须同时验证可信enable标记、实际程序/模式、签名材料、A安全会话、D-22 Core路由与角色；目前这几个前置缺失，所以检查器始终输出install_permitted=false，不能拿JSON字段自报ready。

## 三、实际测试和失败历史

```sh
python3 firmware/system/ble/material_plan.py
python3 firmware/system/ble/test_inspect.py
python3 tools/progress/build.py --check
```

最终验证截面 `f6a303f68195a06ca41ebb65309b1ac7c54955e41b030a8e9a33824129d8ce20`，日志摘要 `201d2b7dc0a8a99cf88e5b0a3b413e7bd87f7b59e3a329e126e15b48f82902e9`。WSL/Linux、Python3.14、实际UID65534/无capabilities、无网络/设备、源码只读；**13项通过、0跳过**，包括：

- 实际候选的只读哈希计划；字段/路径/角色/版本/批准冒充拒绝；大小、目录、软/硬链接、FIFO、过程中材料变化拒绝。
- root、额外命令、任意设备、AF_INET、额外组、无限内存、自动重启/启用、条件重置等真实模板变异拒绝。
- 现有AP Root账号检查的实际前后对照，证明第五IPC成员必须协调新契约。
- **真实私有dbus-daemon/dbus-send**：仅把候选策略用户名映射为测试实际低权UID（不创建用户），其余规则不改。GetNameOwner允许；ListNames、RequestName及错误接口的Properties.Set实际AccessDenied。回复只允许已请求的对应回复。该测试证明有限总线元操作策略，不声称真实BlueZ注册/适配器/多UID或设备策略通过。
- **真实systemd-analyze verify --root**：实际模板放进临时root，提供同路径的业务占位程序（权限可执行，但测试不运行）和依赖占位单元，只做解析，未start。证明系统工具接受模板，无未知选项；不证明真实NSS、cgroup、LSM、信号退出或资源预算生效。

前次Windows原生测试被沙箱临时目录ACL拒绝，且没有Linux总线/systemd，未宣称原生全通过。首次Linux的脚本名inspect.py遮蔽Python3.14标准库，已改名material_plan.py；保留原失败日志（6d9e868a截面），不关闭异常或跳过真实测试。上述13项全绿才是最终边界；通用Rust计数器的0不能冒充没有Python测试，私有证据另记python_test_entries=13。

## 四、关闭验收仍欠什么

1. 明确审核并冻结角色/实际ID/许可表，核对设备local+NSS占用，不对已有账号做rename/抢UID。
2. 接纳D-21数据/权限方案与A真实安全会话；D-22固定IPC使用真实SO_PEERCRED，只进有限恢复路由，不能扩大普通Core Owner API。
3. 真正的BLE服务程序/模式、私有策略检查、无BLE退出和有界SIGTERM/迟到请求清理；本次D-17～20只有库，候选ExecStart程序尚不存在。
4. 真实Root材料签名/安装、NSS/cgroup/BlueZ权限、开启/关闭无PID/无本epoch广播或会话、Core独立运行及旧模式AP兼容。

因此本次是D-24的具体材料/可执行检查前置，**D-24仍未完成**；D-21～26仍6步，总118不变，台账20/169、加权10.6%、原里程碑约80%保持。不能把sysusers/模板/审查JSON算已安装系统，也不让本次材料进入正在由Claude推进的设备应用组。

官方依据：[systemd.exec](https://www.freedesktop.org/software/systemd/man/latest/systemd.exec.html)、[systemd.service](https://www.freedesktop.org/software/systemd/man/latest/systemd.service.html)、[dbus-daemon配置](https://dbus.freedesktop.org/doc/dbus-daemon.1.html)、[BlueZ GattManager](https://github.com/bluez/bluez/blob/master/doc/org.bluez.GattManager.rst)。工程预算/候选UID为本提案选择，不是这些文档保证的设备支持。
