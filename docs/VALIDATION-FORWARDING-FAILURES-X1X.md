# X1X · 转发错误与容量矩阵（进行中）

2026-10-27，主线FWD-15。新增三项实际SQLite/TLS回归已通过，本项仍需操作系统级ENOSPC和固定提交CI，不计完成。不访问NAS/棒子，不请求真实提供商，不开启普通数据。

## 本轮新增证据

| 情形 | 实际验证 | 结果 |
|---|---|---|
| 4096个回执上限 | 一个真实领取并标Unknown的任务，事务填入4095个已确认合成历史回执；通过真实ingest再入队 | Full，游标/队列不前进；Owner明确prune仅删除4095个终态，Unknown及原请求保留；同一未消费事件在腾位后可正常入队 |
| SQLite无法分配页面 | 将临时真实DB的max_page_count收紧到实际page_count且无空闲页；额外DDL实际返回SQLite DiskFull | 大正文通过真实ingest返回Storage，游标和旧Unknown/原请求不变，无新queued；恢复原8192页预算后同一事件可入队 |
| HTTP错误与Retry-After | 本机实际TCP/rustls/Hyper，400/401/403/404/408/413/429/500/502/503/504，带Retry-After:3600 | 已提交后全为Unknown/Unconfirmed并保留实际status；每例仅一次连接/一次完整请求，不重试或跟随 |

工作源码截面728文件，SHA-256 `a0a1e7931fac9b2c452f4164602957a69b7fb8d305194bc20c4716eed6b90602`。Core157、forwarder78、Wire18，共253项全部通过；同三包严格Clippy、workspace fmt、Core/Edge/forwarder生产检查通过。现有nobody身份、空能力/NoNewPrivileges、私有网络/设备、只读源码与依赖，实际TLS仅loopback。

SQLite分页上限触发的FULL是实际引擎故障路径，但不是文件系统真正写满。本轮没有填满宿主、NAS或棒子的磁盘，也不把它等同ENOSPC验收。

## 既有矩阵接续

当前实际转发库测试同时覆盖256个未决上限、接收事件和目标入队原子提交、接收方插入/游标更新失败回滚、已in_flight重开变Unknown、旧attempt/代次迟到结果拒绝、禁止剪除Unknown。TLS组覆盖根证书/主机名、提交前各鉴权点、TLS中取消、失去主管、丢HTTP回复、总截止、重定向、固定/分块响应超过8192字节、压缩/过多过长响应头、提供商业务错误及日志不复制正文/凭据。进程退出、凭据/cgroup等真实系统夹具还需与最终固定CI一起归档，不能仅按测试标题计完成。

## 下一步

在独立mount/network命名空间内创建有界1～2MiB tmpfs，仅对合成outbox运行不可默认执行的故障用例。先证实临时文件系统容量上限，再以固定小块填充到实际ENOSPC；通过真实ingest核对整批不入队、游标与Unknown/原请求不丢；释放合成占位后同一事件恢复一次。脚本必须拒绝在普通宿主目录或未证实容量的文件系统填充，不触及实际设备。之后补CI接线和固定提交结果，再逐项验收FWD-15。
