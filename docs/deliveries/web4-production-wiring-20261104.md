# WEB4 任务8：生产接线与冻结验证

PR [215](https://github.com/Viper-Boss/RustSimHub/pull/215)，分支`codex/web4-production-wiring-20261104`，分阶段提交，未自行合并。**本包离线接线与可执行冻结验证已完成，送负责人审核；不自行合并。ARM健康确认的原5秒失败已在最终同源正式程序上关闭，设备及真实环境验收仍未完成。**没有设备、NAS、宿主NSS/服务操作、真实密钥、生产签名、epoch分配或部署；隔离夹具仅用临时签名与测试epoch1/2。全局保持 **81.5%，20/169，加权10.6%**。AI辅助开发与验证，依赖作者、许可证及PR208历史报告保留，不修改全局台账。

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

其余完整来源/包含关系/路径见`tools/product-integration/inputs.json`。后续正常合main `5304a96d28f2c76c14ab651938a217dbeae67fa3`，再以`143aae03e39eb6e4834d84bcb2b175f8481a8733`正常合入`55f9783910590b7b6ca4612af2ef9ccbd6c7c305`，无冲突，保留217/219/220控制台/Auth/Storage/Wire历史；本轮回读远端main仍55f9783。冲突按原调用语义接入，未整文件覆盖依赖行为。

新版任务书从文档分支`42a10e372aeff58d983299160a137be9bc9b100b`读取，文件SHA256 `8fbd97a4497b13339306b276fd9c2fc90a7714b0e7c589acb1d1771dab213902`；相对用户08f716d链接仅新增顶部合并状态说明。未整条合入文档分支。

最终产品代码 **`8470608781b4ab772597cef0d767e012b594f257`**，`git archive --format=tar` SHA256 **`efebf0bf4619f48041c1e66a310b583d987cd7d97348d3bf9c81e79a5d848dc1`**。全部本轮正式程序及新构建测试driver绑定此源码；后续文档/证据提交的完整HEAD另列于PR215，不冒充程序来源。前冻结`c771ad8825e64d52c7e7dc8418634f34fdd38a84`/归档`80aabeca4884cc40a625af891017b78cdf35f89431673184ae519f7cdedbfc5a`的结果仅为[历史证据](web4-production-wiring-evidence-20261110.json)，不移算为新源码通过。

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
| D16 | runner `--scenario default/product/email` | 8470608同源Native默认/恢复健康/邮件链及ARM默认/恢复健康正式CLI完整链通过；强制128MiB/8任务产品链复验通过。 |
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

## 关闭原ARM时限失败

本轮定位到重复读取/校验大型程序与重复元数据探测的开销。`f7eaf12`、`3b0a303`、`ae14454`保留原签名/ELF/全载荷验证产生的不可变应用字节和原始程序fd；逐次检查原fd、路径、完整inode/权限/owner/链接数/大小/mtime/ctime，进程PID/start/UID/caps/NNP/cgroup仍由实际内核核验。Root只读运行路径保留初次完整签名/载荷验证，小合同/信任/NSS继续原检查；安装器原完整字节重验及有限OwnedRootChanges接口保持。

`a81b0a8`、`8470608`将完整重验保留在对外返回、获取Core lease前、flock前/转换后和最后Recheck，去除同一内部调用链中相邻重复检查；私有借用签名字节不是存活能力。原SQL/Auth lease、同一flock、scratch/state.next/rename/fsync及消费型确认均不变，5秒截止时间不刷新。新增真实换inode、改字节后恢复mtime、权限/链接变化、错误程序、签名/合同变化、截止/永久失效测试通过。原独立`health::probe_process`仍完整验证。

`0948baf`仅将锁中既有`sha2:0.10.9`的release优化级别设3，应用整体仍opt=z/thin/strip/abort；未升级依赖、修改Cargo.lock、强制目标CPU或更换算法。标准SHA256空/abc/百万a与SHA512 abc向量通过。私有ARM同字节24轮诊断为6696.53→1083.98ms、摘要相同，只用于定位；完整确认以如下正式链为准。5秒/250ms/300秒/24MiB/128MiB/8任务预算均未提高。

## 最终源码与实际制品

下列四组均由冻结源码`8470608781b4ab772597cef0d767e012b594f257`重新构建。ARM为原发行构建器实际输出，Native为组合验证制品；不复用旧205/208/c771/0948二进制冒充。全部实际程序字节/SHA256、源归档、模式、launcher、材料、许可、driver与内核/initramfs摘要见[本轮静态证据](web4-production-wiring-evidence-20261111.json)。

| 程序组 | 导出数量/六应用载荷总字节 | 编译与边界 |
|---|---|---|
| ARM legacy-v1 | 13 / **17,238,225** | aarch64，opt=z/thin/strip/panic=abort，三个产品feature均false。 |
| ARM product-recovery-health-v1 | 16 / **17,762,553** | 同上，真实产品/恢复feature、schema9/forwarding/worker4，非dev-fixtures。 |
| Native默认/恢复健康组 | 11 / **23,374,705** | x86_64，opt=z/thin/strip/panic=unwind；默认链用8程序，产品链用11；Core有产品能力，默认场景不启用。真正feature=false默认由ARM证明。 |
| Native邮件组 | 11 / **23,780,497** | 同上，实际SMTP5 Core/worker/email updater；不提供邮件自动确认。 |

六载荷上限仍25,165,824字节。测试driver为同源独立制品：ARM与workspace测试用unwind/LTO=false；Native完整链driver用unwind/thin，Native架构/PID1/cgroup/dispatch适配只在cfg(test)，正式ARM CLI无此开关。Native旧format3数据生成器单列：夹具`549849fa3349afdef114e3e8ff91cf3203c9981b`，程序归档`50deff48336fa36f73996f714518888bb3220d43`；仅创建原Queue数据，不作为最终程序来源。

Go Root/SDK仍为原Go实现，实际源码commit为8470608、buildid绑定归档efebf0bf；原作者及许可保留。原managed_stage核验ARM实际链接16模块、Native17模块及各自许可闭包，无新外部模块，`dependency_approval`仍false。默认/产品unsigned Root各6项装配正反例通过，分别3/14目标；临时应用签名、fixture epoch2和独立角色批准不授予生产权限。Root无manifest.sig且有UNSIGNED，不可安装，未生产签名或分配生产epoch。

## 冻结源码实测（各组不累计）

| 8470608范围 | 实际结果 |
|---|---|
| workspace默认 | **1466通过、0失败、25 ignored**。 |
| workspace all-features | **1669通过、0失败、31 ignored**。ignored未计通过。 |
| fmt / strict Clippy | fmt及all-features/all-targets strict Clippy退出0；保留既有proc-macro-error2 future-incompat notice。 |
| 原Root矩阵 | runner报告**25通过、0失败、3 ordinary ignored**；实际SIGKILL、独立1MiB tmpfs ENOSPC/只读专项通过。共享owned检查在两个driver有重叠，不与workspace累加。 |
| 原恢复/邮件矩阵 | 恢复16＋实际原guard1、邮件9通过；元数据传输合成，与正式CLI成功链分开。 |
| 独立harness | 5通过/0失败及严格Clippy通过，实际执行，不把no-run计通过。 |
| 原健康故障fixture | 原15种变更与scratch fsync后实际退出91路径通过；真实Core/Auth/Store/Unix，unit/cgroup/元数据捕获等边界由原fixture明确合成。 |
| 固定CLI / ARM截止测试 | 实际新程序7项正反例；ARM新增原截止/永久失效测试1通过。 |
| 网页 / 工具 | Node32脚本全退出0；Chromium三宽/缺模块负对照/AP11；发行18、输入12及干净完整SHA严格来源检查通过。来源授权/安装/健康字段仍false。 |
| 只读PID1夹具对照 | 新源码45组/19属性与原控制器一致，实际/proc存活/僵尸/退出；不操作宿主服务。 |

| 完整隔离链 | 实际结果与证据ID |
|---|---|
| 正式ARM默认 | **通过**，`1bd8cc7ef5144dd9baf97aaf7d48e040`：原boot-select/launcher/Core/Auth/Store/Owner/MM，schema6/floor1/无候选/attempted=false，产品feature均false。 |
| 正式ARM恢复健康 | **通过**，`4df3a5c47c324f6cbca5e082cd6a5db9`：确认4.4943秒，wait4峰值RSS105584KiB。 |
| 正式ARM恢复健康，实际资源限制 | **通过**，`efc7625272e9446bbda8f68df3fe8959`：实际guest cgroup memory.max=134217728、swap.max=0、pids.max=8；确认4.7317秒，memory.peak=109289472字节（104.23MiB），pids.peak=3，所有memory.events为0。 |
| Native默认 | **通过**，`d92f051e32cd4169af61d4c093069097`。 |
| Native恢复健康 | **通过**，`f0602964c3b54c8ba4e61093073ac8b5`。 |
| Native SMTP5邮件 | **通过**，`33621820e7744c75aae5198fe2702abf`；真实3→4、format2完成回执及同一writer/DataFence，不宣称邮件自动确认。 |

两轮ARM产品链均包含6项真实安装前拒绝、prepare/apply/inspect/activate退出0、同一候选确认一次及二次拒绝、floor2/无候选/attempted=false、实际OFF进程扫描、停机inspect schema9及当前9备份复用，未重试失败事务。正式ARM的签名、元数据探测、文件/数据/守卫、Core/Auth/SQL及确认账本均为实际实现；MM/PID1服务控制为明确的外部夹具，进程/UID/caps/proc/cgroup由实际内核核验。强制资源限制的私有固定命令exec/wait4观察器源/程序摘要另列，不改变确认参数、返回码、截止时间或授权。

ARM为Cortex-A53/2vCPU TCG、通用Debian6.12.107内核，非产品固件或真实systemd沙箱验收。Native三链复用原生产库入口并以cfg(test)适配架构/dispatch/PID1/cgroup；实际Core、签名、安装器、迁移、Auth/Store及账本不整体Mock。每组的准确范围由各自result与下表限制，不把所有分层故障冒充完整ARM故障链。

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

Native三条链与两轮正式ARM产品链分别记录；未把上述所有分层故障都宣称为逐一重跑整个正式ARM链。原矩阵、普通测试、专项ignored和完整链分别登记，不合计成一个通过数。

## 失败记录（保留，不计通过）

- 首轮库94通过/1失败/6 ignored：CPU竞争下原250ms子进程预期Integrity、实际Busy；未放宽预算。旧隔离链源chmod代次、DBus别名/目录模式/媒体启动停机夹具和工作区ENOSPC失败保留。
- 原panic=abort全特性测试曾触发Cargo6313 rlib/cdylib输出冲突及E0463；单元测试用unwind解决，正式ARM配置不变。Clippy测试模块位置拒绝后仅移动测试模块，生产声明逐字一致，无lint allow。环境切换杀死一轮构建（SIGKILL9），未当作仍在运行或通过，最终已重跑。
- 邮件format3夹具原私有目录缺失，改0700后仍由原Queue创建。原邮件apply完成7目标后Unknown源于固定模板两个目录与有限变更证明不匹配；ebc9af6只修该固定模板证明范围，7b599ae补原writer及Product/Email回归，旧Unknown保留，新事务复验。原硬编码ARM兼容比较导致Native邮件拒绝，仅cfg(test)复用原Native适配。混用旧归档/新driver的prepare按Integrity拒绝，不作为成功。
- 交叉ranlib缺libbfd先失败，修私有工具链库路径后新构建成功。Native暂存前两次空间低于1GiB拒绝，未开始链。矩阵只读文件覆写、输入目录0700导致低权读取失败、旧driver缺原guard测试等均保留；最终指定实际driver并精确列出guard，拒绝零测试假通过。只清理本人已导出并记摘要的缓存，无全局清理。
- ARM复用只读fixture marker在启动前拒绝；新来宾rootfs的01777根目录被正式Root守卫拒绝，夹具明确设0755。首次product-apply在TCG下300秒超时，保留失败和日志，未提高预算；重复启动Python查询成本改为夹具BusyBox固定只读show。后续apply/inspect/activate成功但健康Busy；env_clear丢失夹具标记致cgroup遗漏已修，另一轮实际正确cgroup仍Busy；旧max与Cortex-A53固定只读诊断显示5秒运行观察不满足；本轮完成耗时定位并以8470608正式链关闭，旧失败不改为通过。每次复验均新来宾/新事务，未删除原Unknown重试。
- ARM SHA隔离诊断（非正式制品）：同一6,183,248字节Core重复24次，原锁定sha2 0.10.9约4,960.13ms、锁中已有0.11.1约3,982.85ms，摘要一致、来宾SHA硬件特性可见；尚不证明完整确认满足5秒。未修改冻结生产依赖或把benchmark计入完整链。
- 旧主页测试隐藏面板失败仅修夹具导航，最终三宽及负对照通过。云端c771ad8 [run42775066418](https://github.com/Viper-Boss/RustSimHub/actions/runs/42775066418) test/native-audio均在启动前因GitHub付款/消费额度门禁失败，steps为空、日志BlobNotFound；已读实际annotations，未改计费，未记通过。最终PR HEAD云端状态另在PR回读说明。

- 中间正式源码0948baf产品链确认仍Busy（5.1491秒）；a81b0a8混合诊断仍Busy（5.0977秒），插桩混合诊断4.953秒成功仅作定位，均不作为最终同源正式证据。0948发行曾ENOSPC，INCOMPLETE/日志保留，清理已导出本人缓存后由原构建器复验；847两组ARM构建完成。
- 本轮Native构建的cargo/rustc收到SIGKILL9、执行会话消失，原因未确认；中断日志与退出状态保留。核验已导出默认程序/driver的同源摘要后，仅从同源码/同profile编译缓存续建邮件及SDK，实际全部完成。私有导出driver缺执行位、来源检查错参数、校验updater metadata误期待不存在字段等setup失败已纠正并复跑，不计为测试通过。
- 本轮8470608 [CI run42879932013](https://github.com/Viper-Boss/RustSimHub/actions/runs/42879932013) test/native-audio均因账户付款/消费额度门禁未启动，步骤数0；已读实际annotations，未改计费或记通过。最终文档HEAD检查状态另在PR回读说明。

## 未运行项与有限复验手册

完整产品固件**not_run**：缺锁定`6.12.49-msm8916-g93a71ee9468d` boot（SHA256 `d5790e07a8ee67c6d80ec24ea9b0cd4b5a78ab4440a3de2b8767fea58a03eaee`）与modules（`7e1d5d53a92ae6eb7f604cb98681b5132bc41071118a6230978354ad5b74c4e8`），缺mmdebstrap/gpgv/dtc；readelf、ARM交叉工具链、QEMU可用。公开上游boot摘要不同，不可替代固定锁。通用Debian6.12.107来宾不是UFI003产品固件。现有应用/Root材料明确不可安装、不能称可启动整机镜像。

真实设备/systemd PID1沙箱、真实NKN/BLE/转发ON、真实网络/SMTP、产品内核/DTB/initramfs、生产信任/签名/epoch、部署及人工验收均**not_run**。WASM浏览器专项缺`/wasm`，未计通过。EmailRecovery完整组合和不支持profile的自动确认未验证；不将分层故障测试称为逐一完整ARM故障链。没有设备/NAS/宿主NSS或宿主服务操作，没有使用真实密钥、外发或改全局进度。

[README](../../tools/product-integration/README.md)列固定生产入口及隔离runner；源码/driver/程序必须取匹配组。本包ARM离线时限失败已关闭；真实目标仍须另行验证，不将通用来宾结果等同实机。在干净的冻结SHA checkout中，有限离线复验示例：

```sh
export RUSTSIMHUB_BUILD_SOURCE_SHA256=efebf0bf4619f48041c1e66a310b583d987cd7d97348d3bf9c81e79a5d848dc1
CARGO_PROFILE_RELEASE_PANIC=unwind CARGO_PROFILE_RELEASE_LTO=false cargo test --workspace --release --locked --offline
CARGO_PROFILE_RELEASE_PANIC=unwind CARGO_PROFILE_RELEASE_LTO=false cargo test --workspace --all-features --release --locked --offline
python3 tools/product-integration/verify.py --repository CLEAN_FROZEN_CHECKOUT --integration 8470608781b4ab772597cef0d767e012b594f257 --require-complete
python3 -m unittest discover -s tools/product-release -p 'test_*.py'
python3 -m unittest discover -s tools/product-integration -p 'test_*.py'
python3 tools/product-integration/production_chain.py --mode arm-vm --scenario product --qemu-cpu cortex-a53 \
 --programs MATCHED_8470608_ARM_PRODUCT_PROGRAMS --test-driver MATCHED_8470608_ARM_LIBTEST \
 --revision 8470608781b4ab772597cef0d767e012b594f257 \
 --guest-root "$PWD/.local/FRESH_OWN_ARM_DEPENDENCY_ROOT" --kernel PRIVATE_GENERIC_KERNEL --qemu PRIVATE_QEMU
```

同一命令用`--scenario default`及对应legacy程序复验真正默认；Native/default/product/email用README的明确driver/原format3辅助输入。发行仍是原`tools/product-release/release.py`，unsigned Root仍委托原`tools/product-install/package.py`。完整固件只能在取得并验证锁定boot/modules后用原`tools/rootfs/scripts/build-rootfs.sh --lock-file firmware/base-system.lock.json`及原帮助要求的固定输入另跑，不能替换摘要。

生产有限顺序为`product-prepare → product-apply → product-inspect → product-activate → confirm-running-candidate → inspect-state`，不在宿主作fixture试验。来源/独立角色策略/信任/基线/数据/空间/能力不符即停止；Executing/Unknown先查询原journal，禁止盲重试、清floor/attempted、覆写备份或自动还原旧业务。`product-recover/rollback`只处理原安装器可证明的有限材料。共享生产入口、连续守卫及已支持组合的离线闭环均已完成，ARM时限失败已关闭；没有将生产接线留给本机。真实签名/设备验收由负责人另行授权。

公开[本轮静态证据](web4-production-wiring-evidence-20261111.json)及单列的[c771历史证据](web4-production-wiring-evidence-20261110.json)只含来源、制品摘要和脱敏结果；原始日志、fixture密钥/DB/身份材料留忽略的`.local/`。按任务书不改全局台账；**81.5%，20/169，加权10.6%**保持。
