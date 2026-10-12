# 开发进度

更新时间：2026-10-12 / P1 所有者恢复、客户端 CLI 和关闭重启完成实机验证，完整固件开发继续。

本轮继续：原 owner 的离线签名恢复、独立身份备份、Linux simhubctl、明确重新批准和鉴权 daemon 关闭/重启已实现。
恢复不启用客户端；客户端必须重新证明并经 owner 精确批准。旧会话不能随重批复活。
59 项测试、CI Clippy、ARM64 workspace 默认检查/构建及棒子完整 CLI/SIGTERM/SIGKILL 流程通过。
实现见 [recovery-clients.md](recovery-clients.md)，证据见 [VALIDATION-RECOVERY-CLI-P1.md](VALIDATION-RECOVERY-CLI-P1.md)。

## 已完成

- 新建本地 RustSimHub 目录、独立 Git 仓库及 Viper-Boss/RustSimHub 私有 GitHub 仓库。
- 详细开发计划已迁入 docs/development-plan.md；新增来源/许可证和接手约定。
- 通过 NAS 的 USB 链路及既有维护密钥连接棒子，完成只读硬件与服务基线。
- 新增 domain/wire/core Rust workspace：通话租约、SIM epoch、短信提交规则、有界协议、自检、Linux 只读 MM 探测及 UDS 服务。
- NAS 的 ARM64 官方 Rust 容器已通过测试和 release 构建；源码以允许列表打包，隔离使用数据盘，不挂载旧应用数据或主机 Docker socket。
- boot 与模块清单哈希已固定；64 MiB boot 备份已保存到本机与 NAS 的私有目录，哈希校验一致。
- ARM64 程序已在棒子运行并完成非 root D-Bus 及 UDS 烟测，拒绝非授权 UID；测试单元已停止。
- 新增 storage crate：单写者文件锁、32 容量请求队列、WAL/FULL、schema v1、持久幂等键、事务事件、SIM 隔离、Unknown 恢复、Backup API 与安全恢复标记。
- 默认 core 构建不提供存储 fixture；显式 dev-fixtures 开发构建在棒子独立目录完成合成数据验证。
- 新增 auth/policy crate、schema v2 身份/权限记录与 v1 迁移；签名挑战绑定双方公钥、purpose、scope、generation 与进程随机 instance。
- 新增离线 owner CLI 和 v2 鉴权 UDS；token 只保存摘要，重启失效，撤销持久生效；真实业务写操作仍未开放。
- 新增可信离线恢复、身份明文私有备份、独立客户端密钥/固定设备 pin/私有会话文件与 owner 管理 CLI。
- 鉴权 daemon 支持 SIGTERM/SIGINT 排空、唯一 writer 关闭、运行锁、同路径重启和受限 stale socket 接管。

## 验证状态

- Windows 格式化执行成功。Windows 原生编译未通过：MSVC link.exe 未安装。
- ARM64 Linux：12 项测试通过（core 2、domain 7、wire 3）。覆盖断开/截断帧、超大帧、角色注入、租约过期与换卡、未知短信禁止重发。
- GitHub CI 已通过：Linux 测试、格式检查、Clippy（-D warnings）。首次运行：[38153648494](https://github.com/Viper-Boss/RustSimHub/actions/runs/38153648494)。
- ARM64 release 编译、自检、真实 D-Bus 与受限 UDS 实机验证均通过。程序约 1.88 MiB，只读服务 RSS 样本约 3.3 MiB。
- 完整证据与哈希：[VALIDATION-P0-P1.md](VALIDATION-P0-P1.md)。这些结果不能替代完整业务/性能验收。
- 上轮 47 项身份/权限测试及实机记录见 VALIDATION-AUTH-P1.md；本轮 59 项通过（auth 19、core 8、ctl 1、domain 7、policy 1、storage 19、wire 4）。CI：[38265061208](https://github.com/Viper-Boss/RustSimHub/actions/runs/38265061208)。
- 最新开发 core 约 4.15 MiB、simhubctl 约 0.75 MiB，完成真实离线 CLI 恢复、重新批准/撤销、SIGTERM/SIGKILL 重启；仍不等同于完整性能或固件验收。
- 板上旧服务仍承担业务，新代码没有拨号、发送短信、启用蜂窝数据或改写分区。

## 阶段状态

P0 硬件/恢复基线进行中：系统和权限已检查，备份及恢复入口未闭环。
P1 基础工程进行中：纯规则、协议、持久化、公钥鉴权、恢复/客户端 CLI 与关闭重启已实现并验证；仍缺网页密码/TLS、最终用户设置 UI、完整开机部署和系统恢复。
P2–P7 未开始。当前不是可刷写完整固件。

## 下一步

1. 在鉴权 core 接入受限业务 IPC 与 MM SIM/短信事件、观察期及真实对象对账；实际发送在最终执行点校验权限和持久提交。
2. 继续网页密码/TLS 与生产 systemd 集成，再推进通话；所有者遗失/变更及加密备份另行设计。
3. 并行推进 P0 剩余证据：内核精确源码/补丁、完整私有恢复备份和恢复入口验证。完成前不刷 rootfs/boot。
4. 按计划继续 MM 事件/短信、通话控制、音频 PoC、网络切换及系统镜像集成。
