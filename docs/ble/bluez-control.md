# D-22/24前置：固定BlueZ控制调用与owner连续性

2026-10-30。接续#105的纯GATT库，增加仅Linux显式`bluez-control` feature的四RPC控制部分。
默认库仍无运行依赖；本组件不连接任何总线、不导出GATT对象/通知、不启动服务或密码后端。
调用方必须提供已建立的Connection、实际观察到的唯一owner、明确支持的Hci0枚举。
不提供任意adapter/path/interface/method/options或well-known expected owner输入。

## 固定调用

| 方法 | 目标interface/path | 参数 |
|---|---|---|
| register_application | org.bluez.GattManager1 / /org/bluez/hci0 | 固定manifest ROOT、空options |
| unregister_application | 同上 | 同一ROOT |
| register_advertisement | org.bluez.LEAdvertisingManager1 / /org/bluez/hci0 | 固定ADVERTISEMENT_PATH、空options |
| unregister_advertisement | 同上 | 同一ADVERTISEMENT_PATH |

目标始终是捕获的unique name，不跟随`org.bluez`新owner；不Power/Pair/Connect/Trusted/
Discovery，不调用任意Root功能。总线/manager返回的错误只映射有限Error，不输出原始详情。
RPC出错或超时不表示未执行；本组件不重试/自动补偿，清理不得切到replacement owner。

## owner连续性与future

bind在首次GetNameOwner快照**之前**订阅固定broker的NameOwnerChanged，限定broker sender、
path/interface/member和arg0=org.bluez，队列最多32。调用前后检查待处理变更与当前owner。
只比较两次当前owner不足：同一unique连接可以释放后取回well-known name。
任何已观察到的变更/中断使上下文永久退休；再取回同名也不恢复。retire()可由可信宿主主动
无锁fence，进行普通清理应先于retire；retire不是外部清理已完成证明。

没有新owner监听后台任务，检查点读取实际D-Bus订阅队列。它保证控制调用边界的owner
连续性，不代替整个宿主空闲期间的NameOwnerChanged事件处理或密码/租约授权。
匹配规则排除其他sender伪造的同名signal；测试使用真实dbus-daemon验证，不手写假GetNameOwner。

方法返回已有Pending拥有型future，连接/owner/watch归该工作项所有。宿主沿用原Lifecycle
5秒调用预算，保留超时后的future，迟到成功只能用于原epoch清理，不能作为当前成功发布。
本控制部分不内置新重试或延长期限，也不将zbus内部超时冒称外部事务取消。

## 边界与接续

这是四RPC端口，不是完整GattBackendV1：同一Connection上的固定本地对象导出、回调sender/
device/epoch校验、通知以及运行驱动还待实现。真实BlueZ会读取这些对象；私有测试的外部
manager只核请求格式和响应，不把它称为实际蓝牙注册成功。
本端口不构造恢复能力、验证密文或批准Owner/UID；宿主仍须遵守缺SecurePort拒绝启动，
不得把此低层控制端口当作绕过前置的服务入口。没有生产默认调用/系统总线/蓝牙设备访问。

依赖仅复用本仓库既有锁定zbus5.19.0(MIT)、tokio1.53.2(MIT)、futures-lite2.6.1
(Apache-2.0 OR MIT)，tempfile3.27.0(MIT OR Apache-2.0)仅测试。
新增的是ble本地包的直接依赖关系，没有新第三方包/版本或下载；Cargo.lock只增加该包四条依赖。
默认正常依赖树仍只有ble自身，默认feature不变；Linux专用依赖不扩展到其他目标。

## 实测

新增11项，真实私有dbus-daemon、zbus连接/NameOwnerChanged/请求/回复，只有BlueZ manager端
是合成服务。覆盖四调用固定字段、缺owner/错名字/错真实unique name、前后owner换代、
瞬时释放/取回、伪sender信号、主动retire与迟到回复、保留同一超时future、后端拒绝、缺接口。
开发中初版只查快照，瞬时释放回归实际退出101：Ok != StaleContext；补监听后通过。
总包53项通过，完整验证/HEAD见[交付报告](../deliveries/member-d-bluez-control.md)。

所有总线均为测试私有UNIX地址，进程只结束自己创建的daemon；没有system/session总线
环境变量回退、主机BlueZ/HCI、Root/NSS/polkit/设备、A密码互操作或真正BLE验收。
不将普通UID下的真实bus unique name检验说成不同UID/生产权限验证。
