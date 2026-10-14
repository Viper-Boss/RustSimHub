# 旧 NAS 的离线历史迁移

`rustsimhub-migrate` 读取已完成 SQLite backup 并关闭的私有快照，导入独立
`history.sqlite` 历史归档。没有创建活动 SIM、发送队列、租约、principal、密码或 bearer token。
尚未接入正式网页历史查询，也不是已完成 NAS 在线兼容。

命令：`validate SNAPSHOT SOURCE_ID`、`import SNAPSHOT SOURCE_ID PRIVATE_ARCHIVE_DIR`、
`inspect PRIVATE_ARCHIVE_DIR`。SOURCE_ID 为本次旧 NAS 安装的固定 64 位小写十六进制
命名空间，由可信维护方生成并保存；后续同源导入必须复用，不能从网络地址或当前 SIM 推定。
旧 row ID 的唯一映射为 `(source_id, entity, original_primary_key_json)`，不同 NAS 的相同 ID 不合并。

输入必须是本 UID 持有的正规单链接私有文件和私有目录、绝对规范路径；拒绝符号链接、
硬链接、公开模式和 WAL/SHM/journal。通过已验证只读 fd、immutable SQLite 与 query_only
读取，不修改源库。输入最大 256 MiB、10 万行、单条 JSON 128 KiB，归档最大 512 MiB。
导入前完整校验，提交前再次核对输入 SHA256，归档由私有文件锁串行化、事务提交与 fsync。

支持明确的当前 ModemDeck schema 子集：短信、短信线程、联系人、号码、通话记录、
每用户短信/通话已读与收藏、稳定线路、旧 endpoint 与线路映射、用户线路归属。
保留正文、旧 ID、删除时间、revision、每用户标记和原始关联；它们是历史记录，不能被当作
当前 Rust principal 的授权。身份和凭据必须通过新配对重新建立，外发状态只作为历史字段。
列投影是显式清单，新增旧字段不会自动增加导入范围；缺少必需表/列或异常类型会拒绝。

同一快照重复导入只产生 unchanged 计数。已有同源同 ID 的内容变化则拒绝整批事务，
保留冲突证据；不会用旧快照覆盖新归档或把已删除短信复活。当前不自动合并不同时间的
可变快照；这种更新需要后续明确 revision/tombstone 规则，不靠 last-write-wins 猜测。
源数据库先做只读备份，不能把正在使用的 NAS sqlite 文件直接给工具。

CLI 只输出按实体计数和 history_only 标记；错误信息固定，不打印路径、号码、正文、
用户名或哈希。归档和任何查询结果都属于私有数据，不进入 Git 或公共固件镜像。
旧多用户归属会保留，但尚无归档网页的授权映射；不得向普通客户端暴露完整归档。

8f4d15b 完整 CI 和 ARM64 十项测试通过：ID/删除/归属/已读/去重、跨 NAS namespace、
事务冲突回滚、禁止凭据与 outbox、WAL/链接/权限/异常内容及外来目标保护。
原旧项目完整 schema 的独立合成快照已在 NAS 和隔离非 Root 棒子完成 15→15 重复导入，
插入计数 15→0，源文件不变、各项标记和归属保留；见 VALIDATION-HISTORY-P5A.md。
后续仍需授权历史查询、真实旧库副本和更多版本契约；不能当作完整 NAS 兼容已完成。
