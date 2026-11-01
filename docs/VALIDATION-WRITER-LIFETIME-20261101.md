# 转发队列与应用存储的写锁生命周期

## 触发与证据边界

正式WSL构建器在固定 `97f9c52fa832591a5e260d3dc4181847a8b3dfe3` 跑全工作区时，Core的 `repeated_switches_and_queue_reopens_never_forward_disabled_backlogs_or_old_claims` 在关闭并立即重开Queue后得到Busy。PR #153的远端run 41291276828也在 `c14_signature_and_schema_rejection_preserve_actual_store` 关闭并重开Store时得到Busy；另一轮41291274518通过，失败不能抹掉。

这两次历史日志没有保存持有文件描述符的进程，**不能据此宣布已证实当时的fork窗口**。本次先对Queue、Store各做一个确定性回归：复制锁的文件描述符，保持副本存活，关闭拥有者，再立即重开。两处旧实现都稳定返回Busy，修正后成功；同时验证活动拥有者仍拒绝第二个写者，关闭旧副本也不能解锁独立的后继拥有者。Store测试还核对原state.json、epoch floor和已签名工厂组保持。

## 修正

- Queue和Store原来只在File析构时关闭描述符。增加私有RAII guard，在真正拥有者完成所有同步操作后显式unlock，随后关闭文件。
- Queue保留字段声明次序：SQLite Connection先析构，再释放WriterLock。构造失败也按局部变量逆序先关闭连接后释放guard。复用现有主数据库相同的生命周期方式，不增加新依赖。
- 不改变非阻塞独占准入、状态/数据版本、签名格式、权限或转发默认关闭策略；不靠重试、放宽锁或串行化整个测试集解决问题。

机制依据：[Linux flock(2)](https://man7.org/linux/man-pages/man2/flock.2.html)说明dup/fork副本共享锁，显式LOCK_UN或最后一个副本关闭才释放；独立open描述符分别持锁。[Rust Reference](https://doc.rust-lang.org/reference/destructors.html)规定struct字段按声明次序析构。

## 已执行

- 两个新增回归分别在旧实现实际失败，修正后通过；并非只测试修正后的实现。
- 完整工作区 `cargo test --workspace --all-features --locked --offline --quiet`：**1351通过/0失败/6项原有专用隔离测试忽略**。私有回环网络中先降为普通用户再编译/测试，Rust测试集未强制串行，包含Core234、forwarder94、update54通过。不能将忽略的tmpfs/Root程序测试记作此命令通过。
- 从Cargo实际产物定位并重建转发lib测试，另一次私有回环网络Root测试94通过/1项已有tmpfs测试忽略。最初误用旧二进制的失败保留，不能当成修正版通过证据。
- 本机普通网络环境有5个出口夹具失败；私有回环环境普通用户完整测试通过，没有削弱生产接口绑定。此前入口脚本错误及失败日志保留。
- fmt、Core/forwarder/update的全部目标、全部特性严格Clippy通过；`git diff --check`通过。

完整正式构建器和合并HEAD的远端CI仍须分别绑定实际结果；上述专项不能替代它们。未连接NAS/棒子、未签生产材料、未部署或发送消息，人工测试最后。原P0–P7 **81.5%**及完整行动台账 **20/169**不变；成员分工不变。
