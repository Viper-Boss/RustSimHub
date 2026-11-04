# 窗口8续接指令：把已交模块接成实际生产入口

2026-11-04最新合并状态：健康206/211、安装209/213、数据210/212均已进入main `5304a96d28f2c76c14ab651938a217dbeae67fa3`；206文档冲突通过214正常解决，独立harness锁仅补一行现有依赖。固定5f1d65bc串行默认497/全特性749、严格检查和四模式同源测试通过；并行锁失败保留，未实机/新ARM/签名部署。窗口8直接从最新main续接，共享生产入口与连续守卫仍由网页补齐，不需重做已合依赖。见[合并与复核记录](../deliveries/web4-delivery-merges-20261104.md)。81.5%保持。

日期：2026-11-04（Asia/Shanghai）。任务归负责人 Viper-Boss 主线；用户本轮指定改用**一个网页版 Codex 窗口继续剩余离线开发**。本机暂停同范围代码修改，保留最终审核、合并、设备签名和部署。不要与另一个窗口同时认领本包。

## 可直接复制给网页版的指令

> 在 Viper-Boss/RustSimHub 中，读取分支 `codex/web-product-delegation-20261103` 的 `docs/team/WEB4-INTEGRATE-RESUME-20261104.md`，再按本文开始窗口8续接。昨天第一阶段 PR208 已合并，不能重做；依赖206/211、209/213、210/212已提交，先核对完整HEAD并在你自己的新集成分支正常合入固定历史，不等它们先合main。完成正式Core/updater入口、安装/迁移连续持锁、有限Root变更重验、兼容profile及可信健康确认、实际发行构建和隔离端到端，分阶段提PR。不要只做报告或库测试再把生产接线交回本机。允许使用自有临时密钥、隔离Root/容器及可用ARM交叉编译/QEMU；禁止访问设备、使用真实密钥、分配epoch、生产签名、部署或自行合并main。完整边界、已完成项和交付条件见任务书。环境缺项逐项记not_run，继续能独立完成的代码；遇到真实授权/数据安全矛盾保留拒绝，不削弱现有门禁。

## 1. 开工先读，昨天的成果直接复用

1. 当前代码checkout的 `AGENTS.md`、`docs/HANDOFF.md`、`docs/PROGRESS.md`、`docs/development-plan.md`。
2. 本文及同一任务文档分支的 `WEB4-INTEGRATE-20261104.md`、`WEB4-DEVELOPMENT-DELEGATION-20261104.md`；本文覆盖两者已过时的“8由本机执行/3、6、7尚未交付”状态，其余原十四步与验收要求保留。
3. main已合并的 `docs/deliveries/web4-production-integration-20261104.md`、`tools/product-integration/`、`tools/product-release/`、`firmware/system/product-profiles/`。
4. 下表各固定依赖分支里的实际源码、README与交付报告。**文档分支只提供任务，不能整条合入产品。**

建议新分支 `codex/web4-production-wiring-20261104`，从真实最新 `origin/main` 建立。不要复用已经合并的208分支改写历史，不清理/覆盖本机脏工作树，不强推他人分支。

昨天已经完成，**不重新派发**：

- 202/203/204/205/207和208已正常合入main；来源工具、NKN退出/重登录控件修复、Windows浏览器runner路径修复均在main。
- 第一阶段最终产品代码 `71b268cb185f39b0d98bdc1c3b83b9236abb7754`；已记录31 Node、Edge41/严格检查、输入工具12、本机Chrome三宽主页与11配网场景。更早AP/恢复组合531 Rust及5项原忽略另列，不能把重复Edge相加。
- PR205有限profile/launcher/构建器已实现，默认13/产品16 ARM是**旧固定源码的历史结果**，不冒充本轮新组合构建。
- 新任务是补共享生产链，不能再次仅交“来源检查完成”或只改 `verified/healthy/installable` JSON字段。

## 2. 已回读的依赖快照与合入顺序

本任务书编写时GitHub实际main为 `a2044edfef6aeeced3d2c2eb8743791b34e4ccdc`。下列六笔均open、非草稿、未合并；这是来源登记，**不是本机已经复跑或审核通过的声明**。开工必须重新读取；HEAD漂移应记录新旧差异并按新完整源码重测，不能静默沿用旧证据。

| 顺序/范围 | PR | 固定完整HEAD | 交付报告（位于该HEAD） |
|---|---|---|---|
| 健康1：Core来源 | [206](https://github.com/Viper-Boss/RustSimHub/pull/206) | `fe65bf3e1fd02375ed6fcd38a595271c33180caa` | `docs/deliveries/web3-trusted-health-core.md` |
| 健康2：Root受限确认，base为206 | [211](https://github.com/Viper-Boss/RustSimHub/pull/211) | `77b2baf88bc7409c72c31b5b08ea6b2aa8bf7ba9` | `docs/deliveries/web3-trusted-health-root.md` |
| 安装1：有限签名材料 | [209](https://github.com/Viper-Boss/RustSimHub/pull/209) | `42dca70ff82b405b787616def708903bb126bad5` | `docs/deliveries/web4-product-install-20261104.md` |
| 安装2：执行器/恢复，含209历史 | [213](https://github.com/Viper-Boss/RustSimHub/pull/213) | `28c38c0bcb5c802c1f223787fa27efc957410ebd` | 同上，阶段二证据 |
| 兼容1：实际模式元数据 | [210](https://github.com/Viper-Boss/RustSimHub/pull/210) | `19f9844fd76ff1597b69a694b2e86dff3561989b` | `docs/deliveries/web4-product-data-compat-20261104.md` |
| 兼容2：数据/迁移协调，含210历史 | [212](https://github.com/Viper-Boss/RustSimHub/pull/212) | `a148e670f8b6a38c0d7acfe4f9636b32a7eeff8b` | 同上，阶段二证据 |

GitHub回读206当前有合并冲突，其余五笔当前可合；这只是当时GitHub单笔状态，**不保证六包相互无冲突**。在你自己的分支解决206与现main的真实冲突，尤其保留203恢复actor、208退出修复及当前业务行为；不覆盖整个文件取一边，不更改依赖原分支。

正常保留历史：206含200（含197）及198，211堆叠206；209→213、210→212。按真实Git祖先关系核验，不对同一代码重复cherry-pick。旧195/196/199仍按原任务8来源表核对：需要的代码纳入同一组合，历史报告保留；201可选首页与52/53/56/69草稿不自动纳入。

更新 `tools/product-integration/inputs.json` 的真实输入及依赖/包含关系，登记209/213、210/212、211，不再保留“模块未交付”的null。来源检查严格模式通过只证明源码组装，所有安装/健康授权仍由真实运行链产生。

## 3. 可改范围与不能重复开发的部分

本轮旧窗口3已交付，两阶段6/7也已交付，**共享接线归本窗口独占**：Core/updater的 `main.rs`、`lib.rs`、必要 `Cargo.toml/Cargo.lock`、原 `admission.rs/programs.rs` 的有界守卫交接API、launcher profile适配、发行构建器与必要systemd材料、集成测试和本包报告。

允许最小适配 `product_materials`、`product_admission`、`runtime_compatibility`、`runtime_observation`，每处注明原因并补同源测试；保留作者和原报告历史。复用实际安装器、原Store/Auth迁移、原邮件3→4适配及原确认账本，不另造一套安装数据库、健康服务、发送器、签名授权或平行升级引擎。AP/恢复actor/网页/转发/NKN/BLE业务只作回归消费；确有组合缺陷时单独小提交，不跨包重排。

## 4. 剩余开发动作与可检查的产物

以下是原十四步剩余工作的执行细化，**不新增169项台账分母，不按本表勾选增加整体验收分**。

### A. 先接正式入口，保证真实程序可调用

1. 在独立分支完成上表依赖整合及冲突核对，记录实际main/base/head/祖先/文件范围；保留PR208阶段一成果。产物：更新后的输入锁、组合完整SHA和冲突决策说明。
2. 按 `tools/product-upgrade/README.md` 导出同源模块、补有限依赖/feature。正式Core在正常启动/激活前处理 `--product-runtime-compatibility MODE`；模式有限、无路径/schema输入，不打开业务数据库。metadata来自正式编译Core，不能用harness输出替换。
3. 正式Core接 `--migrate-product-recovery-child` 和 `--inspect-product-recovery-upgrade`；执行原同源子进程文件、实际feature门禁、UID986/空组/空capabilities/NNP/密封字节/有界握手。普通启动不隐式迁移，无feature/非Root/错误参数在副作用前拒绝。
4. 正式updater导出安装/数据模块并提供固定生产prepare/apply/inspect/recover路由；命令名称可按原CLI风格选择并冻结。生产目标、信任和收件路径固定，不能把隔离test-root或任意路径/UID/unit/shell/调用者自报健康放到生产CLI。用真实CLI正反例证明不是未导出的库。

### B. 解决跨安装、迁移的连续守卫，是本轮关键开发

5. 在原维护API内增加最小有界的守卫保留/交接能力。原停机、签名槽位、信任、实际业务/outbox锁、一致备份、lease/deadline持续有效。**不得 `drop(snapshot)` 后重新 `observe()` 冒充同一事务持续持锁**。明确业务writer仅在原迁移握手中转交给原Store的时点，其他守卫仍被持有。
6. 给安装器实现 `LiveInstallationFence`：每个Prepare/逐文件写入前后/Finalize/回退都验证事实；安装合法改变Root时，仅接受实际执行器借出的 `OwnedRootChanges`、事务manifest、固定目标与原/新inode/摘要/权限证据。拒绝扩大到整个 `/etc` 或忽略Root检查；非本事务变更、信任/NSS/slot/DB换代照常失效。原Root合同、updater与信任仍不在产品包写集。
7. 将实际材料验签、固定角色策略/NSS预检、停业务和一致备份、有限安装器、业务重验、产品选择接成一个可观察事务；失败落原journal/Unknown，不清记录、不重置attempted/floor/预算。不在运行中复制数据库冒充一致备份。重开恢复必须新获合格维护守卫，再以留存签名/真实事务证据恢复，不能把旧JSON当存活能力。
8. 改原邮件适配与协调器的有界API，使需要继续的调用保留/返回**同一个实际DataFence**；保留旧调用语义并补新正反例。原适配目前消耗snapshot且不返回，不能在外层重新观察解决。原SMTP5/outbox4三方限制仍生效，普通worker4 baseline不得靠JSON升级为可兼容。
9. 在同一事务生命周期接实际产品数据准入及必要迁移：邮件既有base6前提意味着两种迁移都适用时邮件先于恢复；恢复复用6→7→9原实现、`RecoveryMigrationHandoff`、原child和原撤权语义。默认不迁移；不支持组合明确拒绝且不修改业务。每次写后只按实际结果刷新被证明改变的有限事实，不“全部重新采样后继续”。

### C. 解决profile、健康及发行材料的真实兼容

10. 先列真实支持矩阵：profile/启动前缀/编译feature/schema/outbox/worker/Root材料/回退程序/健康协议。当前211健康profile**仅支持显式schema6/format2/forwarding**，只有原30-forwarding和55-update-health覆盖；AP/provider/recovery覆盖及schema7/9当前不支持。不能把205的product/email/recovery drop-in直接叠上并声称可自动确认。
11. 将兼容矩阵落到真实CLI、launcher、构建器和Root准入：至少完成默认保持及一条明确支持的产品完整成功链。需要让健康覆盖新产品模式时，在原健康生产者和Root准入中做明确有限、必要时版本化的扩展，使用实际新模式metadata、真实Auth/Store/worker代次并补正反例；不能只删drop-in限制、改schema数字或把原只读 `may_confirm=false` 改true。不能完成的组合明确unsupported并留接续，不宣称所有profile全通。
12. 消费原 `--enable-update-health-candidate`、固定 `/run/rustsimhub/update-health/core.sock` 和 `rustsimhub-update confirm-running-candidate`；保留原Auth/controller/SQL lease、同一个flock转换、最后Recheck及原state.next/rename/fsync语义。旧baseline不支持prefix就走原显式维护升级或准确拒绝，不能调用裸confirm代替。健康timer只在显式候选路径有界触发，不默认enable、不循环重试，失败保持Unknown可查询。
13. 扩展原PR205构建器，按真实已实现矩阵输出同源应用、有限Root包、模式/角色/健康材料、来源/许可/版本清单；支持新增字段时显式格式化且保留旧包解析拒绝新kind。Root批准角色策略只能来自独立已持有的信任/批准，不从待安装包自授权，不把981/982当通用批准值。NKN/BLE/转发默认OFF，安装不启worker/网络，不越过原服务/账户边界。
14. 复用现有rootfs/内核/DTB/模块/initramfs离线工具和固定材料锁。能真实构建则产出并核验；缺基础材料明示缺项，应用/Root材料树不能称完整可启动固件。制品和元数据绑定**本轮最终完整源码/归档/程序字节**，不能复用205或208旧二进制冒充。

### D. 通过真实入口和故障矩阵交付，人工留最后

15. 编写可重跑的隔离端到端runner：自有临时信任/签名材料、真实CLI/launcher/Core/Auth/Store/安装器/迁移/确认账本串联。MM/NM、外部服务控制、PID1/cgroup在缺真实环境时可显式合成；授权、签名、实际文件/数据/守卫/确认不能整体Mock。生产ARM校验保持，不向生产暴露native测试开关。
16. 正例覆盖默认不变、显式产品安装、真实数据兼容/一次迁移、选择运行、同一候选健康确认一次、后续只读结果和默认OFF。调用图中每段必须给实际执行命令和成功证据；若模拟PID1，明确只证明合成环境中的接线。
17. 组合反例覆盖：另一个writer/维护调用竞争，安装可证明写集外Root变更，信任/签名/程序/slot/NSS换代，数据和回退不兼容，缺worker/ON代次不符，lease过期，SIGKILL及迟到回复，安装/迁移/确认各阶段中断，写满/只读，后写数据保留，重开Unknown不盲重试。使用真实签名和执行器，隔离容器故障不能写满宿主磁盘。
18. 对冻结最终组合跑fmt、默认/候选feature测试、strict Clippy、原网页/迁移/Root矩阵和来源工具严格模式；实际ARM交叉构建、元数据/真实CLI及可用QEMU验证。测试未运行、专项ignored、历史基线失败、工具/缓存/计费不足分别列出，不能计入通过。Rust1.98、锁文件约束和生产预算保持；缺ARM环境不伪造产物，先交已完成代码及精确本机补跑命令。

## 5. 三阶段提交，避免再交一个“只剩主线接线”的包

| 阶段 | 必须交付 | 允许仍未完成的外部项 |
|---|---|---|
| 8-B 正式入口与守卫 | 依赖整合、Core/updater真实入口、连续DataFence/有限Root变更、旧兼容不退化及实际CLI测试 | 最终发行、完整组合回归 |
| 8-C 生产组合 | 安装/迁移/选择/健康的受支持真实链、profile/发行构建器、同源集成runner | 硬件、真实PID1及当前环境没有的ARM/QEMU专项 |
| 8-D 冻结交付 | 最终源码/归档与产物绑定、全可执行回归、故障矩阵、实际未运行项和有限本机接续手册 | 真实密钥/epoch/设备安装/真人验收 |

可在同一集成分支按阶段提交并开一个PR；若开堆叠PR必须写清base、依赖和最终聚合HEAD，不复制公共历史或假设前笔已合main。不改208历史报告，将续接报告新增到 `docs/deliveries/web4-production-wiring-20261104.md`，README补真实生产用法。报告必须列原十四步映射和本表逐项代码路径/测试/缺项，不能以“已交18项”代替证据。

至少附以下材料：

- 全部输入完整SHA、起始main、最终产品代码SHA及归档SHA256；后续文档提交单列。
- 真实支持/拒绝的profile及升级/回退矩阵，原合同与必要新增合同差异。
- 原守卫从停机/备份、安装、数据handoff到最后提交的所有权/锁序图，真实竞争反例。
- 实际签名/程序/固定目标/数据/Root/健康调用链和可复现命令、退出码、独立测试统计；重叠组不累加。
- ARM、PID1、设备、真实ON worker等未运行项，当前制品是否可安装/可启动的准确边界，下一位本机接手的有限命令及停止条件。
- 许可证/来源和原作者保留；原始日志、临时密钥、数据库和设备信息不提交。

## 6. 允许事项、停止边界及完成口径

可以：在本任务范围开发生产代码，在自己分支普通合入上述已交PR，使用临时合成密钥与临时root、无网络容器、已有交叉工具链和QEMU，自查并正常提交PR。依赖已有交付，**不要停在等待3/6/7、不要等待它们先合main**；发生新缺接口时在本窗口负责的共享接线范围补齐。

不做：自行审核/合main、改保护/CI计费、用真实生产或设备开发密钥、分配epoch、生产签名、SSH/NAS/棒子/服务器操作、写真实分区/块设备/NSS/系统总线/宿主服务、拨号/发送/公网NKN或TURN。PMIC/关机继续搁置。真实部署门禁仍由本机在新只读证据、备份和授权后执行。

对矛盾的授权/回退/数据合同不可删断言硬接；保留失败证据和有限unsupported，继续其他可独立工作。云端CI当前各交付报告为计费未启动，本机尚未逐项重新审查；届时读取实际检查状态，不改成通过，也不自行绕过门禁。

本轮完成标准：原模块已经通过正式生产入口接通，有实际支持的完整隔离成功链及关键失败路径，发行材料与最终源码一致；只剩明确列出的设备/真实环境验收。若仍有共享入口、锁交接或已支持组合代码没写，就写**部分交付**，不能称任务8跑完。

本次只交续接指令，代码和设备验收没有因此新增完成：原P0–P7 **81.5%**、完整行动台账 **20/169**、加权 **10.6%** 保持。代码阶段与正式验收分别记录，预计工时沿原粗估，不给未经本轮实测的完成时间承诺。
