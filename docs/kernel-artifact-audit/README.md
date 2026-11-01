# C17 版本、来源与证据清单

C18只读实现与输入/退出语义见[使用说明](usage.md)。本页保留C17来源截面，
后续实现/验证状态以使用说明和成员报告为准，不能由历史提案的“待审核”文字推断当前合并状态。

固定main `6b5de5f3ca5ff706adacd8f25ed7f638546739a9`。C17来源清单交付候选，待负责人按精确HEAD审核；没有实现C18制品检查器。

| 对象 | 已记录值 | 本轮结论及限制 |
|---|---|---|
| 目标板与compatible | `ufi003 / thwc,ufi003 / qcom,msm8916` | board.toml声明；本轮没有解析实际DTB的根compatible |
| 架构标签 | `aarch64 / arm64` | board与base等原锁的兼容标签；本轮没有据此推定实际kernel/模块二进制架构 |
| 基础内核release | `6.12.49-msm8916-g93a71ee9468d` | board/base/profile/audio/旧上游入口一致；并非读取运行内核 |
| 内核来源commit | `93a71ee9468dd8d47a9e082e5d818608f08712e3` | 六处固定来源一致；不是本轮远端签名核验 |
| 内核Git tree | `72d4e8e42649f763dd969a1de893051ed64387a1` | 四处声明一致；未读取245MB完整源码归档 |
| 公共源归档 | `b252cc0e2f9ada0eca126b1df3cfca71a813ac0cc97db5e2a83b4f0260908a60` | 与board声明相符；实际归档尚未校验 |
| 上游boot.img | `874628988a083f3733e3061cc84c5717ce8e3e61d7b081bf068d03432080158c` | 公开上游输入，不能与改写root selector后的板级boot摘要混比 |
| 板级boot | `d5790e07a8ee67c6d80ec24ea9b0cd4b5a78ab4440a3de2b8767fea58a03eaee` | 与base锁一致；verify-audio-boot-recipe按64MiB零填充重构 |
| 基础Image | `3474aab48d9ff0c9aecfd70201d932a0520f0c93ad416044711ab2644ec8b004` | profile记录的未压缩内核；不是完整boot镜像 |
| 压缩内核 | `1fbbef6ccaff103af9dc3281aacaa3d81e82e001d5957c4fa6a018faed1f767f` | profile记录的压缩内核；不是DTB或initramfs |
| 输入DTB | `5c5a12a8237af1d5451774f497dbd75f0c610134db8ec5d9590d741871afab19` | 公开输入摘要；本轮无DTB字节或compatible解析 |
| 音频DTB | `afb1027cc0371d491349dba868bdbabb927ac87c71f8a447dc504303f93f7425` | 预期输出摘要；5项源文件本轮逐字节匹配锁 |
| 原装模块归档 | `7e1d5d53a92ae6eb7f604cb98681b5132bc41071118a6230978354ad5b74c4e8` | 归档摘要；不是下行清单摘要 |
| 原装模块清单 | `1552358f4e2f63ae0aefc364ade342200269fb673c11c3ad4a4bc3e39f169779` | 与归档SHA是不同对象；profile声明681项，未核每项实际模块 |
| 附加音频模块 | `22项声明 / 四预加载目标的16项闭包` | 22项vermagic声明一致；APR等外部stock依赖仍待真实模块确认 |
| 厂商/设备固件 | `摘要、再分发许可：未核验` | 未读取任何私有固件或设备资料；不能按内核GPL自动推定固件许可 |

## 实际来源

[firmware/board.toml](https://github.com/Viper-Boss/RustSimHub/blob/6b5de5f3ca5ff706adacd8f25ed7f638546739a9/firmware/board.toml#L1)

[firmware/base-system.lock.json](https://github.com/Viper-Boss/RustSimHub/blob/6b5de5f3ca5ff706adacd8f25ed7f638546739a9/firmware/base-system.lock.json#L1)

[firmware/kernel/upstream-source.lock.json](https://github.com/Viper-Boss/RustSimHub/blob/6b5de5f3ca5ff706adacd8f25ed7f638546739a9/firmware/kernel/upstream-source.lock.json#L1)

[firmware/kernel/base-build-profile.json](https://github.com/Viper-Boss/RustSimHub/blob/6b5de5f3ca5ff706adacd8f25ed7f638546739a9/firmware/kernel/base-build-profile.json#L1)

[firmware/kernel/audio/source.json](https://github.com/Viper-Boss/RustSimHub/blob/6b5de5f3ca5ff706adacd8f25ed7f638546739a9/firmware/kernel/audio/source.json#L1)

[firmware/kernel/audio/modules/source.json](https://github.com/Viper-Boss/RustSimHub/blob/6b5de5f3ca5ff706adacd8f25ed7f638546739a9/firmware/kernel/audio/modules/source.json#L1)

所有JSON均为从已有锁派生的研究视图，不作为另一套构建/发布manifest；后续C18仍必须复用原输入与已有校验函数。

## 模块身份与依赖（原锁声明）

以下从原 modules/source.json 派生用于阅读，不是本轮实际ELF核验；所有22项vermagic首字段为上述release，尾部为 `SMP preempt mod_unload aarch64`。

| 模块文件 | .modinfo.name | 声明depends |
|---|---|---|
| `q6adm.ko` | `q6adm` | `apr,q6core,q6afe,snd-q6dsp-common` |
| `q6afe-clocks.ko` | `q6afe_clocks` | `q6afe,snd-q6dsp-common` |
| `q6afe-dai.ko` | `q6afe_dai` | `q6afe,snd-q6dsp-common` |
| `q6afe.ko` | `q6afe` | `apr,q6core` |
| `q6apm-dai.ko` | `q6apm_dai` | `snd-q6apm` |
| `q6apm-lpass-dais.ko` | `q6apm_lpass_dais` | `snd-q6apm,snd-q6dsp-common` |
| `q6asm-dai.ko` | `q6asm_dai` | `q6asm,q6routing` |
| `q6asm.ko` | `q6asm` | `apr,q6core,snd-q6dsp-common` |
| `q6core.ko` | `q6core` | `apr` |
| `q6cvp.ko` | `q6cvp` | `q6voice-common,apr` |
| `q6cvs.ko` | `q6cvs` | `q6voice-common,apr` |
| `q6mvm.ko` | `q6mvm` | `q6voice-common,apr` |
| `q6prm-clocks.ko` | `q6prm_clocks` | `q6prm,snd-q6dsp-common` |
| `q6prm.ko` | `q6prm` | `apr,snd-q6apm` |
| `q6routing.ko` | `q6routing` | `q6adm` |
| `q6voice-common.ko` | `q6voice_common` | `apr` |
| `q6voice-dai.ko` | `q6voice_dai` | `q6voice` |
| `q6voice.ko` | `q6voice` | `q6cvs,q6mvm,q6cvp,q6voice-common,q6afe` |
| `snd-q6apm.ko` | `snd_q6apm` | `apr` |
| `snd-q6dsp-common.ko` | `snd_q6dsp_common` | `(none)` |
| `snd-soc-apq8016-sbc.ko` | `snd_soc_apq8016_sbc` | `snd-soc-qcom-common` |
| `snd-soc-qcom-common.ko` | `snd_soc_qcom_common` | `(none)` |

四项PRELOAD为 `q6afe-dai`、`q6asm-dai`、`q6voice-dai`、`snd-soc-apq8016-sbc`，对应16项音频闭包；
外部stock依赖 `apr` 未由这些22项实际核验。不能把该闭包说成681项原装模块完整匹配。

## 来源、阶段与复现

[来源索引](source-index.json)固定31文件的字节数/SHA256/Git blob；[来源交叉结果](source-checks.json)记录19项声明一致性与7项跟踪源文件字节匹配。
这些是来源证据，不是运行内核、签名、固件许可或发行成功证明。
[接口提案](proposal.md)及[最小样例](minimal-failures.json)供下一阶段审核。

默认只列计划：`python tools/kernel-artifact-audit/test-source-examples.py`。
普通用户Linux隔离源码运行：`python3 -B tools/kernel-artifact-audit/test-source-examples.py --run`。
该入口只执行5项固定来源/现有解析器样例，不是C18检查器或完整C19混版矩阵；实际命令/来源/结果见[成员报告](../deliveries/member-c-nas.md)。

来源许可保留：内核/音频工具的原GPL-2.0-only及SPDX，测试脚本AGPL-3.0-only；未再分发厂商固件，许可未知保持未知。
