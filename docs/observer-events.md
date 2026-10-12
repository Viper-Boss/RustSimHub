# 事件驱动的 SIM 与短信对象核对

2026-10-13。实现位于 core/observer.rs、observer/watch.rs、observer/sources.rs；只读 modem 接口，短信持久化继续使用 storage schema v3。

## 连接与失效

先注册固定服务的 NameOwnerChanged 监听，再取得唯一 owner 并注册该 owner 下 MM 路径命名空间的信号流。监听任务独立持续消费信号，核对工作在主观察器串行执行。关闭/重连时 Drop 中止旧监听任务，旧连接、对象路径和属性缓存不复用到新连接。

处理 Modem/SIM 的关键 PropertiesChanged/StateChanged、ObjectManager 对象变化以及 Messaging.Added/Deleted。SIM 标识、活动状态、modem/SIM 路径、解锁条件或未知/非运行 modem 状态变化生成新的失效代次。普通网络状态变化只触发核对，SignalQuality 变化不触发整轮短信读取。MM owner 改变、流关闭、D-Bus 错误或整轮超时使状态 unavailable，然后以 1、2、4…最多 30 秒退避重连。稳定运行超过 30 秒后故障退避重新从 1 秒开始。

核对前后仍检查 owner、主 modem/SIM 路径和 ICCID。同一可信身份经过至少 2 秒单调观察才 ready。关键通知会取消正在等待的读取；SQLite 单写者执行 observe_line_if/receive_sms_if 时再检查本地非阻塞门禁，避免排队任务在失效或监听任务关闭后发布旧状态/正文。这些内部回调不能经 IPC 提供。

ready 仍仅代表 SIM 身份观察稳定。D-Bus 通知有传递延迟，硬件状态不与应用事务原子同步；这套只读门禁不能直接宣称未来 Send 已安全验收。

## 批次与对象代次

首次发现唯一 modem 时使用 ObjectManager，正常核对直接读取绑定 modem 的当前属性；身份失效或重连重新发现。短信 List 在首次、目录失效和至少每 10 秒修复时调用，批次之间复用有界路径目录。

最多接受 4096 个 SMS 路径，ObjectManager 初次枚举最多 4224 个对象。每批 16 个对象，单对象读取最多 1 秒，整轮最多 30 秒。批次循环按排序路径和游标推进，结束一轮后空闲兜底 10 秒；有剩余批次时继续推进。通知突发至少合并 200 毫秒，防止高频信号不断立即重读。单对象不存在、超时、属性缺失或正文超限只增加错误计数，其他对象继续导入；硬件原件保留待以后重试。大批次不保证固定的即时通知延迟。

路径删除、新增、目录中消失、已完成对象回到未完成状态，都会丢弃当前绑定。同一路径的完整内容改变也创建新 incarnation。source 由路径和单调 incarnation 组成；旧正文不覆盖。重复读取同一 incarnation/内容继续返回原记录。重连/重启或无法消除的歧义仍保留新记录并用 duplicate_candidate 标注，不能只按正文去重。

监听器保存最多 64 个路径变化提示，过量时丢弃提示队列并要求重新核对整个有界目录及绑定；不会把“提示丢弃”当成短信删除。zbus 信号流持续消费，使用独立有界队列请求；这不等于完成任意本地 D-Bus 大消息/信号风暴的资源安全验收。

## 状态接口

受 status_read 保护的 line_status 响应增加 observer：subscriptions、reconciliations、signals、reconnects、identity_invalidations、object_read_errors、storage_errors。只含连接标志与计数，不含路径、SIM 标识、短信 UUID、号码或正文；计数属于本进程，重启归零。signals 指处理的相关通知，不是总线全部消息数。

普通短信读取仍要求 sms_read，撤销/generation 检查和 64 KiB 帧上限沿用既有契约。默认 --serve-authenticated 不连接 modem；--serve-observing 才启动观察器。没有增加 Send、Create、Delete、PIN、拨号或配置写接口。

## 验证与边界

私有 peer D-Bus 测试仅在 cfg(test) 编译，p2p/bus-impl 也是 Linux dev-dependency feature，正式程序没有模拟入口。测试使用本地 UnixStream 对和随机私有临时库，不连接实际系统总线：40 个短信对象中一个超限，其余 39 条分批持久化；删除/新增同路径保留新消息；SIM 属性信号重新观察并隔离；关闭 peer 使观察器退出当前连接。

其他测试覆盖 513 个路径公平推进、4096 上限、内容变化/未完成状态复用、1000 个提示突发、信号分类、读取取消、监听任务关闭，以及 SQLite 排队任务的执行前门禁。它们不能替代真实拔插、PIN/PUK、MM/总线重启和高负载长稳测试。

尚未解决：modem 历史消息实际 SIM 归属、观察间隙内的等内容同路径复用、超限二进制/长短信、跨重启重复归并、路径提示优先级、磁盘满后的维护/保留策略，以及发送的最终权限/线路/持久提交和外部副作用对账。当前不删除 modem 消息，也不自动重启基带或旧业务服务。

接口依据：[MM Messaging 1.24 XML](https://github.com/linux-mobile-broadband/ModemManager/blob/1.24.0/introspection/org.freedesktop.ModemManager1.Modem.Messaging.xml)、[zbus 5.19 MessageStream](https://docs.rs/zbus/5.19.0/zbus/struct.MessageStream.html)。
