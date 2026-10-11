# RustSimHub 接手入口

先读 PROGRESS.md、HARDWARE-BASELINE.md、architecture-foundation.md，
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

## 实机烟测边界

tools/device-smoke.sh 使用 `/var/tmp/rustsimhub-core-p0`，
启动 `rustsimhub-probe-p0` 临时 systemd 单元，最长 300 秒。
仅允许旧系统 nassimhub 用户读取 health/capabilities，完毕停止单元。
没有安装开机服务。异常退出后可执行 `systemctl stop rustsimhub-probe-p0`。

## 重要未完成项

- 系统恢复入口、完整备份和升级断电验证尚未完成；禁止直接替换现有 rootfs/boot。
- domain 中短信对象没有持久化保证；禁止绕过数据库阶段接入 Send。
- call grant 不是认证 token；禁止直接暴露给未认证远端使用。
- P0 的 MM 能力探测不代表来电/发短信/双向声音已验收。
