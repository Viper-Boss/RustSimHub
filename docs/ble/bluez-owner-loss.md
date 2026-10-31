# 空闲期间的 BlueZ owner 丢失入口

2026-10-30，`bluez-control` 显式 feature。
此前四 RPC/Release 入口会在调用前后检查 owner；空闲时还需要宿主驱动事件。
新增 `BluezControl::wait_owner_loss() -> Pending<()>`，由宿主与原 Lifecycle
共同 poll。它只能返回有限错误，永远不会把 owner 丢失报告成业务成功。

## 所有权与唤醒

一个绑定同时最多一个已 poll 的等待者，第二个返回 Busy；未 poll 的 future
没有订阅或占槽副作用。取消等待释放槽位，保留原控制绑定及其从 bind 开始的
NameOwnerChanged 历史；重建等待仍不能越过同 unique 释放再取名的间隙。
等待 future 自己拥有绑定，宿主必须显式 retire 后排空/丢弃等待 future，
不能以丢弃 BluezControl 外壳代替退休在途操作。

等待端使用第二个容量 32 的实际 MessageStream 接收端，规则仍只匹配 broker
sender/path/interface/member 及 arg0=org.bluez。zbus 对同 rule 使用独立的广播
receiver，不与 RPC 检查器争抢消息，也不持有检查器 stream Mutex 跨空闲 await。
订阅后再次执行原检查，故订阅前的 owner gap 由原接收端捕获。
真实 owner 变化/流错误/流关闭立即单调退休绑定；显式 retire 或其他入口检测到
退休也通过 Notify 唤醒空闲等待。Notify 在读 live 前 enable，避免检查与等待
之间丢失唤醒。没有 spawn、后台任务、自动重启或跟随替换 owner。

它是可长期 pending 的事件等待，不是 5 秒注册 RPC。宿主应同时驱动 shutdown/
生命周期：看到错误后停止接受新回调，清理本 epoch 安全会话，保留未完成远程
调用及 Unknown 证据；不向新 owner 重放注册/取消。该 API 本身不会停止 RF、
撤销 Core capability、关闭安全会话或证明 quiescent。调用方遗漏 poll 就不会
获得空闲事件；当前尚无设备端 daemon 或完整 GattBackend 接线。

## 验证范围

8 项新增真实私有总线测试：无后续请求也发现 idle owner 丢失；先丢失再重取
同名而后开始等待；空闲等待期间实际 RPC 不阻塞；并发等待 Busy 与 retire 唤醒；
取消后重建；外来 sender 信号不触发；未 poll 不占槽；实际连接关闭有限失败。
其余四 RPC、广播对象及纯库测试一并回归。没有主机系统总线、BlueZ/HCI/RF、
真实 UID/Root/NSS、密码、Core/Wi-Fi 端到端或部署验证。
