# P1 持久化验证

日期：2026-10-12。源码检查点：`0ebde80`。当前仍为开发原型，未接入真实 modem Send。

## 验证结果

| 环境 | 检查 | 结果 |
|---|---|---|
| GitHub Linux / Rust 1.98 | fmt、28 项 workspace 测试、Clippy -D warnings（all-features） | 全部通过 |
| NAS ARM64 / Rust 1.99 | 28 项 workspace 测试（core 2、domain 7、storage 16、wire 3） | 全部通过 |
| NAS ARM64 | release + core/dev-fixtures、自检 | 通过，容器退出 0 |
| 棒子 nassimhub 用户 | SQLite 幂等、Backup API、重启 unknown、恢复 epoch | 通过，检查流程样本 587 ms |
| 棒子 nassimhub 用户 | 提交后进程直接退出 73，再启动恢复 | 通过，Unknown 不可重发 |
| 棒子文件权限 | 4 个测试/备份/恢复库 | 均 0600、所属 nassimhub、各 28672 字节 |

CI：[38156813973](https://github.com/Viper-Boss/RustSimHub/actions/runs/38156813973)。
NAS 精简镜像没有 cargo-clippy，因此 Clippy 在 CI 执行，不声称 ARM64 Clippy 已运行。
28 项计数中包含一个用于子进程故障注入的辅助测试入口。

## 关键失败路径

- 并发相同请求只有一个操作、一个 prepared 事件；同键不同参数冲突。
- 非所属客户端不能提交；重复提交和重复确认拒绝。
- 注入事件写入失败后，操作保持 Prepared、游标不前进，不返回发送许可。
- SQLite 页面限额触发 SQLITE_FULL，操作和事件一起回滚。
- SIM 切换取消 Prepared、隔离 Submitting，并持久保留新的 epoch。
- 第二个进程/连接的 Store 被文件锁拒绝，不能对活跃发送执行启动恢复。
- 新版本、外部数据库、损坏库、符号链接和宽松权限均拒绝；保留已有内容。
- 子进程在已提交 Submitting 后退出：重开得到 Unknown；在未提交事务中退出：保留 Prepared。
- 备份恢复轮换 db_epoch；Prepared 也转 Unknown，避免备份之后已发出的短信被重发。
- 完成复制但未完成隔离的恢复标记会触发隔离；空的部分恢复被拒绝并保留证据。
- 不同 epoch、未来游标和超大游标拒绝。

本次 SQLITE_FULL 是数据库页面限额模拟，不是把 NAS 或棒子磁盘写满。
进程退出测试不等同于真实断电或硬件 I/O 故障验收。

## 产物与来源

- 镜像：`rust@sha256:2c3a22f0a5533ea2dd5a16627bc841228151faa2d4de2644ac9987e4a2f1f2fa`。
- rusqlite 0.40.2 / libsqlite3-sys 0.38.2（bundled），依赖 SHA 在 Cargo.lock。
- 最终允许列表源包 16 个文件，与已提交 Git blob 逐字节一致。
- 源包 SHA-256：`ec0c70e1326dc0e828308d5ba6e499f961f95f9364035b57bbbf7cb2c1bd88f0`。
- ARM64 开发程序：3,888,560 字节，约 3.71 MiB，带 dev-fixtures。
- 程序 SHA-256：`767a308c26406f519e7d7b67a81ffa9f9129381f71f2fb615633817f8741467c`。
- 最终增量测试构建约 34 秒、release 约 2m13s。首次引入 SQLite 的优化编译约 12 分钟；不将二者混为同一构建。
- 本机 `.local/artifacts/rustsimhub-core-p1`；棒子 `/opt/rustsimhub/dev-p1/rustsimhub-core`。
- NAS 当前产物在构建目录 `src/target/release/rustsimhub-core`，原 P0 已归档到 `history/p0-first/`。

棒子测试结束后，旧四个关键服务仍 active；Wi-Fi 与 USB ethernet connected，gsm disconnected。
本轮没有读取旧业务数据库、调用真实发送/拨号、启用蜂窝数据或安装新开机服务。
测试数据库留在棒子独立随机私有目录，原始日志仅保存于本机 .private/。

## 尚未完成

Store 已完成基础实现；core 正常只读服务尚未绑定 Store 的启动和关闭流程。
仍需身份/配对/权限、MM SIM 监听、modem 对象关联与真实短信对账、收件箱、
事件 ACK/保留/授权/一致性快照、后续 schema 迁移、完整配额管理和断电验证。
