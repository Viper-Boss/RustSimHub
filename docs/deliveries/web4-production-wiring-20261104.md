# WEB4 任务8：生产接线与冻结验证

PR [215](https://github.com/Viper-Boss/RustSimHub/pull/215)，分支`codex/web4-production-wiring-20261104`，分阶段提交，未自行合并。**PR保留草稿：共享生产入口/连续守卫已经实现，Native完整成功链已通过；8-D的ARM健康确认仍在原5秒预算下Busy，未宣布全部完成。**没有设备、NAS、宿主NSS/服务操作、真实密钥、生产签名、epoch分配或部署；隔离夹具仅用临时签名与测试epoch1/2。全局保持 **81.5%，20/169，加权10.6%**。AI辅助开发与验证，依赖作者、许可证及PR208历史报告保留，不修改全局台账。

## 固定来源

起始main `a2044edfef6aeeced3d2c2eb8743791b34e4ccdc`，复用已合PR208（输入`66b2cfff2a57566efc36a0ea894af7f04d4a91a3`）。按真实祖先正常整合，不重复cherry-pick：

| 来源 | 完整HEAD |
|---|---|
| 健康206 | `fe65bf3e1fd02375ed6fcd38a595271c33180caa` |
| 健康211，含206 | `77b2baf88bc7409c72c31b5b08ea6b2aa8bf7ba9` |
| 安装209 | `42dca70ff82b405b787616def708903bb126bad5` |
| 安装213，含209 | `28c38c0bcb5c802c1f223787fa27efc957410ebd` |
| 数据210 | `19f9844fd76ff1597b69a694b2e86dff3561989b` |
| 数据212，含210 | `a148e670f8b6a38c0d7acfe4f9636b32a7eeff8b` |
| 旧195 | `6787addaf4ca0e79f8b8455cbb3175372c9c318c` |
| 旧196 | `395a07eea72a074f3126466d03a2b899787ac9eb` |
| 旧199 | `fb77eaf63a7ef50a8da229664fe37129a9a0bd6f` |

其余完整来源/包含关系/路径见`tools/product-integration/inputs.json`。后续正常合main `5304a96d28f2c76c14ab651938a217dbeae67fa3`，再以`143aae03e39eb6e4834d84bcb2b175f8481a8733`正常合入`55f9783910590b7b6ca4612af2ef9ccbd6c7c305`，无冲突，保留217/219/220控制台/Auth/Storage/Wire历史；2026-11-10再读远端main仍55f9783。冲突按原调用语义接入，未整文件覆盖依赖行为。

新版任务书从文档分支`42a10e372aeff58d983299160a137be9bc9b100b`读取，文件SHA256 `8fbd97a4497b13339306b276fd9c2fc90a7714b0e7c589acb1d1771dab213902`；相对用户08f716d链接仅新增顶部合并状态说明。未整条合入文档分支。

冻结代码 **`c771ad8825e64d52c7e7dc8418634f34fdd38a84`**，`git archive --format=tar` SHA256 **`80aabeca4884cc40a625af891017b78cdf35f89431673184ae519f7cdedbfc5a`**。后续报告提交不冒充该程序来源。制品与验证按真实源码分组，历史重叠测试不相加。

## 新版18项映射

| 项 | 实际代码/产物 | 验证或边界 |
|---|---|---|
| A1 | `tools/product-integration/inputs.json,verify.py`、正常合并历史 | 完整HEAD如上；严格来源检查通过，只证明组装。 |
| A2 | `crates/core/src/main.rs,runtime_compatibility.rs` | 正式Core固定MODE先于启动/激活，实际feature/schema/前缀，不开DB；未知模式、多参数拒绝。 |
| A3 | Core main、`tools/product-upgrade/recovery_child.rs` | 固定child/inspect，UID986、空附加组、空有效能力、NNP、密封字节与R/G/D；普通用户拒绝。原child未新增CapBnd要求，运行健康Core的检查包含Bnd。 |
| A4 | `crates/update/src/main.rs,lib.rs,admission/product.rs` | 生产prepare/apply/inspect/recover/rollback/activate无路径/UID/schema/健康输入；测试root只在测试API。 |
| B5 | `admission/retained.rs,recovery.rs,programs.rs` | 同一MaintenanceSnapshot、原业务/outbox writer跨安装保持；只在原迁移握手交出对应writer。 |
| B6 | `product_materials/rootio.rs,executor.rs`、`admission/retained.rs` | Opaque OwnedRootChanges由实际fd、固定目标、原新inode/摘要/权限/父目录支持；新增Root文件fd移入同一Snapshot，执行器释放后仍持有。 |
| B7 | `admission/product.rs`、原ProductInstaller/Store | 停有限服务、持锁一致备份、安装/账本串联；失败保留journal/Unknown，不重置floor/attempted、不恢复旧业务数据。 |
| B8 | `admission/programs.rs`、`product_admission/coordinator.rs` | 原邮件适配返回同一快照；base6及三方SMTP5/outbox4不变；format2 Root回执绑定原持锁outbox3摘要，仅作旧备份来源证据。 |
| B9 | `admission/recovery.rs`、原child/coordinator | 原6→7→9，G前重验源、D后验原日志/实际结果再取得writer；邮件先于恢复，默认不迁移。 |
| C10 | launcher `src/product.rs`、Core runtime、`admission/running/product.rs` | 有限支持矩阵见下；旧schema6健康合同保留，新增显式schema9/forwarding合同，AP/邮件自动确认拒绝。 |
| C11 | `firmware/system/product-profiles/`、`product_materials/targets.rs,package.rs` | 新恢复健康profile14固定Root目标；真实模式/独立角色策略/回退检查；单Core profile v1要求基线与候选Core字节一致。 |
| C12 | Core `update_health.rs,update_health/`、update `runtime_observation/`、launcher | 原健康前缀/socket/confirm-running-candidate、真实Auth/SQL lease、同flock转换、最后Recheck/state.next/rename/fsync；不调用裸confirm。 |
| C13 | `tools/product-release/release.py,managed_stage.py`、原`product-install/package.py` | 原构建器扩展同源应用、实际模式探测、Go源码/buildid/许可闭包；unsigned Root不含签名、不可安装。实际同源ARM13/16程序构建及unsigned装配通过。 |
| C14 | 原`tools/rootfs/`及固定系统材料锁 | 通用ARM来宾启动冒烟通过；缺锁定产品boot/modules与完整rootfs工具，整机固件not_run，不能将应用树称固件。 |
| D15 | `production_chain.py,fixtures/`、updater libtest | 临时签名、真实Core/launcher/Auth/Store/安装/迁移/确认；MM/PID1等外部环境合成。Native架构/cgroup适配只在cfg(test)，正式ARM无该开关。 |
| D16 | runner `--scenario default/product/email` | c771ad8同源Native默认/恢复健康/邮件链通过；ARM正式CLI结果见下，不移用旧字节。 |
| D17 | 原Root矩阵、恢复/邮件矩阵、健康故障fixture、完整链preflight | 已执行/失败/未运行分列；ordinary ignored不计通过，不删除Unknown重试同一事务。 |
| D18 | fmt、workspace/Clippy、Node/Chromium、来源工具、发行/QEMU | 按源码/feature/架构记数，最终冻结专项结果见下。 |

## 原十四步对应

| 原动作 | 本轮对应实现/证据 |
|---|---|
| 1 固定清单 | A1：inputs完整SHA/包含关系、严格检查。 |
| 2 正常整合 | A1与来源历史：208复用、六依赖/两个后续main正常合入。 |
| 3 缺项不伪实现 | A1/C14/D18：来源授权字段恒false，缺项单列。 |
| 4 正式入口 | A2–A4：真实Core/updater有限CLI。 |
| 5 模式/数据链 | B5–B9/C10–C11：原守卫和真实模式元数据。 |
| 6 可信健康 | C12：原Auth/SQL/lease/flock/Recheck与一次确认。 |
| 7 服务角色/OFF | B6/C11/C13/D16：独立角色/固定材料/实际OFF进程检查；真实PID1另列。 |
| 8 安装与迁移 | B5–B9：同一快照、原journal/Unknown、writer-only交接。 |
| 9 新组合发行 | C13/D18：新源码构建，205/208旧二进制不作新证据。 |
| 10 系统材料 | C14：原工具/固定锁，区分测试来宾和产品固件。 |
| 11 端到端 | D15–D16：签名/锁/授权/数据/确认不整体Mock。 |
| 12 组合故障 | D17：真实失败、Unknown、晚回复与未运行分列。 |
| 13 冻结复验 | D18：按源码分别记录，不累加重叠集合。 |
| 14 阶段PR/接续 | PR215分阶段提交；固定入口见README，签名/设备另行授权。 |

## 支持矩阵及守卫生命周期

| profile | 实际模式/数据 | 健康与回退边界 |
|---|---|---|
| legacy-v1 默认 | schema6、worker4/outbox3、原启动，NKN/BLE/转发OFF | 不隐式装产品/迁移；默认链无产品profile/健康socket，floor1、无候选、attempted=false。 |
| product-v1 | schema6、显式产品前缀、worker4/outbox3 | 真实维护兼容准入，本轮不提供自动确认。 |
| product-email-v1 | schema6、SMTP5/outbox4；3→4仅base6及三方SMTP5 | updater缺email feature在安装前拒绝；不自动健康确认。 |
| product-recovery-v1 | schema9、真实recovery feature、维护从6/7 | 原迁移，不自动确认。 |
| product-email-recovery-v1 | 邮件3→4先于6→7→9，SMTP5+recovery feature | 仅满足原门禁可迁移；组合完整链尚无通过声明，不自动确认。 |
| product-recovery-health-v1 | **仅forwarding Root**、schema9、worker4/outbox3或队列不存在，14 Root目标 | 基线/候选/回退实际实现模式与健康前缀，单Core profile要求相同Core字节；一次确认。AP/SMTP5健康、旧基线缺前缀、worker/ON代次错误拒绝。 |

角色1201/1202仅为夹具，生产值须独立Root策略/NSS批准，包不能自授权。旧基线缺prefix先独立显式维护升级，不能改JSON兼容。

```text
原维护锁 + 同一Snapshot[业务/outbox writer,Root/source/trust/NSS/slot/fd]
 -> 同一DataFence[仅实际执行器证明的固定Root变更] -> 原安装journal
 -> 原Snapshot -> 邮件仅outbox writer交原worker，结果4后取回
 -> 原Snapshot -> 恢复仅业务writer交原child R/G/D，结果9后取回
 -> 原Store stage/select先消耗attempt -> 数据库writer交真实Core
 -> 新鲜Root观察 + 原Auth/SQL lease/flock/最后Recheck -> 原账本确认一次
```

Root fd在执行器/DataFence释放后仍由原Snapshot持有；新源/信任/NSS/slot/业务inode/写集外Root变化继续使原能力失效。新schema9备份可匹配实际当前摘要重用；旧before6必须有原child完成证据；旧outbox3必须有原format2邮件完成证据。绝不覆盖旧备份或自动恢复旧数据。

## 最终源码与实际制品

生产代码冻结为上述c771ad8，原始归档80aabeca不随报告提交重打包。验证夹具提交`dfdd3f55e32cf98690c00322cd1af94e4df8fb86`仅在外部ARM测试环境改目录、PID1查询/cgroup标记及只读诊断；不改生产程序。`10beaf8373a4147126b343002f949d8dce73d7cc`仅修独立迁移harness的导出消费、锁文件现有storage依赖及说明；有限CPU模型选择提交`a101c01fd4b2ebfc05c37277ef8a963b33cc28c8`仅为测试环境补充，完整验证提交见[静态证据](web4-production-wiring-evidence-20261110.json)，不改生产程序。所有下列正式程序均从c771ad8重新构建，不复用205/208或旧50deff程序。构建后按真实ELF、有限模式、健康前缀、来源和launcher输出核验。

| 正式程序组 | 实际数量/六应用载荷总字节 | 编译与准确边界 |
|---|---|---|
| ARM legacy-v1 | 13程序 / **17,107,153** | aarch64、release opt=z/thin LTO/strip/panic=abort，Core三个产品feature均false。 |
| ARM product-recovery-health-v1 | 16程序 / **17,697,017** | 同上，真实产品/恢复feature、schema9/forwarding、worker4；非dev-fixtures。 |
| Native默认/恢复健康 | runner使用11程序 / **23,352,529** | release opt=z/thin/strip/panic=unwind测试制品；Core有产品能力，默认场景未启用。真正feature=false默认另由ARM证明。 |
| Native邮件 | runner使用11程序 / **23,754,609** | 同上，实际SMTP5 Core/worker和email updater，非dev-fixtures。 |

六载荷上限仍25,165,824字节（24MiB）。完整程序SHA256/字节、材料/许可摘要、真实模式metadata、构建清单摘要见静态证据。ARM两份profile-build摘要分别`79ab087fce898729413d15ac4319632a7fb163fc6b52626292edc667e81989fc`、`cf90afe11b081c1c88c10a3fec1c01e0bd6f5263201ef6fb4acb6abd080b5d78`。

产品实际Go Root与SDK仍是原Go实现，来源c771ad8/buildid rsh80aa，原作者及许可证保留。原managed_stage核验16个实际链接模块及许可闭包、未引入新外部模块；runtime notice SHA256 `911f8f5782931320f5b8d1160a76365b83aea6447ee6c04fa6d5591467db9dad`。默认/产品unsigned Root各6项装配正反例通过，分别3/14目标；用临时应用签名、fixture epoch2及独立角色批准，只证明装配。Root没有manifest.sig且有UNSIGNED，不可安装，未做生产签名或epoch分配。

## 冻结源码实测（各组不累计）

| c771ad8范围 | 实际结果与边界 |
|---|---|
| workspace默认 | **1458通过、0失败、24 ignored**。 |
| workspace all-features | **1661通过、0失败、30 ignored**。测试opt=z/panic=unwind/LTO=false，正式ARM仍thin/abort；ignored未记通过。 |
| fmt / strict Clippy | `cargo fmt --all -- --check`及`cargo clippy --workspace --all-targets --all-features --release --locked --offline -- -D warnings`通过。Rust1.98的proc-macro-error2 future-incompat提示保留。 |
| 原Root矩阵 | runner报告24通过/0失败/3 ordinary ignored；专项另执行真实SIGKILL、独立1MiB tmpfs ENOSPC及只读。 |
| 原恢复/邮件矩阵 | 恢复16 + 实际updater原guard1、邮件9通过，使用原child/Auth/Store；矩阵自身的metadata传输合成，与正式CLI完整链分开。 |
| 独立harness | 5项及严格Clippy通过；消费实际updater导出，不放宽私有健康API。 |
| 原健康故障fixture | 原Core库/Auth/Store/Unix连接及账本检查通过，含15项变更及scratch-fsync后实际退出91；metadata/PID1/ready适配明确合成。 |
| 固定生产入口 | 有限CLI正反7项通过；未知模式、多参数、普通用户在副作用前拒绝。 |
| 网页 | Node32组；实际Chromium320/390/1280主页、缺模块负对照及AP11场景通过。外部设备API合成，WASM专项未运行。 |
| ARM只读PID1夹具对照 | 45组、19项属性与原控制器一致；私有路径适配，实际/proc存活/僵尸/退出反例，未操作宿主服务。 |
| 来源/工具 | 干净c771ad8严格来源检查通过（authorization/health/installable等仍false）；发行18、输入12通过，夹具后再次18/12通过。 |

Native三条完整链均成功，使用无网络的一次性Root容器和实际同源Core/launcher/媒体/Edge。默认经原boot-select、真实Owner登录/MM协调保持schema6/floor1/无候选/attempted=false。恢复健康链含6项真实安装前拒绝（签名、程序、信任、NSS附加别名、维护锁、业务writer），DB及原账本不变；14固定Root目标、原6→7→9、inspect/activate、重复activate在停服务前拒绝、实际Core/Auth/MM、确认一次及二次拒绝、floor2/无候选/attempted=false、OFF进程扫描、停机后inspect schema9及当前9新备份重用通过。邮件链实际7目标、原Auth绑定及3→4、format2完成回执和原业务writer连续保持通过，不宣称邮件自动确认。

| 正式ARM来宾链 | 实际结果 |
|---|---|
| legacy默认，最终Cortex-A53 | **通过**。正式默认13程序、boot-select/launcher/Core、原Auth/Store/Owner登录及MM协调，schema6/floor1/无候选/attempted=false；没有隐式产品/健康入口。 |
| 恢复健康，max CPU | 6项安装前拒绝；prepare/apply/inspect/activate全部0；确认1/Busy，约**5.3039秒**。独立只读inspect-runtime为1/Busy、5.7761秒；Core/worker metadata仅24.44/31.49ms。 |
| 恢复健康，Cortex-A53 | 6项安装前拒绝；prepare/apply/inspect/activate全部0；确认1/Busy，约**5.4553秒**；只读inspect-runtime 1/Busy、5.5128秒。真实三个服务cgroup正确，metadata24.93/28.83ms。 |

两轮ARM失败后原账本均保留**floor2、candidate epoch2、attempted=true、confirmed epoch1**；未误确认、未清预算，也未自动重试。ARM的确认后第二次拒绝、floor2/无候选/attempted=false、确认后OFF扫描、停机inspect及当前9备份复用因此**not_run**，这些正例仅由明确标出的Native完整链证明。当前2 vCPU配额/TCG下未满足5秒运行观察预算；尚未定位到每段耗时，也未证明真实目标满足预算，不能把它描述成无风险环境噪声。生产拒绝保持，不更改签名/锁/确认语义迁就测试。

ARM使用实际aarch64发行程序和真实通用来宾内核/proc/cgroup2；无Native架构适配。服务控制/MM为明确的外部夹具，非真实systemd PID1。夹具先建立0755安全Root目录；BusyBox只读show读固定控制器PID及实际/proc，所有启动/停止仍由原控制器操作真实程序和真实cgroup；只读ARM标记跨生产env_clear保留此行为。未以夹具提供健康授权，签名/Auth/数据/确认仍是实际实现；5秒、250毫秒、300秒及24MiB门禁不变。

## 故障验证的实际层次

| 故障 | 实际检查点/边界 |
|---|---|
| 另一维护/业务writer竞争 | 最终Native产品链真实flock竞争，安装前拒绝且DB/账本字节保持；非仅JSON断言。 |
| 写集外Root变化 | `admission/retained/tests.rs::unrelated_root_change_invalidates_permanently_without_releasing_original_writer`，实际签名安装中改动，原writer仍被持有，能力永久失效。 |
| 签名/程序/信任/NSS变化 | 最终Native链实际篡改/独立NSS附加别名反例；旧slot/链接/inode/state.next由`runtime_observation/tests.rs`实际签名文件与守卫回归覆盖。 |
| 不兼容数据/回退 | 原`product_admission`与launcher比较、原实际DB/签名守卫测试；不将合成元数据传输误记为正式ARM入口。 |
| worker OFF/ON/Unknown/旧代次/重复PID | `runtime_observation/tests.rs::worker_off_on_unknown_residual_old_generation_and_duplicate_mainpid_matrix`；外部unit/cgroup观察为合成，实际设备ON未运行。 |
| Auth撤权、SQL/actor/观察lease过期 | Core `update_health/tests.rs`真实Auth/Store/Unix peer及Root健康故障fixture；实际死亡/exec/socket替换拒绝，原账本不变。 |
| 迟到/断连/错误来源/绑定 | 原恢复16及邮件9矩阵实际通过；Root完成回执不授予旧存活能力。 |
| 安装中断/SIGKILL | 原安装Root矩阵真实SIGKILL及保留journal恢复；不清Unknown或恢复旧业务数据。 |
| 确认中断 | 原健康fixture在scratch fsync后实际退出91，检查state.next未发布、原账本保持；不把它写成SIGKILL。 |
| ENOSPC/只读 | 原Root矩阵独立小tmpfs/只读tmpfs；不填充宿主磁盘。 |
| 后写业务及备份 | 最终Native链真实Core/Auth/MM写入后保留schema9，停止后inspect及两次当前9备份重用；旧before6/旧outbox3只有实际完成来源证据可接受，不能覆盖或自动还原。 |
| 重开Unknown/代次变化 | 原journal/保留材料重验与DataFence永久失效回归；全新维护观察仍须实际信任/角色/槽位/数据匹配，不以旧JSON恢复权限。 |

三条完整链只证明明确记录的Native合成外部环境接线；未把上述所有分层故障都宣称为逐一重跑整个正式ARM链。原矩阵、普通测试、专项ignored和完整链分别登记，不合计成一个通过数。

## 失败记录（保留，不计通过）

- 首轮库94通过/1失败/6 ignored：CPU竞争下原250ms子进程预期Integrity、实际Busy；未放宽预算。旧隔离链源chmod代次、DBus别名/目录模式/媒体启动停机夹具和工作区ENOSPC失败保留。
- 原panic=abort全特性测试曾触发Cargo6313 rlib/cdylib输出冲突及E0463；单元测试用unwind解决，正式ARM配置不变。Clippy测试模块位置拒绝后仅移动测试模块，生产声明逐字一致，无lint allow。环境切换杀死一轮构建（SIGKILL9），未当作仍在运行或通过，最终已重跑。
- 邮件format3夹具原私有目录缺失，改0700后仍由原Queue创建。原邮件apply完成7目标后Unknown源于固定模板两个目录与有限变更证明不匹配；ebc9af6只修该固定模板证明范围，7b599ae补原writer及Product/Email回归，旧Unknown保留，新事务复验。原硬编码ARM兼容比较导致Native邮件拒绝，仅cfg(test)复用原Native适配。混用旧归档/新driver的prepare按Integrity拒绝，不作为成功。
- 交叉ranlib缺libbfd先失败，修私有工具链库路径后新构建成功。Native暂存前两次空间低于1GiB拒绝，未开始链。矩阵只读文件覆写、输入目录0700导致低权读取失败、旧driver缺原guard测试等均保留；最终指定实际driver并精确列出guard，拒绝零测试假通过。只清理本人已导出并记摘要的缓存，无全局清理。
- ARM复用只读fixture marker在启动前拒绝；新来宾rootfs的01777根目录被正式Root守卫拒绝，夹具明确设0755。首次product-apply在TCG下300秒超时，保留失败和日志，未提高预算；重复启动Python查询成本改为夹具BusyBox固定只读show。后续apply/inspect/activate成功但健康Busy；env_clear丢失夹具标记致cgroup遗漏已修，另一轮实际正确cgroup仍Busy；最终max与Cortex-A53固定只读诊断显示5秒运行观察仍不满足，具体耗时定位尚未完成。每次复验均新来宾/新事务，未删除原Unknown重试。
- ARM SHA隔离诊断（非正式制品）：同一6,183,248字节Core重复24次，原锁定sha2 0.10.9约4,960.13ms、锁中已有0.11.1约3,982.85ms，摘要一致、来宾SHA硬件特性可见；尚不证明完整确认满足5秒。未修改冻结生产依赖或把benchmark计入完整链。
- 旧主页测试隐藏面板失败仅修夹具导航，最终三宽及负对照通过。云端c771ad8 [run42775066418](https://github.com/Viper-Boss/RustSimHub/actions/runs/42775066418) test/native-audio均在启动前因GitHub付款/消费额度门禁失败，steps为空、日志BlobNotFound；已读实际annotations，未改计费，未记通过。最终PR HEAD云端状态另在PR回读说明。

## 未运行项与有限复验手册

完整产品固件**not_run**：缺锁定`6.12.49-msm8916-g93a71ee9468d` boot（SHA256 `d5790e07a8ee67c6d80ec24ea9b0cd4b5a78ab4440a3de2b8767fea58a03eaee`）与modules（`7e1d5d53a92ae6eb7f604cb98681b5132bc41071118a6230978354ad5b74c4e8`），缺mmdebstrap/gpgv/dtc；readelf、ARM交叉工具链、QEMU可用。公开上游boot摘要不同，不可替代固定锁。通用Debian6.12.107来宾不是UFI003产品固件。现有应用/Root材料明确不可安装、不能称可启动整机镜像。

真实设备/systemd PID1沙箱、真实NKN/BLE/转发ON、真实网络/SMTP、产品内核/DTB/initramfs、生产信任/签名/epoch、部署及人工验收均**not_run**。WASM浏览器专项缺`/wasm`，未计通过。EmailRecovery完整组合和不支持profile的自动确认未验证；不将分层故障测试称为逐一完整ARM故障链。没有设备/NAS/宿主NSS或宿主服务操作，没有使用真实密钥、外发或改全局进度。

[README](../../tools/product-integration/README.md)列固定生产入口及隔离runner；源码/driver/程序必须取匹配组。ARM时限问题仍未解决，不将后续性能定位或真实目标验证写成已通过。有限离线复验示例：

```sh
CARGO_PROFILE_RELEASE_PANIC=unwind CARGO_PROFILE_RELEASE_LTO=false cargo test --workspace --release --locked --offline
CARGO_PROFILE_RELEASE_PANIC=unwind CARGO_PROFILE_RELEASE_LTO=false cargo test --workspace --all-features --release --locked --offline
python3 tools/product-integration/verify.py --repository CLEAN_FROZEN_CHECKOUT --integration c771ad8825e64d52c7e7dc8418634f34fdd38a84 --require-complete
python3 -m unittest discover -s tools/product-release -p 'test_*.py'
python3 -m unittest discover -s tools/product-integration -p 'test_*.py'
python3 tools/product-integration/production_chain.py --mode arm-vm --scenario product --qemu-cpu cortex-a53 \
 --programs PRIVATE_C771_ARM_PRODUCT_PROGRAMS --test-driver PRIVATE_C771_ARM_LIBTEST \
 --revision c771ad8825e64d52c7e7dc8418634f34fdd38a84 \
 --guest-root "$PWD/.local/FRESH_OWN_ARM_DEPENDENCY_ROOT" --kernel PRIVATE_GENERIC_KERNEL --qemu PRIVATE_QEMU
```

同一命令用`--scenario default`及对应legacy程序复验真正默认；Native/default/product/email用README的明确driver/原format3辅助输入。发行仍是原`tools/product-release/release.py`，unsigned Root仍委托原`tools/product-install/package.py`。完整固件只能在取得并验证锁定boot/modules后用原`tools/rootfs/scripts/build-rootfs.sh --lock-file firmware/base-system.lock.json`及原帮助要求的固定输入另跑，不能替换摘要。

生产有限顺序为`product-prepare → product-apply → product-inspect → product-activate → confirm-running-candidate → inspect-state`，不在宿主作fixture试验。来源/独立角色策略/信任/基线/数据/空间/能力不符即停止；Executing/Unknown先查询原journal，禁止盲重试、清floor/attempted、覆写备份或自动还原旧业务。`product-recover/rollback`只处理原安装器可证明的有限材料。共享生产入口已在本分支实现；ARM的5秒时限失败仍为本包未关闭项，不把它改称“本机待接线”。真实签名/设备验收由负责人另行授权。

公开[静态证据](web4-production-wiring-evidence-20261110.json)只含来源、制品摘要和脱敏结果；原始日志、fixture密钥/DB/身份材料留忽略的`.local/`。按任务书不改全局台账；**81.5%，20/169，加权10.6%**保持。
