# X1G：Core有限服务协调与真实进程状态

2026-10-25。源码、本机真实Core→D-Bus→polkit→PID1链路、424项Rust、严格检查和四份生产ARM64通过。网页、安装和棒子验收仍缺；不是已部署功能。原P0–P7仍80%，新增X1单列。

## 行为与边界

- Core默认保持被动。新CLI `--serve-device-forwarding-managed`在已有设备运行参数后显式给出独立worker UID，才接有限服务协调。原`--serve-device`及`--serve-device-forwarding`不启动管理任务；新模式自身也不在开机时连接DBus、轮询或运行转发进程。
- 最近登录Owner成功配置/修改开关后，先持久化意图，再请求固定`rustsimhub-forwarder-reconcile.service`。Owner查询状态只请求固定`rustsimhub-forwarder-status.service`。规则、回执、清理仍用原权限/队列接口；浏览器不能传服务名、命令、路径或job模式。
- 固定系统bus地址，忽略环境bus地址；固定`StartUnit`的`fail`模式，不请求交互授权。在受理前订阅`JobRemoved`，核对实际返回job路径/数字ID/单元名、完成结果及systemd唯一owner。未收到StartUnit回复不自动重发；收到路径本身不算完成。
- 管理请求最多等8秒，保留现有网页/Core 15秒时限。成功受理但未确认结束时，返回`configuration_applied:true`、同一`request_id`、`service_action:unconfirmed`及未核实状态。Root job由PID1按原有限时限收尾；没有Core后台转发循环或常驻管理器。
- 只有实际job成功后才读Root回执：固定路径、Root目录、0640单链接普通文件、NOFOLLOW/NONBLOCK、8192字节上限。还必须匹配当前boot、Core实例、Owner绑定摘要、配置代次、启用意图，采样时间不早于请求且不过期，运行/关闭状态与PID字段相符。
- 异步前后复核真实Owner会话与上下文；不持有Auth/Management锁等待Root，允许Root回调原Core IPC。单个Core内在途管理请求拒绝并发写入。超时后若旧Root job仍在收尾，新操作可能继续未确认；需要查询及显式应用同一请求，不能让页面自动反复开关。
- 基础Queue状态仍只是意图；被动模式继续`process_state_verified:false`。管理模式只有新鲜、匹配且实际核验的Root报告才能变true。失败保留已落盘意图，不能读旧报告假报成功，也不回显目标凭据。

## 验证

完整workspace **424项Rust通过**（新增5项），严格Clippy、生产feature检查通过。真实Auth/wire和私有D-Bus场景覆盖：快速任务在方法回复前已结束、明确失败且存在可用报告、丢失StartUnit回复、丢失完成信号、错误job路径、systemd owner变化、过期/错boot/错Owner/错Core/错代次/错PID报告、并发配置变化、普通配对客户端拒绝及并发写拒绝。超时只受理一次，保持配置代次和请求ID。Root回执的可写权限、错误属主、硬链接、符号链接拒绝通过。

`tools/test-forwarding-supervisor.py --managed`额外通过实际链路：

1. 使用制品属主对应的既有非Root NSS身份运行真实Core，worker使用另一个既有低权限身份；不创建账户。仅临时允许该Core身份启动两个唯一命名、Root所有的测试管理单元，可能激活共享polkit基础服务。
2. 经真实Owner IPC关闭/开启，由Core实际请求PID1；在任何手动Root复查前，响应已取得真实`disabled`/`running`进程证明。
3. 两条合成通知分别实际DNS/TLS并confirmed，各HTTP一次；重复Root协调不重启健康worker；保留X1F凭据、组/能力、条件boot及整组退出核验。
4. 关闭通过Core实际控制，故意忽略SIGTERM的额外自有进程被SIGKILL、由夹具父进程回收，worker与凭据消失。
5. finally先撤销临时polkit规则，再移除唯一测试单元、进程和文件。共享polkit基础服务不由测试停止；没有部署生产规则、访问棒子、实际发短信或用蜂窝/外网。

旧四UID IPC和独立worker DNS/TLS/停止场景兼容通过。生产Core、Edge、forwarder、库核对AArch64及摘要；QEMU执行Core/worker元数据，schema6/协议4、无开发夹具。未据此声称ARM64完整systemd或棒子冷启动通过。源码制品未签名或部署。

私有索引：`.private/forwarding-core-service-checkpoint-20261025-first.json`。初期两次测试断言失败保留：无效token在wire语法层被拒绝、浏览器通道将Busy统一映射denied；未修改生产权限或扩大重试来让测试通过。公开CI按精确SHA另存，不预先宣称待执行CI成功。

## 下一步

简单Owner网页开关、隐藏凭据的目标/规则编辑、回执与合成测试；安装前账户/数字ID冲突检查、同源签名组和一致性outbox备份；棒子实际网络/故障/关闭/冷启动。Email等后续发送器、历史显式补发和预提交失败有限处理仍未完成，Unknown仍不自动重发。需在安装和设备验收后才能开放真实短信转发。

参考：[systemd的StartUnit/job/JobRemoved接口](https://www.freedesktop.org/software/systemd/man/latest/org.freedesktop.systemd1.html)、[原服务管理与真实关闭证据](VALIDATION-FORWARDING-SUPERVISOR-X1F.md)。
