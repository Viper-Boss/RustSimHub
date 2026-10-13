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
