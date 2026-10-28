# 本机 WSL2 Linux 测试入口

用户已要求本机后续 Linux 测试默认在 WSL2 执行，不再以 Windows 跳过结果替代 Linux 验证。
本页只描述开发测试环境，不是设备部署、恢复或整机升级步骤。

## 环境

Windows 侧安装 Microsoft WSL，Ubuntu-24.04 以 WSL2 模式运行。
测试使用普通 Linux 用户 `rsh-test`，Rust 工具链与根 `rust-toolchain.toml` 一致（1.98.0，包含 rustfmt/Clippy）。
准备 Python、C 编译器；compat/完整工作区测试还需要 Go，网页检查需要 Node，原生音频需要 ALSA/Opus 开发库。
工具安装和已经验证的版本以本人交付报告为准；WSL 程序的版本号与发行版的 WSL1/2 模式是两回事。

本机镜像网络曾返回系统错误，已回退到 NAT，保留防火墙；不会为了下载依赖关闭 TLS 验证或开放 LAN 代理。
本机仅对新测试发行版设置资源上限及普通用户，没有授予免密码 sudo，也没有将设备接口传入虚拟机。
若使用别的发行版或用户，通过下面的参数明确选择，不覆盖/注销现有发行版。

## 自动入口

在 Windows 仓库中运行：

```sh
# 默认只列计划
python tools/fixtures/backup-recovery/wsl_test.py

# 首次准备已有锁定依赖；实际运行全部15个业务恢复场景与Cargo接入
python tools/fixtures/backup-recovery/wsl_test.py --run --suite recovery --fetch

# 缓存准备好后的重复运行，不联网下载依赖
python tools/fixtures/backup-recovery/wsl_test.py --run --suite recovery

# 其他Linux测试范围
python tools/fixtures/backup-recovery/wsl_test.py --run --suite compat --fetch
python tools/fixtures/backup-recovery/wsl_test.py --run --suite workspace --fetch
python tools/fixtures/backup-recovery/wsl_test.py --run --suite native --fetch
```

`--distribution Ubuntu-24.04 --user rsh-test` 为默认值；可显式替换。
`--ref HEAD` 默认只测试当前完整提交，不把未提交的工作区文件混入成功结果。
如需先验证暂存快照，可在明确授权的 Git 操作中生成 tree SHA，再用 `--ref <TREE_SHA>`；
结果会标记 `kind=tree`、base HEAD 和精确 tree，不冒充提交 HEAD 已通过。发布后仍需复核最终提交。

## 源码与测试保持

入口将选定 Git 对象归档，记录对象类型、完整 SHA、tree、归档及执行器 SHA256。
WSL 端核对实际 WSL2 内核、非 Root UID 和归档摘要，拒绝链接、越界、特殊文件及超限源码。
不复制 `.git`、私有维护目录、Windows target、生产数据库、凭据或预编译产物。
源码从公开 Git 归档恢复到 Linux 用户目录；临时数据库、TMPDIR 和 target 都位于 Linux 原生文件系统，
避免 Windows 挂载盘的权限/锁行为使 SQLite 测试失真。

recovery 范围依次执行锁定依赖准备、全仓格式、四项构建元数据单测、十五项真实矩阵、
recovery Cargo 测试及全 targets 严格 Clippy。
矩阵原本就核对十五个固定场景及逐项通过集合，零 ignored；Cargo 接入再次执行同一矩阵，不重复计工作量。
完整工作区/compat/原生音频使用各自真实 Cargo 命令；此工具不是完整 Actions 工作流的替身，Root 专项夹具仍应另列实测。

默认2个构建任务；依赖准备600秒、直接矩阵900秒、Cargo测试1800秒、严格Clippy1200秒上限。
超时杀掉该步进程组并保留失败。依赖准备失败停止依赖它的步骤，其他测试失败保留各自真实退出码。
没有自动重试产品动作、过滤测试、清空业务库或改变 schema/权限/产品超时。

## 结果

每次运行在本机私有结果目录保存 `source.json`、逐步 `.log`、`summary.json` 和 `evidence.zip`。
运行开始打印结果位置；`evidence.zip` 只含来源元数据、摘要和日志，不含测试数据库、密钥、工作区或缓存。
日志以具体源对象、UID/内核、命令、退出码和实耗核对；只有本次所有步骤实际通过才为 `passed=true`。
未启动、缺工具、源码错误、失败、超时和未覆盖范围必须原样记录，不能用控制器单测或安装成功代替业务验证。

适用矩阵及产品边界见[离线矩阵](offline-matrix.md)、[备份范围](backup-scope.md)和[一致性缺口](consistency-gaps.md)。
本入口的实际运行结果只认[本人交付报告](../deliveries/member-c-nas.md)中的对应源 SHA 证据。
