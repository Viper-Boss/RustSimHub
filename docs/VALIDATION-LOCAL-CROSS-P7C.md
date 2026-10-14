# 本机交叉编译验收

2026-10-14。构建环境优化不增加功能进度：当前仍为 62.5 点、约 63%。

此前 Rust 编译在 NAS 的原生 ARM64 容器完成。NAS 为 4 核 Cortex-A55，构建容器限制 2 核。现已验证本机 24 核 Intel Core Ultra 7 270K Plus 的 Ubuntu WSL，默认 8 个构建任务；棒子继续承担硬件验证。

## 源码与结果

- 使用精确已提交稳定源码 `8f4d15b70ee7ac380d0404cdc770e05a3e298fd8`，不是工作区尚未接线的历史查询改动。
- 源归档 SHA256：`0df653f7810727d1d56ab6da3257bc2a0a09ddcf41a6999a058393750cf6eaff`。每个输入与该 Git commit 比较一致。
- Rust 1.98.0、GNU aarch64-linux-gnu-gcc 15.2.0；GNU 包按官方 APT 元数据大小和 SHA256 验证，私有解包，不安装 NAS/棒子/WSL 全局包。
- `cargo test --workspace --all-features --locked -j 8`：151 项通过。
- `cargo build --release --workspace --no-default-features --locked --target aarch64-unknown-linux-gnu -j 8`：六个生产程序通过，全部 ELF AArch64；glibc 依赖不高于棒子 2.41。
- 最后一轮 ARM release 09:21:43—09:22:51 UTC，用时约 68 秒，复用了前面失败构建留下的依赖产物。不能当作首次全量构建或与旧 NAS 不同源码构建的严格速度对照。
- 六个程序在 NAS 的隔离 Debian 13 ARM64 容器，以及棒子的隔离非 Root 临时服务中运行 `--build-info`：全部源 hash 一致，Core development_fixtures=false。
- 两端 Core 无硬件 self-check 与 CLI help 通过。临时容器移除、临时服务停止，旧四服务 active、全部 PCM closed；没有拨号、短信或蜂窝测试。

匹配设备系统的 NAS 运行镜像：`debian@sha256:a29215f6a35e51e22adffa17f89e9d2ef06214e64a2bad10d765c46aea49f11f`，Architecture=arm64。没有将 NAS 宿主机或旧构建镜像的 glibc 升级。

## 失败证据与边界

最初 Zig 链接器不支持 Cortex-A53 修复选项，已改用 GNU 工具链，没有移除该修复。该 WSL 的 `/tmp` 为 tmpfs，跨会话工具链被清空；随后改为私有持久缓存。GNU binutils 的私有共享库路径也已补齐，增加实际 C 编译预检。

旧 NAS Rust 构建镜像的 glibc 低于本次设备产物需要的 2.39，首次运行检查拒绝；失败保留，使用与设备匹配的 Debian 13 镜像后通过。本机产物不声明支持所有旧版 NAS 宿主环境。C/C++ 音频适配仍使用既有 NAS 构建流程。

这些检查证明交叉构建、动态库兼容启动和无硬件自检；没有重验全部短信/电话/网络行为，没有安装生产启动服务或刷写系统。历史查询 WIP 仍未提交、未构建、未部署。

本地产物：`.local/wsl-artifacts-8f4d15b70ee7ac380d0404cdc770e05a3e298fd8`，包含六程序、SHA256SUMS、build.json。完整日志 `.private/wsl-build-8f4d15b70ee7ac380d0404cdc770e05a3e298fd8.log`，exit=0；两端运行报告 `.private/local-arm-runtime-8f4d15b-second.json` passed=true、cleanup_verified=true。首次 SDK/运行失败报告同目录保留。

操作见 [local-cross-build.md](local-cross-build.md)。之后使用本机 Linux 测试与交叉编译，并在 NAS/棒子验证实际 ARM 行为；不重复在棒子上做编译工作。
