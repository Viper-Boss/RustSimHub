# 开发进度

更新时间：2026-10-12 / P1 持久化模块完成开发与实机验证，完整固件开发继续。

本轮继续：SQLite 单写者、持久短信操作与事务事件、备份恢复及失败路径测试已完成。
恢复备份会隔离 Prepared/Submitting，并以持久恢复标记防止中途退出后误发送。
28 项测试、CI Clippy、ARM64 构建和棒子非 root 进程退出恢复均通过。
实现见 [storage.md](storage.md)，证据见 [VALIDATION-STORAGE-P1.md](VALIDATION-STORAGE-P1.md)。

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

## 验证状态

- Windows 格式化执行成功。Windows 原生编译未通过：MSVC link.exe 未安装。
- ARM64 Linux：12 项测试通过（core 2、domain 7、wire 3）。覆盖断开/截断帧、超大帧、角色注入、租约过期与换卡、未知短信禁止重发。
- GitHub CI 已通过：Linux 测试、格式检查、Clippy（-D warnings）。首次运行：[38153648494](https://github.com/Viper-Boss/RustSimHub/actions/runs/38153648494)。
- ARM64 release 编译、自检、真实 D-Bus 与受限 UDS 实机验证均通过。程序约 1.88 MiB，只读服务 RSS 样本约 3.3 MiB。
- 完整证据与哈希：[VALIDATION-P0-P1.md](VALIDATION-P0-P1.md)。这些结果不能替代完整业务/性能验收。
- 板上旧服务仍承担业务，新代码没有拨号、发送短信、启用蜂窝数据或改写分区。

## 阶段状态

P0 硬件/恢复基线进行中：系统和权限已检查，备份及恢复入口未闭环。
P1 基础工程进行中：纯规则、协议、只读 core 和持久化基础已实现并验证；仍缺身份/配对/鉴权及常规服务生命周期绑定。
P2–P7 未开始。当前不是可刷写完整固件。

## 下一步

1. 添加设备 Ed25519 身份、本机 owner、客户端配对与权限记录，完成 core 鉴权。
2. 将 Store 绑定 core 启停与受限 IPC，随后接入 MM SIM/短信事件及真实发送对象对账，再推进通话。
3. 并行推进 P0 剩余证据：内核精确源码/补丁、完整私有恢复备份和恢复入口验证。完成前不刷 rootfs/boot。
4. 按计划继续 MM 事件/短信、通话控制、音频 PoC、网络切换及系统镜像集成。
