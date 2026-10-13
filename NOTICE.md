# 来源与许可

RustSimHub 的新增源码使用 AGPL-3.0-only；详见 LICENSE。

设计和硬件接入行为参考了本地 AirSimHub 项目
`nassimhub-ufi003-firmware-public`：AGPL-3.0-only。保留其来源：
https://github.com/Viper-Boss/AirSimHub
（已通过本地 origin 核对）。本阶段复用开发计划及许可证文本；
没有复制或分发基带固件、NV、射频校准数据、密钥或用户数据。

Linux 内核及相关驱动补丁应保留各自 GPL 许可证；不能用此项目许可证覆盖它们。
NasSimHub / ModemDeck 的 PolyForm Noncommercial 与 AGPL 部分必须逐文件核对，
不得直接整体复制并重新许可。后续复用源码需在本文件补充路径、提交及原许可证。

2026-10-13 音频实现参考 AirSimHub 提交
`ea019485e5d890a96acae357c52b94c746de837f` 的
`node/agent/internal/voicealsa/alsa.go` 与 `node/deploy/audio/hold-cs-voice.c`
（AGPL-3.0-only）：保留七项路由名称、控制 PCM 及媒体 PCM 的板级约定，
Rust 重新实现句柄生命周期及有限格式检查，没有复制 Go/C 实现。
`alsa` 0.12.1 是 MIT/Apache-2.0 上游封装；内部 FFI 不属于本仓库的 safe Rust 代码。
构建用 libasound 及 Debian 包的原许可证继续有效；包版本与校验值见
`tools/audio-builder-deps.json`，不能将它们重新许可为 AGPL。

2026-10-14 编解码使用 `opus` 0.4.0（MIT/Apache-2.0）和 `opusic-sys` 0.7.5（BSD-3-Clause）。
后者在已锁定 Cargo crate 内携带 Opus 1.6.1 源码，默认静态构建，保持 hardening、stack protector 与 fortify，关闭 DRED/OSCE。
Opus 原始版权、BSD 条件/免责声明和专利许可链接保留在 `licenses/Opus-COPYING.txt`；发布静态媒体二进制时须同时交付。
构建工具 CMake/Ninja 及其 Debian 依赖仅进入隔离镜像，不随此声明重新许可；运行时不要求给棒子安装 libopus。

2026-10-14 WebRTC 使用 str0m 0.24.1、dimpl 0.7.5 与 rust-crypto provider（上游 MIT/Apache-2.0）；
证书生成的 transitive rcgen/AWS-LC 仍包含 C 依赖和上游 Apache/ISC/OpenSSL 等许可证。
不能将调用侧 forbid(unsafe_code) 或 rust-crypto feature 宣称为所有传递依赖均为 Rust。
完整应用发布仍须携带 RustSimHub LICENSE、Opus COPYING 和实际静态依赖的第三方 notices；开发诊断包尚不作为完整发布。
