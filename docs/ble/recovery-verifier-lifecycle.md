# D-22：阻塞验证的连接保留与迟到结果清理

2026-10-30，接续已合并的运输层 PR #92。本批只提供可信宿主的工作执行边界，
没有选择密码协议、配置验证器或开放恢复服务。

`PeerConnection::verify_blocking(packet, adapter)` 消耗已接纳的连接和有界 Packet。
连接失效或 CloseTransport 帧直接拒绝；正常调用把连接、包和只读适配器调用一起移入
独立的命名线程，不占 Tokio 共享阻塞池。调用者须在 poll 前释放普通 Auth 锁；
不得在适配器里重新取得该锁执行未知耗时操作，也不得派生返回后仍运行的后台验证。

| 路径 | 资源与输出 |
|---|---|
| 有效期限内成功 | 把原连接和适配器输出交回宿主；输出不是已验证业务能力 |
| 适配器失败 / 线程创建失败 | 有限 Error；关闭原连接并释放占用，无普通业务派发 |
| unwind 构建中的 panic | Worker 错误，栈展开丢弃连接；不是 release 进程存活保证 |
| 五秒截止 / 已 poll 后取消等待 | 等待者退出；线程继续持有连接、零化包和占用槽，真正返回后丢弃迟到输出，再清理资源 |
| 未 poll 就丢弃 future | 不启动线程，立即丢弃其拥有的连接和包 |
| 线程开始或结果交付时已过期 | 不调用适配器或不交付结果；不刷新原截止 |

一个 SourceGuard 只能有一个持有中的连接，所以超时不能释放旧槽并并行启动第二个验证。
宿主必须维持同一个 Guard，不能重建 Guard 来绕过占用。成功交回的连接也仍占用槽。
线程结束/错误丢弃连接时沿用原始字段顺序，先关闭 stream 再释放 Slot。

这里不强杀 Rust 线程。适配器永久卡住时，该 Guard 保持 busy，内存/流/槽保留至线程
返回或进程退出；不是已完成关闭/资源已排空的证明。线程不能代替 D-24 cgroup/CPU
预算或服务监督，也不为任意不可信插件提供隔离。线程创建失败映射 Worker，返回值不含
原始系统错误。普通 panic hook 仍会运行，适配器不得在 panic/日志中带秘密。
本仓库 release profile 是 **panic=abort**，该构建的线程 panic 会终止整个 Core，
不会变成 Worker；本地 unwind 测试不能推导 release 的故障隔离。未改变 profile/hook，
生产接线须评审真实适配器的 panic 行为。捕获其他锁或分离子任务仍是可信装配责任。

`T` 是普通内部输出，不是 Auth 的恢复能力；本批没有生产验证器、Owner/grant 检查、
会话防重或 Wi-Fi 派发。宿主拿到结果后仍须短锁复核同 Auth 实例、设备/realm/Owner/
grant 代次、连接和单调期限，再按既定接口铸造能力。断线之前的 context 也不保证
远端当下在线；网络保存 Gate 和持久 Unknown 属于 D-23，不能以本线程成功替代。

## 实际验证与边界

测试直接调用本生产模块的私有 `run_owned`，使用真实 UnixStream、实际 Slot 析构、
oneshot/mpsc 和真实线程；只模拟适配器的返回/阻塞/panic，不另造执行状态机。
**没有构造 PeerContext 或 PeerConnection 来伪造合法来源**，正例测试的是资源所有权
执行器，不是 SourceGuard 的跨 UID 准入或密码成功。

新增九项覆盖成功返回原流、错误、panic、超时、已 poll 取消、未 poll 丢弃、入口过期、
共享阻塞池饱和时仍执行，以及适配器返回后等待者过期。超时/取消用通道明确保持线程
未结束，实际检查流仍无 EOF、Slot 仍占用；释放工作后检查 EOF、Slot 释放及输出析构。
所有阻塞夹具有两秒释放期限，产品公开截止仍固定五秒。

环境支持单 UID 的临时 user namespace，但多 UID 映射因缺 `newuidmap` 返回127。
未安装助手/修改 NSS 或主机账户；合法跨 UID SourceGuard 准入、nonce/占用在此真实准入
路径上的生命周期仍未运行。namespace 内显示 Root 不是主机 Root 或设备验收。
现有运输层14项拒绝/编解码测试保持。

完整证据见[交付报告](../deliveries/member-d-recovery-peer-lifecycle.md)。
默认 feature 关闭，Core 二进制未调用此 API；无新依赖、schema、Root 材料或部署。
撤回本 API 无数据迁移/设备恢复动作。D-22 仍缺 A 会话及实际 GATT 接线，完整步骤不增加。
