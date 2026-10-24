# X1F：有限服务管理与真实关闭检查

2026-10-25。状态：源码、419项Rust、严格Clippy/生产feature检查、本机真实systemd及ARM64构建/元数据验证通过。尚未接Core服务协调或网页，未安装生产单元、未签名/部署棒子，也未验证设备冷启动。原P0–P7仍80%，新增X1单列；X1不是完整已交付功能。

## 实现

- `supervisor.rs`：只开放固定生产路径、固定单元的Root reconcile/status/retire，Root不常驻。Owner开关仍先由Core落盘；Root管理已有策略的实际服务，不把浏览器参数转换成shell、任意单元或任意UID。
- 独立worker由PID1直接以专用非Root身份运行，CapabilityBoundingSet与AmbientCapabilities均为空，NoNewPrivileges开启。没有Root启动器或自定义身份切换入口。Root有限管理单元也使用空能力集合。
- Root从同boot且30秒内的新鲜Root网络报告选择**明确配置的wlan0或usb0**、类型、地址及DNS，拒绝歧义/断开/过期/其他类型。实际socket仍使用X1E绑定设备、ifindex和源IP；没有系统DNS、环境代理或蜂窝回退。
- Root将有界配置写入0600运行时文件，PID1通过LoadCredential交付每个服务的私有只读副本。令牌不进argv、环境变量或日志。worker拒绝可写credential、符号链接、硬链接和不安全元数据；Root发布缓冲使用Zeroizing。
- Root开机标记包含实际Owner绑定摘要及策略代次。OFF先移除标记并同步，再停整个单元/cgroup及有限触发源、移除运行时配置，按实际Core状态清理匹配能力。Root报告绑定boot/Core实例/Owner/代次/当前能力、PID及启动tick，区分意图和实际进程证明。
- Core协议增加Root专属条件Retire；旧清理只匹配原能力摘要，不能撤销替换后的能力。受理前Busy有限重查，丢失Grant/Claim回复不自动重发。开机只读State在Core socket尚未就绪时有界等待。
- `firmware/system/forwarding/`为安装候选，特意在自动overlay之外：worker/条件boot/有限管理单元、固定polkit、接口选择、专用账户及目录候选。默认无enabled标记、无转发timer/socket/path。

## 实际验证范围

完整Linux：`cargo +1.98.0 test --workspace --all-features --locked --offline`，**419项通过**，新增3项Rust测试（Root迟到清理竞争、Root网络选择、实际systemctl状态解析）。严格workspace Clippy和Core/forwarder生产feature检查通过。

`tools/test-forwarding-supervisor.py`在只有lo的临时网络命名空间、唯一临时systemd单元及合成Core数据中实际通过：

| 场景 | 直接证据 |
| --- | --- |
| 默认OFF启动 | worker主PID为0、无标记/能力；实际boot oneshot条件不满足，ExecMainPID为0 |
| 启用 | PID1使用现有低权限NSS测试身份创建worker；真实UID/GID/组、五类能力为空及NoNewPrivileges=1，单元/cgroup/PID一致 |
| DNS/TLS/回执 | 两个独立合成通知，各实际HTTPS一次，均在真实Core中confirmed；不分享完整合成正文 |
| 重复启用/条件启动 | 当前worker PID保持，未重复启动或发HTTP |
| credential拒绝 | 可写、硬链接、符号链接三种情况均在连接Core之前拒绝；有效只读credential正对照确实到达专用测试IPC入口 |
| 额外进程 | 专门创建并放入本测试cgroup的额外进程，使状态核验拒绝假报正常 |
| 整组关闭 | 故意忽略SIGTERM的自有子进程由systemd强制SIGKILL，等待并回收；worker消失，Root运行时配置和服务credential目录消失，管理oneshot均无主PID |
| OFF后再请求启动 | 无新worker、无新能力；条件boot守卫继续跳过 |

测试使用既有NSS nobody作为一次性夹具身份，不创建/修改系统账户；生产候选要求独立锁定账户，不能使用nobody。只停止/移除唯一fixture名称的临时单元，不触碰已有服务。测试守卫不允许杀掉其他同UID进程来使检查通过。实际cgroup验证为v2；源码中的v1 systemd挂载路径尚未实测。

兼容性复核：旧实际四UID Core IPC验收与四组独立worker DNS/TLS/停止场景全部通过；关闭后重开保持禁用、Unknown保留、失去确认不重投、错误出口/CA零HTTP，均只使用本机合成数据。

上述是本机条件启动、服务/进程及凭据验证，**不是设备物理冷启动、安装脚本、实际Owner网页或真实短信转发验收**。Root报告独立于基础Queue/Core状态；Core/网页尚未消费它，基础查询仍`process_state_verified:false`。

生产Core、Edge、forwarder和库四份制品均核对AArch64与SHA-256。QEMU执行ARM64 Core/forwarder元数据，schema6/协议4、无开发夹具；未据元数据推断ARM64完整systemd/HTTP或设备运行通过。制品未签名或安装。

## 失败记录与权限边界

早期记录见[验收前WIP](VALIDATION-FORWARDING-SUPERVISOR-X1F-WIP.md)，原失败不覆盖：实际空闲Job为空字符串的兼容、Root启动器降权失败、数字测试UID无法被PID1解析。增加Root环境能力的候选编辑被自动审批拒绝，未执行；该架构已删除，实际通过的PID1方案没有增加环境能力或移除隔离。审批服务的额度错误恢复后，所有新运行均经过正常审批。

私有完整索引`.private/forwarding-supervisor-checkpoint-20261025-second.json`，真实服务最终日志`forwarding-supervisor-live-20261025-resume-third.log`。测试/源码摘要、原失败与本机网络隔离边界单独保存；公开CI结果按精确提交SHA核对，不预先宣称待执行的CI成功。

## 下一步

1. Core显式启用有限管理协调：Owner成功配置后触发固定Root动作、等真实job结束，再核对新鲜且绑定当前Owner/代次/Core/boot的Root回执；失败时不能读旧报告假报成功。
2. 简单Owner网页总开关、有限规则/隐藏凭据/回执；独立账户与安装前冲突检查、outbox一致性备份、同源签名组。
3. 棒子上实际服务、网卡变化/断网/异常退出、关闭和完整冷启动；再开放明确授权的蜂窝出口。预提交失败的有限处理策略仍缺，Unknown不自动重发。

参考：[私有credential及目录语义](https://www.freedesktop.org/software/systemd/man/latest/systemd.exec.html#Credentials)、[整组退出语义](https://www.freedesktop.org/software/systemd/man/latest/systemd.kill.html)、[systemd授权详细字段源码](https://github.com/systemd/systemd/blob/v257/src/core/dbus-util.c)。
