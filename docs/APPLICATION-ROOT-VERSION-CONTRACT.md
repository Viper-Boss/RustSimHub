# 应用、Root维护配置与持久数据的版本契约

2026-10-27更新：[U1阶段一](VALIDATION-APPLICATION-CONTRACT-U1.md)已实现Core/worker实际构建能力描述和严格解析，257项及实际native生产CLI通过；Root/数据/候选比较、维护准入、ARM/设备仍未验收，UPD-01继续进行。

2026-10-27，UPD-01启动盘点。状态：下列现状已对照源码；整体兼容选择/回选门禁尚未完成，不据本文把UPD-01或FWD-20计完成。本文不是设备升级指令。

## 四种版本分别处理

| 层 | 当前权威来源 | 已有约束 | 尚需补齐 |
|---|---|---|---|
| 签名应用组 | `crates/update/src/lib.rs`、`tools/application_package.py` | manifest格式1是固定五文件，格式2是固定六文件；板型/架构/主库schema范围/epoch/source/精确载荷签名均验证 | 普通manifest尚没有Root助手/转发outbox的完整兼容声明；不能把主库schema6等同全部数据兼容 |
| Root安装配置 | `firmware/system/setup-ap/contract.json`、`crates/core-launcher/src/lib.rs`、`tools/setup_ap_package.py` | 热点联合契约格式1；八文件维护包格式2；Core/Edge/AP/launcher实际程序同源/摘要/契约匹配 | 需统一表达与回选应用的兼容判断，明确拒绝未知组合 |
| 业务数据库 | `crates/storage/src/lib.rs`、更新器manifest的schema_min/max | 当前schema6；业务数据独立于选定程序目录 | 应用组选择/回退不会回滚或替换短信库；后续迁移和恢复由UPD-06、REC及成员短信任务分别验收 |
| 转发数据库 | `crates/forwarder/src/queue.rs`、`queue/history.rs` | 当前格式3；在设备/Owner绑定与完整性检查后接受1/2迁移，in-flight重开为Unknown，prepared过期 | 旧应用无法理解3；必须显式声明可读/可写/迁移范围，不能反向改user_version或删除Unknown以求兼容 |

应用版本epoch、普通manifest格式、Root维护包格式、主库schema和outbox格式不是同一个数字。

## 当前Core启动选择的实际范围

启动器固定执行`/opt/rustsimhub/selected/rustsimhub-core`，读取有界16384字节/3秒的`--build-info`，实际UID/GID986；模式和参数均由程序固定，不执行元数据提供的shell/路径/argv。

| Core档位 | 源码归档标识/能力 | 固定模式 | 边界 |
|---|---|---|---|
| 原厂16 | `9bbc71d42fb5b10d0a2346579dccc56f6d732828689c2879fed48b1b796a5d39` | `--serve-device`，12个参数 | 不附加转发或热点参数 |
| 转发48 | `d641489c23d019d2c7011f8709c51d2890f0fd3aab4d75ecbb42e0d1158f9cae` | `--serve-device-forwarding-managed`，13个参数 | 不附加热点参数；不据此假定它可读取格式3 outbox |
| 应用49 | `f0226d2b32ec69a7ab1a062516a75dfbe7229b403d8f72210c7c8e2a099896b6` | 两端热点契约精确匹配才联合16参数，否则受管转发 | 未声明的历史构建只按这份实际归档识别 |
| 后续显式声明Core | `service_launch.format=1`、有限三种modes，component/core和实际架构匹配 | 只选择声明的固定模式，热点还需精确契约 | 未知模式/重复模式/错误类型/未知旧档位/开发夹具拒绝 |

这些是软件归档标识，不是设备身份。该表证明命令行选择范围，不能单独证明outbox/Root服务/真实冷启动均兼容。已有测试见`crates/core-launcher/src/tests.rs`及`tools/test-core-launcher.py`，设备历史证据见P4U/P4W；必须保留证据所属提交，不将旧证据移作新源码的ARM/设备证明。

## 固定接口和数据策略

- 普通签名应用组：格式1/2，Ed25519域`RustSimHub.AppManifest.v1\0`，载荷24MiB以内；选定组只写Root限定的版本目录和选择台账，不写boot/rootfs或业务数据库。
- 热点Root材料：独立签名域`RustSimHub.SetupApBundle.v1\0`、八个固定目标；打包准入要求Core/Edge/AP/launcher同源且生产feature，主库6、普通应用2、启动器1、联合契约完全一致。
- 联合热点契约当前为auth2/database6/application2，parent/trial/commit/material/owner_adoption均1，固定Core和Root模式；最大窗口300秒。契约缺失/畸形/权限不可信时不启用热点，不靠新参数破坏旧Core基础业务。
- 转发worker协议4、独立UID983、systemd只读凭据、固定Root动作。Root服务从选定六文件组取程序，开关OFF不运行worker；未知版本不能靠启动旧worker“试试看”。
- 新outbox格式3只在Owner和完整性验证后迁移1/2；现有格式3中的Unknown和已确认回执不得因换程序消失，代码回退与数据回退分开。
- 候选选择器验证已确认基线仍符合它理解的签名/schema要求；但当前这个检查尚不覆盖全部outbox/Root语义。这一缺口必须明确，不用既有绿色测试冒充已封闭。

## UPD-01接下来的交付

1. 从实际Core/forwarder/AP/launcher元数据形成明确、严格类型、有界的兼容描述，包含outbox可读/迁移/写入格式以及对应协议，不能以文件名或版本号猜测。
2. 给每个允许回选的历史档位固定Root启动模式和能力；未知历史档位拒绝，不默认为最新能力。
3. 对当前Root安装契约、选定/候选应用和实际持久数据版本做只读比较；输出可选择/需维护/拒绝及具体原因，不自动改开关、身份、短信DB或outbox。
4. 用真实历史元数据和生产程序输出作正对照，测试错误/缺失/重复字段、未知格式、只读格式不含实际版本、格式3回选旧应用、Root契约不匹配及错误架构。不能只有手写“新版本”JSON自证。
5. 将检查结果接到有限维护准入；签名材料接线属UPD-02，实际含格式1/2/3的设备回退验收属FWD-20，完整系统组合/掉电/健康/冷启动按原动作保留。

本轮没有刷机、安装Root材料、改设备信任或执行回退。PMIC/关机继续搁置，四位成员的NAS/SMS/NET/统计任务不在此接管。
