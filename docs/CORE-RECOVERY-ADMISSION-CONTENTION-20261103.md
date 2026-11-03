# Core恢复确认与后台Gate锁竞争：诊断交接

此项属于成员D原Core恢复执行链的修复范围。主线只做诊断、最小复现和验收边界，不在本批接管其Core/Auth/Storage实现，也不放宽原权限、租约或Unknown规则。

## 已复现的机制

固定原产品e3762ec/归档06cb909c，在私有副本里仅加入合成时序和错误种类日志：

1. 在 `control_recovery` 的 `with_auth(... move |a| ...)` 闭包入口暂停250ms。暂停时已经持有真实Auth mutex，尚未执行控制请求的持久claim或入队。
2. 原 `run_trial` 每150ms检查Gate。原 `with_auth` 使用 `try_lock`，取锁失败返回Busy；原actor随后按失败路径回滚并完成事实记账。
3. 控制线程恢复后，原 `check_recovery_trial` 检查已不再active的Begin收据，底层拒绝；Auth把Storage所有错误统一映射为 `Error::Storage`。

实际日志顺序为：

```text
SYNTHETIC_DIAG control_admission_auth_hold_start
SYNTHETIC_DIAG auth_lock_busy
SYNTHETIC_DIAG control_admission_auth_hold_end
SYNTHETIC_DIAG storage_error=Forbidden
... tests.rs:549 ... Err ... Storage
```

原失败测试 `partial_persist_and_failed_compensation_leave_unknown_and_refuse_new_effects` 在该条件下单次以0通过/1失败、退出101结束，和报告中的同类外层Storage一致。这里底层是Forbidden，**不是磁盘I/O错误**。这是受控故障注入，证明一种同类机制存在；没有原网页版失败期间的锁时序日志，所以不能断言其必然根因就是此机制。

没有更改主线工作树Core/Auth代码、依赖、断言或生产二进制；诊断副本未提供release来源stamp，不是候选包。普通UID、独立loopback、无设备/NAS/消息外发，私有证据`.private/recovery-admission-contention-20261103-first.json`与同名前缀日志保留。

## 维护者下一步

- 在原确认/取消队列及原actor里设计一致的admission顺序，避免**自己正在验证的合法控制**与后台Gate互相拆台；不引入第二网络事务或远端可伪造的claim。
- 优先评估让合法控制的准入与原actor消费在同一序列中完成。具体接口和重复回执/取消/队列满行为须给出可审方案，不把这里的建议当作已批准实现。
- 保持外部长期持Auth锁时仍有限失败和真实回滚。不能直接改成无限阻塞 `lock()`，也不能把Busy当成验证成功或延长授权deadline。
- 底层Forbidden、SQL/I/O及Busy需要在合成诊断中可区分，但客户端错误仍不得泄露敏感状态、SSID、正文、密钥或内部SQL。
- 原请求UUID/realm/Owner/grant/连接、独立Confirm、绝对deadline、保存前后撤权和Unknown均保持；丢回复不能重新执行。

## 必须证明的回归

1. 控制准入的确定性150ms边界交错，后端commit恰好一次且收据一致；不靠“单项后来通过”替代。
2. 队列已满、重复/旧请求、外来连接和Normal控制证明仍拒绝，且不消耗无处执行的claim。
3. 正在准入时撤权、断连或到原deadline，拒绝后端新副作用，原已发生副作用仍记录清理/Unknown。
4. 长期Auth占用仍按原有限清理，不让阻塞任务在超时后复活继续执行。
5. 原 `busy_auth_mutex_cannot_hide_an_actual_backend_rollback` 必须仍通过，不能删除、改弱或回填结果。
6. 原整组87项和真实Core/Auth/SQLite链按精确HEAD跑完；外部NM/verifier仍可合成，不能拿合成当设备验收。

本批未修复Core生产逻辑，不勾选D步骤或提升原81.5%验收比例。音频过期帧的主线LAN修复是独立问题，不能据其通过宣称Core或网页版全部失败已解决。
