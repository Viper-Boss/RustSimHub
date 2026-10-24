# X1D 转发执行通道与主开关

日期：2026-10-24。源码与隔离验证，未访问棒子、安装服务或转发真实短信。

## 已实现

- 显式`--serve-device-forwarding`沿用原设备参数，末尾增加WORKER_UID；Core、Edge、worker必须是三个不同的非Root UID。原默认设备入口不接纳新Root/worker协议。
- 复用现有Core socket，以实际SO_PEERCRED分流：Edge只走原浏览器鉴权协议2，UID0只走有限Root协议4，指定worker只走协议4任务命令，其他身份拒绝。没有新增转发专用socket或自动启动进程。
- Root仅可State/Grant/Revoke，不能修改Owner策略；worker仅可Acquire/Check/Finish。每连接一笔请求、64KiB帧、客户端总截止5秒，核对实际Core UID及响应协议/ID，不自动重试。
- Root仅能为已持久启用、当前实际Owner/realm/代次授予30秒能力。每次领取及连接前/TLS前/HTTP提交前重新核对实际Auth、业务库、SIM和精确Claim；检查阶段必须有序。
- 每次Check带实际Claim的SHA-256，覆盖URL、凭据、正文、ID、代次和线路；Rust客户端从将要发送的Claim重新计算，不能仅复用领取时的摘要提示。内容被改动会拒绝并保留Unknown。
- 一次只保留一个在途Claim，丢失领取回执不能再次领取同一payload重试。已发提交许可却丢失ACK时，即便worker报告未提交，Core仍保守保存Unknown。
- Core重启后从私有outbox读取绑定，先核对实际Auth再恢复；不初始化空库或接管其他Owner。旧在途转Unknown，旧内存能力失效。关闭时没有新增Core轮询或扫描。
- 新Owner命令`set_forwarding_enabled`只需token、expected_generation、唯一request_id和enabled，要求最近5分钟登录。凭据与目标留在Core，不回显或要求重新输入。无目标时拒绝首次开启；停用递增代次、取消未提交任务并保留Unknown；重新开启默认只处理之后的新消息，同请求重查不跳过新消息或撤销当前能力。

状态机：`crates/core/src/forwarding/ipc.rs`；共享协议、实际异步客户端及Fence：`crates/forwarder/src/worker_ipc.rs`。主开关接入正式Core鉴权handler，仍使用协议2。

## 验证

完整404项Rust、全工作区严格Clippy、生产Core无默认特征检查、Core/Edge及转发库ARM64构建通过；相关Core共19项、转发库40项。覆盖真实签名Owner/队列、单个在途与唯一终态、撤权/换卡/关闭/过期/重开不重派、实际Unix客户端三阶段、修改URL/正文/Bearer/线路/代次拒绝、丢ACK保留Unknown、总开关保存密钥及重新开启起点。

`tools/test-forwarding-worker-ipc.py`使用本机一次性临时目录和真实不同UID进程：非Root Core、Root、Edge、worker及可访问socket目录的其他UID分别连接，确认权限来自实际SO_PEERCRED；以真实Owner主开关关闭后停止并重开测试Core，代次/关闭/Unknown保持，旧能力拒绝。所有Core和短期客户端都等待退出。没有安装真实worker服务，也没有HTTP递送；测试中的Confirmed是合成回执输入，不能替代真实接收方确认。

最初Clippy风格告警已修正并保留失败日志。本机sudo -n没有免密权限，未启动多UID测试；随后通过明确的WSL Root入口仅运行该临时目录测试并退出0。主开关及内容绑定更新后再次通过真实Root测试。CI新增同一隔离步骤，精确提交结果另记。

最终本地日志`.private/forwarding-worker-ipc-20261024-fourth.log`与exit=0；真实Root测试`.private/forwarding-worker-ipc-root-20261024-second.log`与exit=0。三份ARM制品核对AArch64与哈希，未签名/安装。另在现有QEMU直接运行实际ARM生产程序：build-info确认development_fixtures=false，开发fixture命令拒绝，新服务模式在创建文件前拒绝Root worker UID，有限客户端命令实际识别且只尝试临时目录中不存在的Unix路径。对应报告`forwarding-worker-ipc-arm-runtime-20261024-first.json`；优化器可将参数比较编译为机器指令，不能以二进制内是否存在完整参数字符串来判定功能。并行README文档已逐字节核对后正常快进至3dcdbbf，另一个任务的未提交接手记录保留，没有reset或stash。

## 边界与下一步

enabled仍只是配置意图，能力撤销不是进程退出证明；process_state_verified仍为false。尚缺真实绑定出口Connector、独立worker程序/循环、Root固定服务启停与开机策略、PID/触发器/冷启动验收、简单网页总开关/规则/合成测试、outbox一致性备份和签名联合安装。旧delivery_api仍为false，不宣称已交付转发功能。

原AP/STA、NAS及其他P0–P7缺口继续保留；原整体80%，X1等扩展单列。上次核实现场7a60650/confirmed46、68条短信，本轮未读写设备；单条10010/CXYE授权已消费，没有新的短信发送授权。
