# 独立配网前置检查

2026-10-24 [P4K AP/STA试连返回后端](VALIDATION-SETUP-AP-STATION-TRIAL-P4K.md)源码完成：同一wlan0的有限STA试连、精确内层checkpoint恢复原AP、凭据/配置集合核对及固定DHCP回复隔离；原Root路径默认不变。最终332项Rust（热点44项）、严格Clippy、生产检查、三个真实材料CLI及三程序/库ARM64通过，NAS私有网络命名空间六条规则加载/排他/删除与参考核对通过，未做报文或无线实测。尚未接Root执行循环/Core/网页或保存，未部署/未访问棒子，不开热点/切网，整体79.5%、10010仍44，当前无需用户操作。

2026-10-24 [P4J首次窗口/有限AP租约](VALIDATION-SETUP-AP-BOOTSTRAP-P4J.md)源码完成：真实LocalSetup设备证明和新鲜通话空闲接入唯一Start/有限心跳，签名认领保留原截止时间；恢复仅取消，Unknown持锁至Root精确Closed。320项Rust、严格Clippy、生产检查、三个真实CLI及三程序/库ARM64通过。仅只读确认现场43组服务正常/无通话/普通数据关闭，未部署本轮代码或开热点；无SIM首配、AP到STA、重启配置恢复/网页仍缺。用户已醒，已请求只读Wi-Fi列表/布局验收，暂不试连接；整体79.5%、10010仍44。

2026-10-24：[P4I真实Core绑定和首次代码私有导出](VALIDATION-SETUP-AP-BINDING-P4I.md)源码及三项真实CLI联调通过；尚未将未认领窗口接到自动AP租约/切网，未部署。

2026-10-24：[P4H材料/证书与可信导出](VALIDATION-SETUP-AP-MATERIAL-P4H.md)源码和隔离CLI联调通过，候选模板未进入overlay，实际首次窗口/切网/联合安装与AP实机仍未完成；当前不切换保存Wi-Fi。

2026-10-24。此文是只读能力检查及后续功能顺序，不是热点功能已实现或实机通过。整体仍79.5%，普通数据关闭，关机调查继续搁置。

## 已核对

当前设备的NetworkManager无线能力声明包含AP模式。使用D-Bus只读属性读取，未请求扫描、未创建连接、未切换SSID、未开启热点，也没有修改普通蜂窝数据。

依据是`Device.Wireless.WirelessCapabilities`的AP标志位0x40；属性及位定义分别见[官方D-Bus文档](https://networkmanager.dev/docs/api/latest/gdbus-org.freedesktop.NetworkManager.Device.Wireless.html)和[官方类型定义](https://networkmanager.dev/docs/api/latest/nm-dbus-types.html)。这是设备/驱动的能力声明，尚未证明热点能实际启动、手机能连接，或AP和路由器Wi-Fi能同时运行。

第一份检查确认设备没有iw工具，没有据此推断不支持热点；第二份通过既有NetworkManager D-Bus取得能力声明，没有为了检查而安装软件。原始版本、设备路径和能力值只存私有记录：`.private/wifi-capabilities-20261024-first.json`、`second.json`。

## 后续功能顺序

1. 已有所有者的Wi-Fi选择/配置页面：[P4D](VALIDATION-WIFI-CATALOGUE-P4D.md)已交付185ec6e/confirmed43，提供只读缓存列表、同区登录、显式选网和手输隐藏SSID；复用当前受限试连接、40秒未确认回退和最近登录要求。真实Owner读取和网页/切网尚未验收，不等同主动扫描。
2. [P4G Root执行程序](VALIDATION-SETUP-AP-EXECUTOR-P4G.md)及[P4F Core控制侧](VALIDATION-SETUP-AP-CONTROL-P4F.md)源码已连接保护文件租约、热点事务和真实TLS就绪检查；材料证书安装/首次窗口/切网/网页及实机仍缺，默认禁用且不部署；[P4E事务库](VALIDATION-SETUP-AP-TRANSACTION-P4E.md)已实现并通过Rust/ARM和隔离规则加载，尚未接授权/证书/网页或部署。继续准备无保存SSID时的受保护管理热点：每台独立凭据、明确初始化窗口、只开放配置需要的设备端服务；不默认共享蜂窝或NAS网络。
3. 把设备身份、公钥及可信HTTPS交付和首次Owner流程接起来。保留证书验证，不用开放热点或关闭HTTPS检查代替首次信任。
4. 再验证热点切到路由器Wi-Fi、错密码、超时回退、手机重新发现设备和保存后冷启动。

当前维护连接经过保存Wi-Fi，NAS侧USB维护IPv4尚未激活。首次热点切换需要先建立备用维护入口、备份、设备端有界回退，并在用户方便到现场时验证；用户休息期间只做代码与隔离测试，不直接切换无线模式。

保持原来的电话/短信单一控制者和网络变更互斥；不写基带/NV/校准区，不用此任务推进关机或内核实验。真实证书信任、配网和手机听音验收后续分别记录，能力声明不增加验收分数。
