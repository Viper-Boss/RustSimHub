# P4K 热点到家庭Wi-Fi的试连与返回后端

精确源码01ea674的[GitHub CI40065960462](https://github.com/Viper-Boss/RustSimHub/actions/runs/40065960462)两个任务全部成功，含真实材料CLI步骤；完整证据station-trial-ci-20261024-first.json。候选未部署。

最终本地332项工作区Rust（setup-ap 44项）、全工作区严格Clippy、Core生产检查、三个实际材料CLI和三程序/库ARM64通过。首次候选改名清理测试失败保留，修正及最终追加授权/隔离校验后的第二份全量日志为权威结果。精确提交CI另记，未部署。

2026-10-24。本轮增加独立的AP→STA→原AP试连后端，尚未接到Root执行循环、Core命令和网页，也未部署。它只测试WPA2关联和可用IPv4地址，不保存家庭Wi-Fi、不验证互联网可达，不宣称完整配网完成。整体保持79.5%，棒子仍为已交付185ec6e/confirmed43。

## 交互方向

单无线设备切换到家庭Wi-Fi时，原配置热点会暂时消失。拟采用“在热点提交试连→设备试连→返回同一个热点显示结果→用户确认保存”的流程，使错误口令或试连超时后仍能回到原页面。最终保存、离开热点并连接已保存网络属于下一步，尚未实现。已部署的旧Wi-Fi试连接仍要求USB备用链路，不能直接移除该门禁后用于热点配置。

## 已实现源码

- `crates/setup-ap/src/station_trial.rs`：限40秒的试连器；实际调用者必须持有原Core网络互斥并提供真实Root租约/Owner gate。初始授权、固定隔离、试连过程及返回后的授权均核验。取消、超时或调用方控制通道关闭后继续等待精确清理。结果区分“试连成功且返回AP”“试连失败且返回AP”和Unknown；Unknown不释放隔离或重试。
- `crates/setup-ap/src/nm/station.rs`：复用同一NetworkManager连接和唯一owner，初始AP必须已Ready。只在原属己AP checkpoint上创建45秒重叠checkpoint，只有本次内部checkpoint可回滚或销毁。STA为固定wlan0/WPA2-RSN-CCMP/IPv4 DHCP/禁IPv6的RAM配置，autoconnect=false，绑定dbus-client；无保存或APN/WWAN写入。SSID和密码沿用现有Core个人网络输入规则，不接受外部设置字典、设备路径或UUID。
- `nm.rs`：新增仅能在preflight前开启的保留AP内存配置选项，使切STA时原AP配置仍在RAM。原Root worker没有启用该选项，默认仍为volatile。普通AP配置核验补齐band、ap-isolation、pairwise/group、psk-flags及may-fail，返回时不能只核对SSID和地址。
- `guard.rs`和`setup-guard-trial-v1.json`：新的显式试连隔离仅额外允许IPv4 DHCP服务端67→客户端68的回复，保留AP地址HTTPS、其他wlan0输入丢弃及双向禁止转发。原AP-only guard保持原规则，不能冒充试连隔离。不新增站点HTTPS、SSH、DNS代理、普通已建立连接放行或蜂窝回退。

NetworkManager官方接口说明memory配置仅在内存保存、volatile在断开后销毁；此处保留AP内存配置是为了可靠返回同一AP，而不是写永久热点配置。重叠checkpoint不会取代原checkpoint；原checkpoint若回滚会销毁较新的重叠checkpoint，因此丢失/过期结果必须通过实际AP状态核对。[官方激活接口](https://networkmanager.pages.freedesktop.org/NetworkManager/NetworkManager/gdbus-org.freedesktop.NetworkManager.html)，[官方checkpoint标志](https://networkmanager.dev/docs/api/latest/nm-dbus-types)。

## 精确回退及失败边界

先核对原AP、原checkpoint和现有配置集合；记录本次随机UUID与凭据指纹后才请求变更。CheckpointCreate或激活回复丢失都不重放。未知checkpoint不会按猜测销毁。恢复只回滚已记录的本次checkpoint，再核对原AP模式、地址、加密和客户端隔离；仅删除本次精确匹配的STA候选，并要求配置集合回到原集合。外部owner变化、候选改名、其他新配置、回退失败或原AP不一致均保留Unknown。

首次测试发现候选UUID被改后可能漏清理并误报恢复，已补试连前后配置集合校验；失败报告保留。即使返回AP期间授权过期，也不能把先前成功结果当作当前有效确认。返回AP网络状态不代替AP HTTPS实测，调用者还需执行既有真实证书/Origin/身份及AP地址探针。

## 验证与边界

最终本地检查和精确提交CI结果记录在本页后续补充及私有checkpoint中，不能用此前P4J的CI代替。新增p2p D-Bus夹具覆盖同一AP恢复、成功/丢激活回复、自动消费checkpoint、丢checkpoint回复不猜测、错误子网、回退失败、错误凭据句柄、owner/候选变化，以及原AP隔离/加密被改。独立试连器测试授权/隔离拒绝、取消/失联清理、超时、清理失败和恢复期间授权过期；未增加保存、Delete全部配置或启用蜂窝的方法。

NAS独立网络命名空间实际加载了六条候选规则；排他创建拒绝重复、重复后内容保持、精确删除及与仓库JSON参考逐项一致均通过。没有改NAS宿主防火墙或棒子无线，没有做报文转发或真实DHCP测试。证据`station-guard-kernel-isolation-20261024-first.json`。

没有访问棒子、升级应用、开热点、切网、拨号、发送短信、开启普通数据或改证书信任。本轮仅为NAS私有网络命名空间的隔离规则检查接入NAS。原业务和用户刚验收的Wi-Fi列表/布局保持，累计10010仍44。

## 直接接续

1. 把试连器接入Root现有AP事务循环：显式保留AP RAM配置和新隔离模式；试连时暂停AP-only Ready探测，但继续原Root租约/截止时间。保留至少60秒原窗口余额，内层45秒timer不能延长外层窗口；取消后必须等待清理，不能直接drop执行future。
2. 增加Core→Root受保护的有限试连请求/结果，绑定原AP session、当前真实Owner/generation及独立一次性试连ID；落盘防重在变更前，口令不进入状态/日志/intent。Core保留原网络锁，不能另启现有USB Wi-Fi事务；Unknown不自动重试或按猜测归档。
3. 返回AP后验证真实AP HTTPS，再显示有界结果；Owner明确确认后才能保存和结束AP。保存过程需持久确认意图、实际配置回读、未知结果核对和最终STA激活/启动行为，不能用本轮内存试连冒充保存。
4. 接首次网页、无SIM空闲证明、认领后未联网重启的配置恢复和联合签名安装。备用维护入口就绪后再安排真实热点/错误口令/保存及冷启动验收；当前不需用户操作。
