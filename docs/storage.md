# SQLite 持久化边界

当前实现：crates/storage，P1 的持久化基础。尚无真实 modem Send 或远端写接口。

## 所有权与持久化

Store 启动独立阻塞线程，它独占一个 SQLite Connection 和 .writer.lock 的文件锁。
Handle 通过容量 32 的通道提交请求；满时返回 Busy，退出后返回 Stopped。
调用方必须在 blocking worker 中使用同步 Handle；不得阻塞 Tokio 事件循环。
Handle 接受的 client_id 必须由后续 core 鉴权得出，不能直接信任远端请求。

Linux 数据目录必须已有、由进程用户拥有、权限 0700；数据库/锁/WAL/SHM 必须是
同用户私有普通文件。符号链接和宽松权限拒绝。Windows 的生产 ACL 尚未实现。
WAL + synchronous=FULL、foreign_keys=ON、trusted_schema=OFF，busy_timeout 3 秒。
自动 checkpoint 256 页，journal_size_limit 4 MiB，max_page_count 8192（默认 4 KiB 页约 32 MiB）。
journal_size_limit 是回收目标，不是 WAL 活跃期间的硬上限；本阶段未实现保留空间及完整配额管理。
依据：[SQLite WAL](https://www.sqlite.org/wal.html)、[同步选项](https://www.sqlite.org/pragma.html#pragma_synchronous)。

user_version=1、application_id 固定。仅初始化空数据库，不接管外部库；
新版 schema、损坏库均报错并保留文件，不生成空库覆盖。v1 初始化在事务中完成。
未来版本的 expand/contract 迁移及失败回退还需单独实现和验收。

## 短信操作

prepare_sms(request) 将操作和 prepared 事件一起提交。
唯一约束为 (client_id,idempotency_key)，语义指纹涵盖 SIM epoch、号码及正文等请求参数。
同键同参数返回原操作，不增加事件；同键不同参数返回 Conflict。
旧 SIM 的相同重试返回原已取消/未知操作，不为新 SIM 创建新的发送。
当前没有过期删除，不能声明已经实现 7 天保留期。

begin_submit(id,client) 校验所属客户端、当前 SIM epoch 和 Prepared 状态，
提交 Submitting 与事件后才返回携带号码/正文的 Submission。
调用 modem Send 前必须取得此结果。提交失败时不会返回发送许可。
成功收到 modem 接受结果后 mark_submitted；失败/超时不能证明未发时 mark_unknown。

启动恢复将全部 Submitting → Unknown 并生成事件，禁止自动重发。
change_line 在一个事务里取消 Prepared、将 Submitting 转 Unknown、推进 epoch 并记录事件。
迟到的旧 SIM 结果不能修改已隔离的操作。SIM 变化来源仍待接入真实 MM 监听。
操作和 Submission 的正文/号码不实现 Debug；错误只输出固定类别。

该顺序实现应用层“最多尝试一次”的保守边界，不保证运营商只投递一次。
modem 对象关联、结果对账、收件箱、分段短信及用户明确的新操作重试仍待 P2 实现。

## 事件和备份

events 在同一事务递增 SQLite sequence，接口以十进制字符串返回，避免 JS 大整数精度损失。
cursor={db_epoch,seq}；分页最多 256 条，不能跳到未来 sequence。
不同 epoch 返回 ResyncRequired。当前内部事件包含操作 ID、SIM epoch 和固定 kind，无正文。
尚无客户端 ACK、保留窗口、订阅授权及一致性业务快照 API；不等同于完整同步服务。

backup 使用 SQLite Backup API 写入独立私有目录；先写 backup.pending、同步文件，
再重命名 backup.sqlite3 并同步目录。拒绝覆盖已有备份，失败残留需要保留并检查。
restore 只支持离线恢复到新目录，恢复时在同一事务中生成新的 db_epoch，
并把备份中的 Prepared/Submitting 全部转为 Unknown，避免恢复较旧备份后重新发送。
复制前先同步 .restore-pending 标记，只有完成隔离事务后才移除并同步目录。
若进程在中途退出，下次 open 会依据标记继续执行隔离；空或损坏的部分副本会被保留并拒绝。
旧 cursor 随 epoch 失效。真实基带对象关联与人工对账流程仍待实现。
P1 身份模块新增 schema v2，从 v1 事务迁移且保留业务 cursor。
恢复时隔离全部持久授权，防止旧备份复活已撤销的权限，详情和离线恢复限制见 auth.md。
参考：[rusqlite Backup API](https://docs.rs/rusqlite/latest/rusqlite/backup/index.html)。

## 开发实机检查

诊断 fixture 仅在显式启用 core 的 dev-fixtures feature 时编译，默认构建不提供这些命令。
所有短信内容都是合成数据，不调用 modem。

```sh
cargo test --workspace --all-features --locked
cargo build --release -p rustsimhub-core --features dev-fixtures --locked
rustsimhub-core --storage-check PRIVATE_FRESH_DIRECTORY
rustsimhub-core --storage-crash-fixture PRIVATE_FRESH_DIRECTORY  # 预期退出 73，无 Rust 析构
rustsimhub-core --storage-recover-fixture SAME_CRASH_DIRECTORY
```

check 验证幂等、重启 unknown、Backup API 和 restore epoch。
crash/recover 验证真实进程终止后的 WAL 恢复；恢复命令要求已存在的 fixture 操作标记。
这些不能替代设备断电、磁盘 I/O 故障和完整生产数据恢复验收。
