# 常驻媒体程序源码与 ARM 构建检查点

本页记录233110a的历史构建；当前e271ca1/621994eb…及生产实机结果见[后续验收记录](VALIDATION-MEDIA-WORKER-P3F.md)。不要将下述旧构建身份改写为新版本。

2026-10-16，冻结源码 `233110ad5af01973bce0950f1bb94377df8520a3`，源码归档 SHA256 `5c33e42400f9ce5e04fbc38ce2175d65aea7deea960aeaebb0910ed155a7352f`。接口和权限边界见 [media-worker.md](media-worker.md)。此检查点不增加进度点数。

- 本机 WSL Rust1.98.0：workspace 193 项测试和 Clippy、Native 17 项测试和 Clippy 通过。仅本机补齐 ALSA 开发头文件、C++/CMake/Ninja；没有在棒子上编译。
- 本机 WSL 六程序 ARM64 生产构建退出0；`.local/wsl-artifacts-233110ad5af01973bce0950f1bb94377df8520a3/build.json` 记录逐文件摘要、工具链与 GLIBC 要求，`development_fixtures=false`、`hardware_tested=false`。
- NAS 固定 ARM64 镜像 `sha256:90158fe893786a51eaad5e126e4915031872974b3b27347b44a4ce41d760ba37`，Rust1.99.0 / ALSA1.2.8：Native 10+7 项测试和生产 release 构建退出0。构建容器无网络、只读根、UID1000、空能力集合，2CPU/1GiB 限额。release 构建3分54秒，不作全量性能对照。
- 同源 Native `rustsimhub-audio-alsa` 为5,451,168字节，SHA256 `0c21880ed11cfdba7b53b48b61b4f4d5df1347e0d6fcd2de1fbf3fc2c6fc8bb9`；下载到 `.local/native-233110a/` 后独立核对摘要。完整日志 `.private/native-233110a-build.log`。

真实电话下的常驻 worker、Core 单次票据和 PCM 生命周期仍待实机验收。HTTP 仍拒绝 IssueMedia，网页信令与麦克风界面尚未接入，`media_available=false`；此构建不是已经启用的音频产品服务。此前签名安装包与组合镜像仍属于各自原始源码，不能以新构建替代其归属。此次没有新增拨号、短信、蜂窝数据或分区写入。

GitHub main 已发布准确的233110a源码；未确认该提交触发新的 CI，不能把本地测试写成远程 CI 结果。
