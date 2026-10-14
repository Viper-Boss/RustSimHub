# 本机 ARM64 构建

本机 Ubuntu WSL 的 x86_64 Rust 编译器负责 Linux 测试和 ARM64 交叉编译。NAS 保留原生 ARM 验证，棒子负责实际 modem、PCM、网络与权限验证。编译成功不代替硬件验收。

Rust 与工具包保存到项目 `.local/wsl-cross`，不修改全局 PATH。GNU ARM 工具链按 Ubuntu 当前官方 APT 元数据中的版本、大小和 SHA256 下载，只解包到当前 WSL 用户 `~/.cache/rustsimhub` 私有目录，位置记录在 `gnu-root.txt`。Rust 固定 1.98.0，目标为 `aarch64-unknown-linux-gnu`。生成程序使用 Cortex-A53 目标，保留 Rust 的 Cortex-A53 修复选项；构建脚本拒绝需要 glibc 高于当前棒子 2.41 的 ELF。实际设备运行及其他动态库符号仍须验证，不能把 Ubuntu 交叉 sysroot 版本当作设备版本。

在 Ubuntu WSL 中运行：

```sh
sh tools/wsl-prepare-cross.sh
sh tools/wsl-build-committed.sh /absolute/source.tar.gz FULL_GIT_COMMIT SOURCE_SHA256
```

构建脚本核对归档 SHA256、每个输入文件与指定 Git commit 的内容；不会使用工作区里未提交的源文件。全工作区 Linux 测试使用 `--all-features --locked`；ARM64 release 使用 `--no-default-features --locked`，开发夹具不进入生产 Core。默认 8 个构建任务，可通过 `RUSTSIMHUB_BUILD_JOBS` 选择 1/2/4/8/12。

源码和编译 target 使用 WSL `~/.cache/rustsimhub` 私有目录，减少跨 Windows 文件系统的编译 I/O；路径记录在 `.private/wsl-build-workdir.txt`。该环境的 `/tmp` 是 tmpfs，WSL 退出会清空，不能用于跨会话保留工具链或编译缓存。成功后六个生产程序、SHA256SUMS 与 build.json 保存到 `.local/wsl-artifacts-COMMIT`，已有产物目录不覆盖。正式部署还须在 ARM 主机核对各程序 `--build-info` 的源归档 hash、Core fixtures=false，以及运行失败路径。脚本不会安装或启动设备服务。

这个流程覆盖 Rust 工作区。`native/audio-alsa` 的 ALSA/WebRTC C/C++ 原生适配单独构建，当前仍使用已验证的 NAS 环境；没有把它算作本机交叉编译已通过。
