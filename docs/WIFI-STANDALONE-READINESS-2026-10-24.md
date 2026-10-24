# 独立配网前置检查

2026-10-24。此文是只读能力检查及后续功能顺序，不是热点功能已实现或实机通过。整体仍79.5%，普通数据关闭，关机调查继续搁置。

## 已核对

当前设备的NetworkManager无线能力声明包含AP模式。使用D-Bus只读属性读取，未请求扫描、未创建连接、未切换SSID、未开启热点，也没有修改普通蜂窝数据。

依据是`Device.Wireless.WirelessCapabilities`的AP标志位0x40；属性及位定义分别见[官方D-Bus文档](https://networkmanager.dev/docs/api/latest/gdbus-org.freedesktop.NetworkManager.Device.Wireless.html)和[官方类型定义](https://networkmanager.dev/docs/api/latest/nm-dbus-types.html)。这是设备/驱动的能力声明，尚未证明热点能实际启动、手机能连接，或AP和路由器Wi-Fi能同时运行。

第一份检查确认设备没有iw工具，没有据此推断不支持热点；第二份通过既有NetworkManager D-Bus取得能力声明，没有为了检查而安装软件。原始版本、设备路径和能力值只存私有记录：`.private/wifi-capabilities-20261024-first.json`、`second.json`。

## 后续功能顺序

1. 已有所有者的Wi-Fi选择/配置页面：[P4D](VALIDATION-WIFI-CATALOGUE-P4D.md)已交付185ec6e/confirmed43，提供只读缓存列表、同区登录、显式选网和手输隐藏SSID；复用当前受限试连接、40秒未确认回退和最近登录要求。真实Owner读取和网页/切网尚未验收，不等同主动扫描。
2. [P4F Core控制侧](VALIDATION-SETUP-AP-CONTROL-P4F.md)和保护文件租约已实现，真实Root消费执行/证书/首次窗口仍缺，默认禁用且不部署；[P4E事务库](VALIDATION-SETUP-AP-TRANSACTION-P4E.md)已实现并通过Rust/ARM和隔离规则加载，尚未接授权/证书/网页或部署。继续准备无保存SSID时的受保护管理热点：每台独立凭据、明确初始化窗口、只开放配置需要的设备端服务；不默认共享蜂窝或NAS网络。
3. 把设备身份、公钥及可信HTTPS交付和首次Owner流程接起来。保留证书验证，不用开放热点或关闭HTTPS检查代替首次信任。
4. 再验证热点切到路由器Wi-Fi、错密码、超时回退、手机重新发现设备和保存后冷启动。

当前维护连接经过保存Wi-Fi，NAS侧USB维护IPv4尚未激活。首次热点切换需要先建立备用维护入口、备份、设备端有界回退，并在用户方便到现场时验证；用户休息期间只做代码与隔离测试，不直接切换无线模式。

保持原来的电话/短信单一控制者和网络变更互斥；不写基带/NV/校准区，不用此任务推进关机或内核实验。真实证书信任、配网和手机听音验收后续分别记录，能力声明不增加验收分数。
