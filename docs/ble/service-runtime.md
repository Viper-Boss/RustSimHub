# D24：显式低权 BLE 运行层

2026-11-03后续：[固定A候选的实际安全会话/Core Wi-Fi闭环](../deliveries/member-d-secure-recovery-integration-20261103.md)已通过D非测试service和UnixSecurePort验证。运行层在原回调期限内排空当前通知再接下一输入，并随I/O唤醒推进原持有future，10ms tick仅作到期兜底；没有扩大队列/期限或移除来源检查。协议冻结、正式Core监听/Root安装及设备验收保持独立。

2026-11-01。用户批准 #53/#56/#132 的离线实施后新增 `rustsimhub-ble` 的 `service` feature 和 `--service` 入口。默认 feature 仍为空；Root/C 的模板未安装、启用或改变生产 UID。此运行层转交不透明字节，实际 A 安全会话和 Core 监听装配仍是独立前置。

`bridge-policy.json` 是 systemd 只读 credential，格式为严格 JSON 对象 `{ "format": 1, "core_uid": ..., "ble_uid": ..., "ble_gid": ... }`，上限8192字节，重复/未知字段、数组、Root/同角色 UID 拒绝。CLI只读普通单链接0400文件，属主限Root/本进程；不接受 socket、URL、CA、角色或授权字段来自 GATT。实际路径固定 `/run/rustsimhub/core-recovery/recovery.sock`。

Core端口在任何BlueZ注册之前探测：实际有效UID/GID和无额外组；2750 Core属主/BLE组目录、0660同属主/组普通单链接socket；连接前后socket inode不变；实际 `SO_PEERCRED` 对端必须是配置的非Root Core UID。探测不发送报文。此结果只证明内核来源，不能证明 A 握手或业务授权；Core必须仅在真实A端口可用时公布此独立端点。

单epoch/peer/5秒I/O保留原私有 v1长度帧，Auth和Command仅1/2标签，不解析业务内容。取消、超时、坏头、对端EOF和运行层围栏使本次link永久失效，直到匹配的显式disconnect；不透明重连、重放或降级均无实现。私有类型和实际连接产生回复，不接受网络传入的 `verified=true`。

运行层使用真实 `BluezControl`/`BluezBackend`/`Transport`。显式新backend模式允许首个真实BlueZ回调排队一个 `BindPeer`，主机必须绑定实际Device1 lease后才能确认；回调最终再次核私有lease，单独返回Accepted不能放行。旧默认backend仍在未绑定时拒绝。没有扩大D-Bus许可表或加入ObjectManager发现RPC。

复用snapshot之前已建立的原始PropertiesChanged/InterfacesRemoved订阅，每10ms有界排空；单个owned监测任务仅可关闭Core通道，不产生正授权。设备断开、adapter变化、移除、owner更换、Release、停止与错误均关闭当前Core通道；owner更换不清理新owner。运行层保留原有在途注册/I/O并驱动补偿，最多20秒排空加4秒关闭总线，不延长25秒unit停止预算。资源已排空但远端注销失败仍为Unknown；不把强制断开当RF关闭证明。Release已表明广播移除时不再重复注销该广播。

`firmware/system/ble/render_material.py --output <new-file.tar>` 输出0600可重复的审查归档，含现有固定材料、角色策略提案及精确hash；拒绝覆盖/符号链接，不提取、不执行、不创建账户、不安装。manifest持续 `install_permitted=false/production_ready=false/ids_reserved=false/default_enabled=false`。UID982/GID982/Core986仍是提案，由主线做真实NSS登记与安装。

验证分开记录：常规真实私有broker/Unix I/O的服务测试使用同UID **cfg(test)专用构造器**（仅该夹具允许runner继承组；生产构造始终拒绝同角色UID，实际额外组仍拒绝），只验证运行/关闭，不冒充不同角色来源证明；独立 `core_port_uid`（test=false）在只读、network=none容器切换实际UID1001/1002，覆盖12场景，包括篡改文件属主后实际对端仍1003的拒绝。原3角色GATT/kernel FD/严格policy试验另外复跑。外部BlueZ/Core报文端点是合成，运行层、分帧、socket、UID和broker是真实模块。无生产密码stub或ALSA/系统BlueZ/NM/设备访问。

原48M/16tasks/10%CPU等unit限额没有放大，但这次容器试验不等于该完整systemd/cgroup预算下验收。实际NSS/Root装配/ARM、A安全provider与Core正式监听、系统BlueZ许可、HCI/广播/RF、设备恢复均保留主线验证。正常Core和媒体不依赖此可选BLE服务启动；正常USB/Auth/业务状态没有改变。
