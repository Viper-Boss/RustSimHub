# 旧 NAS 完整 schema 的离线历史归档验收

2026-10-14，Rust 源码 8f4d15b，CI 38519741620 success。
持久离线 ARM64 builder `rustsimhub-history-build-p5` 最终 exited 0；
10 项兼容/迁移测试通过，release 构建完成。没有拨号、发短信、蜂窝或网络修改。

- source archive SHA256：`0df653f7810727d1d56ab6da3257bc2a0a09ddcf41a6999a058393750cf6eaff`
- rustsimhub-migrate SHA256：`9c88aea0f46b0f24a780edd239a4089504458d1957fbe81b36d8fb804513e14d`
- 旧项目完整 schema SHA256：`2376e571caa795b37d49e0b95ee69efd3eb3a48e513b8be2e362de9f1ac04f09`

独立 Python SQLite 用当前旧项目完整 schema 构建合成快照，开启外键并检查无违例，
不是使用 Rust 读取器的列投影创建一份自证输入。只包含合成消息、联系人、通话及用户标记。
输入、归档和原始报告均在私有目录，不包含真实用户旧库或凭据。

NAS UID 1000 与棒子 Core UID 的隔离 systemd 单元均完成 validate、import、再次 import、
inspect。棒子单元使用 PrivateDevices、禁止 /run/dbus、严格系统只读和有限归档可写路径，
不访问调制解调器。首次插入 15 条、再次插入 0 条且 unchanged=15；源快照 SHA 前后不变。

从棒子取回合成归档后，独立 Python 查询确认原短信 ID、已删除时间、外发历史状态、
联系人原 owner、每用户线程标记保留。密码、principal、SMS/call 执行队列表不存在；
旧外发状态仍是归档 JSON，不进入业务重放。单测另外验证变化快照拒绝与整批回滚、
不同 NAS 的同 ID 隔离、WAL/链接/权限/异常数据及外来目标保护。

`tools/test-history-migration.py`；私有报告 `.private/history-full-schema-8f4d15b-first.json`
passed=true、pcm_closed_and_test_units_stopped=true。原四服务 active、所有 PCM closed。
程序在 `/opt/rustsimhub/dev-history-p5-8f4d15b/rustsimhub-migrate`；没有安装启动单元。

首次实现 u64/usize FromSql 编译错误和合成目录权限失败已修复。独立旧 mtime 缓存测试
被排除；所有复用工作区的打包源码必须 tar --touch。临时 foreground 构建的观察超时没有
当作成功或直接重启；确认其容器 collected 后使用持久异步构建，保留真实退出状态与完整日志。

验收边界：证明当前完整 schema 的合成历史归档与重复导入行为；尚未证明真实旧库的全部
数据/版本兼容，也未接入授权历史网页、NAS 在线会话或 PCM 桥。不同时间快照的变更合并
仍拒绝冲突，不推定最后写入优先。P5 只计这部分实现与验收，完整 NAS 交付继续。
