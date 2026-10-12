# RustSimHub 接手入口

先读 PROGRESS.md、VALIDATION-RECOVERY-CLI-P1.md、recovery-clients.md、VALIDATION-AUTH-P1.md、auth.md、VALIDATION-STORAGE-P1.md、storage.md、VALIDATION-P0-P1.md、HARDWARE-BASELINE.md、architecture-foundation.md，
再按 development-plan.md 的阶段顺序继续。GitHub：
https://github.com/Viper-Boss/RustSimHub（私有）。

## 私有资料

原开发机仓库根目录 .private/ 包含 nas.json、device.json、原始日志。
这些文件被 Git 忽略。NAS 密码不写进任何可提交文件。
device.json 引用旧工程维护密钥和固定 known_hosts，通过 NAS 转发至 USB 设备。
在新机器接手时需要安全取得这些材料；不要关闭 SSH 主机密钥校验。

.local/python-deps 为本地 Paramiko，.local/cargo 为 Rust 缓存。
原开发机 Windows 缺少 MSVC 链接器，优先使用已建立的 NAS ARM64 构建环境。

## 常用操作

从项目根目录运行：

```powershell
python tools/remote.py --config .private/nas.json --script tools/nas-baseline.sh --output .private/nas-baseline.log
python tools/remote.py --config .private/device.json --script tools/device-baseline.sh --output .private/device-baseline.log
python tools/remote.py --config .private/nas.json --sudo --script tools/nas-builder-status.sh
python tools/package-source.py
```

NAS 构建目录 `/mnt/docker-data/rustsimhub/p0`；容器 `rustsimhub-build-p0`。
源包只有 Cargo 清单、锁文件、许可证和 crates 源文件。容器以 UID/GID 1000 运行，
限制 2 CPU、2 GiB 内存；不提供设备、host network 或主机控制 socket。
旧 NAS 容器和旧棒子服务不属于此新仓库的可删除资源。
重新构建前先确认该构建容器已停止、保留 build.log 及产物，再仅移除
`rustsimhub-build-p0` 容器（不加 -v，不删除源目录和缓存）。
随后上传新的允许列表源包并执行 tools/nas-start-build.sh；脚本会拒绝复用既有容器名。
构建镜像已固定 digest，避免未来同名标签变化导致不可追踪的构建环境。

boot 备份位置：本机 `.private/backups/boot.img`，NAS
`/mnt/docker-data/rustsimhub/private-backups/p0/boot.img`，棒子临时副本
`/var/tmp/rustsimhub-p0-private/boot.img`。三份均为 64 MiB，
预期哈希在 firmware/board.toml；本机与 NAS 已复核。
不要重复执行 tools/device-backup-boot.sh：它会拒绝覆盖既有备份。
完整恢复备份仍未完成，boot 单独备份不能恢复用户数据或整个系统。

## 实机烟测边界

tools/device-smoke.sh 将 `/var/tmp/rustsimhub-core-p0` 安装到
`/opt/rustsimhub/dev-p0/rustsimhub-core`（避免被 PrivateTmp 隐藏），
启动 `rustsimhub-probe-p0` 临时 systemd 单元，最长 300 秒。
仅允许旧系统 nassimhub 用户读取 health/capabilities，完毕停止单元。
没有安装开机服务。异常退出后可执行 `systemctl stop rustsimhub-probe-p0`。
原 P0 ARM64 release 产物留在本机 `.local/artifacts/rustsimhub-core`、NAS 的
`history/p0-first/rustsimhub-core`、棒子 `/opt/rustsimhub/dev-p0/rustsimhub-core`。
哈希和实测资源值见 VALIDATION-P0-P1.md。

SQLite 持久化/故障恢复已验证，实现见 storage.md。设备身份、公钥配对、短期会话和
core 鉴权基础也已验证，见 auth.md；禁止添加未鉴权的业务写接口。
存储诊断命令只在 dev-fixtures feature 下提供，测试只使用合成数据。
tools/device-storage-smoke.sh 使用 dev-p1 独立产物及随机私有临时目录，
不会读写原项目业务数据库。测试库留在棒子上，可用于后续排查。
NAS 官方精简构建镜像没有 Clippy；Clippy -D warnings 在 GitHub CI（Rust 1.98）执行。
旧存储 P1 产物在本机 `.local/artifacts/rustsimhub-core-p1`、NAS `history/build-*` 归档、
棒子 `/opt/rustsimhub/dev-p1/rustsimhub-core`，哈希见 VALIDATION-STORAGE-P1.md。
后续重新构建使用 tools/nas-reset-stopped-build.sh：校验已退出、标签、镜像和挂载，
归档后只移除本工程构建容器；保留缓存及全部旧服务。
上一身份包源码检查点为 5389eb8；Store 已绑定 --serve-authenticated 的 daemon 生命周期。
旧鉴权 P1 产物在本机 `.local/artifacts/rustsimhub-core-auth-p1`、NAS `history/build-*` 归档，
棒子 `/opt/rustsimhub/dev-auth-p1/rustsimhub-core`。
工具 tools/device-auth-smoke.sh 仅使用新的随机私有目录与合成 owner/client；不读取旧数据库。
它以 nassimhub 非 root 用户启动 rustsimhub-auth-p1 临时单元，最长 180 秒，测试后已停止。
异常时停止该单元，禁止把 fixture 的已知密钥用于正式设备 owner。
真实 owner 尚未初始化。simhubctl 及可信离线恢复已完成，最终用户安全设置 UI 还待开发。
恢复 DB 会隔离 owner；用原 owner 私钥及 --recover-owner 重新证明，不能删表重新 bootstrap。

本轮源码检查点 7f69dd8；先读 recovery-clients.md 的命令、pin 与权限边界。
最新 core/CLI 在本机 `.local/artifacts/{rustsimhub-core-recovery-p1,simhubctl-recovery-p1}`，
NAS 当前 `src/target/release/{rustsimhub-core,simhubctl}`，棒子
`/opt/rustsimhub/dev-recovery-p1/{rustsimhub-core,simhubctl}`，哈希见验证文档。
tools/device-recovery-cli-smoke.sh 只生成新的私有合成目录，使用 FIFO 协调离线证明，不需要 Python/jq。
临时 rustsimhub-recovery-p1 单元最长 180 秒，最终已停止；异常时仅停止此项目测试单元。
SIGTERM 清理自身 socket；SIGKILL 后持锁确认旧 socket 不再监听才接管；就绪需真实 IPC 握手，不能只测 -S。
客户端重新批准要求 fresh proof 和精确 generation；不让旧 token 复活。
--backup-identity 保存独立明文私有 seed，拒绝覆盖；先加密/安全保管后才能作为正式异机备份方案。
NAS 构建还检查 workspace 默认 feature；CI 文件因 workflow scope 限制保留原配置，测试/Clippy 均通过。

Git HTTPS 偶尔被连接重置。当前机器 `.local/api-push.py` 是备用方式：只允许远端为
本地 HEAD 的父提交、比较创建树与本地树、不强推；GitHub 规范化提交时验证对象后同步本地 ref。
首次 bootstrap 和数据代码仍完整保留历史。不要复制任何凭据到源码。
tools/verify-build-source.py 检查当前源包与 HEAD 字节，tools/audit-source.py --revision HEAD
可检查已提交树的指定隐私模式；这不替代完整依赖/漏洞审查。

## 重要未完成项

- 系统恢复入口、完整备份和升级断电验证尚未完成；禁止直接替换现有 rootfs/boot。
- domain 中短信对象仍是纯规则；真实 Send 必须通过已提交的 storage.begin_submit，且先完成身份/权限校验。
- call grant 不是认证 token；禁止直接暴露给未认证远端使用。
- P0 的 MM 能力探测不代表来电/发短信/双向声音已验收。
- TLS/密码、最终用户设置 UI、生产系统单元、加密/异机身份备份与完整硬件恢复还未完成。
