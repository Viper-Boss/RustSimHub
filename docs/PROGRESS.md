# 开发进度

更新时间：2026-10-11 / 初始化阶段。

## 已完成

- 新建本地 RustSimHub 目录、独立 Git 仓库及 Viper-Boss/RustSimHub 私有 GitHub 仓库。
- 详细开发计划已迁入 docs/development-plan.md；新增来源/许可证和接手约定。
- 通过 NAS 的 USB 链路及既有维护密钥连接棒子，完成只读硬件与服务基线。
- 新增 domain/wire/core Rust workspace：通话租约、SIM epoch、短信提交规则、有界协议、自检、Linux 只读 MM 探测及 UDS 服务。
- NAS 的 ARM64 官方 Rust 容器正在构建，源码以允许列表打包；隔离使用数据盘，不挂载旧应用数据或主机 Docker socket。

## 验证状态

- Windows 格式化执行成功。Windows 原生编译未通过：MSVC link.exe 未安装。
- ARM64 测试、release 编译及棒子运行验证尚在执行；最终结果另行更新。
- 板上旧服务仍承担业务，新代码没有拨号、发送短信、启用蜂窝数据或改写分区。

## 阶段状态

P0 硬件/恢复基线进行中：系统和权限已检查，备份及恢复入口未闭环。
P1 基础工程进行中：已有纯规则、协议及只读 core；尚缺数据库、身份认证和业务写接口。
P2–P7 未开始。当前不是可刷写完整固件。

## 下一步

1. 完成 ARM64 编译、失败路径测试、只读实机烟测；记录产物哈希和构建镜像 digest。
2. 固定 boot/kernel/modules 证据并完成私有恢复备份和恢复入口验证。
3. 添加 SQLite 单写者、幂等键唯一约束、事务/outbox 和崩溃恢复。
4. 添加设备身份、本机 owner、客户端配对和 core 鉴权，之后接入真实短信/通话。
