# X1C Core转发管理接口与实际鉴权接线

日期：2026-10-24。此阶段为源码与隔离验证，现场应用仍为7a60650/confirmed46，未安装本轮代码、启动转发或访问新目标。

## 已实现

`crates/core/src/forwarding.rs`将转发配置和队列管理接进实际`authenticated::handle`，使用现有长度前缀Unix IPC与签名登录会话。完整观察/短信/通话模式提供被动管理句柄；只读、仅鉴权等模式不提供该能力，health明确区分`forwarding_configuration_api`与仍为false的`forwarding_delivery_api`。Edge通用鉴权转送可沿用原协议，尚无新的网页表单。

| 已存在的协议命令 | 鉴权与行为 |
| --- | --- |
| `forwarding_status` | 实际Owner/SystemManage/SmsRead；读配置意图、代次和队列计数，不刷新敏感操作登录时限 |
| `forwarding_rules` | 同一Owner绑定；仅返回目标ID/类型、规则和凭据已隐藏标记，URL、query、chat ID、密钥均不回显 |
| `forwarding_configure` | 最近5分钟登录的实际Owner；expected_generation、唯一request_id及完整有界Policy，真实Store事件尾作为首次启用起点 |
| `forwarding_receipts` | Owner；规范十进制游标与每页1～32条，不包含短信正文或凭据 |
| `forwarding_prune` | 最近登录Owner及精确代次；仅清理指定序号以内的confirmed/failed/cancelled，不删除Unknown或在途记录 |

修改使用原认证协议2；命令token不放进命令行，CLI仍使用`--request-auth SOCKET`从有界标准输入读JSON。wire先拒绝非法代次/UUID/游标/页大小/未知字段，Core再解析并验证有限Policy。配对Client即使被授予SystemManage/SmsRead/ClientsManage也不能操作这些Owner接口。

Core在阻塞工作任务内依次取得Auth和管理锁，唯一Queue写入者仍受目录/文件权限、NOFOLLOW及flock约束。启动只保留句柄，不创建目录、不读取短信或排队、不启动定时循环。默认关闭时只查状态/规则/回执不创建`STATE_DIR/forwarding`，首次有效Owner配置才创建私有目录和独立outbox。其他Owner/realm/Owner代次不能接管旧队列，包括重开文件后的情形；主库schema仍为6。

停用先持久禁用和递增代次，未提交任务cancelled，在途任务Unknown，迟到旧代次确认不能改成成功。此处没有Root服务主管，**`enabled`仍只是配置意图**：启用显示`awaiting_service`，停用显示`disabled_unverified`，`process_state_verified=false`。尚不能向用户显示已运行或已经停止全部进程。Root及worker接线前不交付为可用转发服务。

## 联调发现与修正

原Queue将“当下Store事件尾”混入配置请求幂等摘要。保存成功但响应丢失后，若又收到新短信，重复同一request_id会得到Conflict。摘要现在只绑定Owner/realm/expected_generation/配置意图；第一次实际落盘的事件起点保留，重查不会重采样或跳过新消息。修改内容复用request_id仍拒绝，旧代次请求仍不能覆盖更新配置。

此格式尚未部署，设备没有旧版本转发outbox需要迁移。最初新增测试的游标类型比较编译失败，以及两条合成短信正文相同而触发既有重复候选过滤的失败日志均保留；测试改为真正不同的新消息，未放宽重复短信保护。

## 验证与边界

8项新增Core测试通过真实UnixStream和正式handler，使用真实签名Owner/Auth/Store/Queue；没有伪造Owner布尔开关：

- 默认查询无文件及配置模式缺席拒绝；不以本地意图宣称无进程。
- 首次保存、真实新短信到达、同请求重查，起点不变且新消息仍可唯一入队；凭据/URL不回显。
- 停用时在途Unknown、未提交cancelled、有限清理保留Unknown、重开后状态与目标保持。
- 实际签名配对Client和无效会话，五种管理操作全部拒绝且不创建目录。
- 错误配置/旧代次/软链接目录拒绝，无外部目录写入。
- 实际Owner代次变化，新登录和文件重开仍不接管旧outbox。
- SQLite触发器模拟落盘失败，不回假成功或推进代次；解除故障后同请求安全完成。
- wire溢出/非规范游标/未知字段/缺字段拒绝。

完整393项Rust、全工作区严格Clippy、生产Core无默认特征检查，以及Core/Edge程序和转发库ARM64构建通过，三份制品均核对AArch64与SHA-256；本地日志`.private/forwarder-core-workspace-20261024-first.log`及exit=0。制品未签名、未交付，不能代替正常同源签名安装包。精确提交CI结果随后记录。转发实际worker IPC、绑定出口的Connector、Root启停/开机策略、网页总开关/规则表单与合成测试、正常签名交付、关闭/重启无PID验收仍未完成。X1单列，原P0–P7仍80%；当前现场短信68条、普通数据关闭，一次10010/CXYE发送授权已经消费，不得重发。
