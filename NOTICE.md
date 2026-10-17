# 来源与许可

RustSimHub 的新增源码使用 AGPL-3.0-only；详见 LICENSE。

2026-10-15 `firmware/system/` 的固件加载/radio/USB及相关单元和
`tools/system/` 对应隔离测试复用 AirSimHub 同一提交
`ea019485e5d890a96acae357c52b94c746de837f` 的 AGPL-3.0-only 板级集成代码。
原始路径、Git链接解析后的来源及摘要见 `firmware/system/reuse-provenance.json`。
调整挂载路径与依赖、使用持久USB标识；新增私有目录准备与内部树组合工具。
没有复制旧Go Agent或分发厂商固件、NV、校准、身份、设置码和维护密钥。

2026-10-15 `tools/rootfs/` 复用 AirSimHub 提交
`ea019485e5d890a96acae357c52b94c746de837f` 的基础镜像构建、验签/审计工具及核心测试，
逐文件原始 SHA256 见该目录 `reuse-provenance.json`，修改范围见 README。
`scripts/`、`firmware/` 保留 AGPL-3.0-only；Android boot 解析辅助工具
`kernel/audio/replace-appended-dtb.py` 按原项目许可证映射保留 GPL-2.0-only，
全文为 `licenses/GPL-2.0-only.txt`，不由项目 AGPL 覆盖。
顶层 `firmware/base-system.lock.json` 是实际签名 Debian 快照的包元数据，不包含包制品；
Debian 包及内核/模块仍保留各自许可证，完整制品发布须交付相应来源与版权材料。

2026-10-14 NAS 兼容契约参考 AirSimHub 提交
`ea019485e5d890a96acae357c52b94c746de837f` 的
`node/proto/auth.go`、`identity.go`、`compat.go`、`node.go`（AGPL-3.0-only）。
Rust 实现规范字节、公钥标识、版本方向与有限只读权限映射；没有启用旧协议网关。
`tools/compat-golden/main.go` 通过未修改的旧 proto 模块生成纯合成向量，
不复制真实设备身份、NAS 私钥、SIM 或消息数据。Go 模块仅用于对照验收。

离线历史迁移的表/列契约参考本工作区 NasSimHub / ModemDeck 的
`internal/platform/database/schema.sql`。仅使用数据格式与字段事实；Rust 导入器和合成
测试 schema 为新增实现，不复制该项目 Go 业务代码、认证实现或密码/会话资料。
原项目 PolyForm Noncommercial/AGPL 许可证继续有效；不能将完整旧源码、数据库或
应用直接重新许可为本项目许可证。完整旧 schema 仅在私有验收目录生成合成快照。

设计和硬件接入行为参考了本地 AirSimHub 项目
`nassimhub-ufi003-firmware-public`：AGPL-3.0-only。保留其来源：
https://github.com/Viper-Boss/AirSimHub
（已通过本地 origin 核对）。本阶段复用开发计划及许可证文本；
没有复制或分发基带固件、NV、射频校准数据、密钥或用户数据。

Linux 内核及相关驱动补丁应保留各自 GPL 许可证；不能用此项目许可证覆盖它们。
NasSimHub / ModemDeck 的 PolyForm Noncommercial 与 AGPL 部分必须逐文件核对，
不得直接整体复制并重新许可。后续复用源码需在本文件补充路径、提交及原许可证。

2026-10-17 `firmware/kernel/audio/ufi003-audio.dtsi`及四个`include/dt-bindings/sound`头文件
直接保留AirSimHub提交`ea019485e5d890a96acae357c52b94c746de837f`的原始Git内容，
来源路径、Git blob与SHA256见同目录`source.json`。默认GPL-2.0-only，文件已有SPDX声明保持原文，
许可证见`licenses/GPL-2.0-only.txt`；不受本项目AGPL重新许可。新增DTB重建工具为AGPL-3.0-only。
不包含boot二进制、厂商固件、NV或校准数据；重建步骤不写设备。

`tools/verify-audio-boot-recipe.py`及其板级夹具以GPL-2.0-only许可，复用仓库已有的
GPL-2.0-only Android boot v0解析/重建器`tools/rootfs/kernel/audio/replace-appended-dtb.py`，
新增固定root标签与只输出摘要报告的核验边界；不受本项目AGPL重新许可。

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

2026-10-17 `firmware/kernel/audio/modules/nassimhub-incall-audio.patch`及`Makefile.outoftree`
保留AirSimHub同一提交ea019485e5d890a96acae357c52b94c746de837f的原Git内容；原路径/Git blob/SHA与固定内核源码、工具链和模块输出锁见同目录source.json。
Linux/ASoC补丁按GPL-2.0-only保留，文件已有SPDX及内核声明继续有效，许可证见licenses/GPL-2.0-only.txt；不受本项目AGPL重新许可。没有提交内核模块二进制或设备状态。
