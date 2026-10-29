# C13/C14 更新器失败矩阵接口与最小样例提案

2026-10-29，成员C / yh1030，UPD-06离线子范围。**阶段一：来源、接口与失败样例方案待审核，尚未实现或执行新用例。**

起始main完整SHA：`e48072ba9760c18f10c44847a1c6d94863eafd91`。负责人已合并PR #10，merge=`e48072ba9760c18f10c44847a1c6d94863eafd91`，被合并HEAD为`b66c0e1df606980524d94fb26585b4a73aa7f7e3`。
原C13只读预研基线`dfd11e9e75d7b532de5fcaeacbdb9fcd87e33215`的11个更新器文件与本基线逐字节相同；以下链接全部固定到本轮main。旧PR审核台账是否更新仍由主线处理，不自行将C09～12或本提案计作验收完成。

按[执行规则](../team/ADDITIONAL-OFFLINE-PACKAGES-2026-10-27.md#t6-maximum-offline-delegation)，新包第一份小PR先固定接口/来源/失败样例，主线审核后实现其余代码。本PR只新增本提案、追加本人交付报告，不改变生产接口、持久格式、Root授权、schema、签名格式、锁文件或其他成员文件。

## 输入、输出与故障证据

- 输入：临时0700应用根、合成Ed25519密钥及原format1/2完整应用组A1(epoch1)/C2(epoch2)，显式schema6。密钥只在后续运行时的私有临时目录生成，不进入源码、报告或日志。
- 调用：原Store::open/install_factory/stage/choose/select_runtime/confirm；不另造更新器、数据库恢复器或模拟成功接口。
- 输出：仅用例ID、阶段、真实API返回/错误类型、原文件及台账摘要、floor/candidate/attempted/confirmed与重开后选择；公开日志不含密钥/数据库/ELF。Store将底层IO折叠为Error::Io，不凭它断言具体errno。
- 观察顺序：故障前→错误返回后（内存/磁盘分列）→drop及保存目录证据→原Store重开→choose；open/choose会改变恢复状态，不能先调用再宣称是原故障现场。
- 每例独立对照根；权限失败必须普通UID真实触发，Root不得运行后误判通过。仅恢复本测试目录权限，不删除台账/孤儿目录、不清floor或延长候选尝试预算来换取通过。

审阅目标是以下实际接口、7类候选规格、5个最小失败样例及R1/R2等决策点；不是本PR已完成C13/C14矩阵的声明。


## C13 真实接口与状态边界

固定主线：`e48072ba9760c18f10c44847a1c6d94863eafd91`。仅静态核对，未执行C13新测试。

| 接口 | 输入/前置 | 实际行为及必须保留的边界 |
|---|---|---|
| [Store::open](https://github.com/Viper-Boss/RustSimHub/blob/e48072ba9760c18f10c44847a1c6d94863eafd91/crates/update/src/store.rs#L113) | 绝对目录；同EUID且无组/其他写权限、文件单硬链接；独占锁、key绑定 | 可能创建锁/versions，接纳合法state.next或清除无效next；不是纯只读函数 |
| [Store::install_factory](https://github.com/Viper-Boss/RustSimHub/blob/e48072ba9760c18f10c44847a1c6d94863eafd91/crates/update/src/store.rs#L312) | 合法完整签名包；仅建立/恢复出厂基线 | 已有confirmed时保留；不能借此确认任意候选、重置floor |
| [Store::stage](https://github.com/Viper-Boss/RustSimHub/blob/e48072ba9760c18f10c44847a1c6d94863eafd91/crates/update/src/store.rs#L345) | 没有待定候选；已有fallback先验证；签名、schema、floor、payload均通过 | 直接创建versions/hash和逐文件sync；最后写candidate/floor并持久化；不是版本目录rename发布 |
| [Store::floor](https://github.com/Viper-Boss/RustSimHub/blob/e48072ba9760c18f10c44847a1c6d94863eafd91/crates/update/src/store.rs#L207) | 读取当前句柄内存状态 | stage成功推进epoch；正常回退不降低，失败时需另查磁盘/重开 |
| [Store::status](https://github.com/Viper-Boss/RustSimHub/blob/e48072ba9760c18f10c44847a1c6d94863eafd91/crates/update/src/store.rs#L460) | 读取当前句柄内存状态 | 不是磁盘事务成功证明，也不证明健康/维护授权 |
| [Store::choose](https://github.com/Viper-Boss/RustSimHub/blob/e48072ba9760c18f10c44847a1c6d94863eafd91/crates/update/src/store.rs#L432) | 精确验证候选，第一次成功前持久化attempted | 再次选择或坏候选会清候选并回退，保留floor；只返回路径，不执行应用 |
| [Store::check_runtime_paths](https://github.com/Viper-Boss/RustSimHub/blob/e48072ba9760c18f10c44847a1c6d94863eafd91/crates/update/src/store.rs#L234) | runtime和payload_root为绝对同EUID且无组/其他写权限的目录；versions与payload_root同dev/ino | 预检查失败不应消耗候选；selected/next仅接受受限符号链接 |
| [Store::select_runtime](https://github.com/Viper-Boss/RustSimHub/blob/e48072ba9760c18f10c44847a1c6d94863eafd91/crates/update/src/store.rs#L269) | 先check_runtime_paths，再choose | choose成功后才设置程序权限、创建并发布selected.next→selected；后半段失败可能已消耗试运行机会 |
| [Store::confirm](https://github.com/Viper-Boss/RustSimHub/blob/e48072ba9760c18f10c44847a1c6d94863eafd91/crates/update/src/store.rs#L449) | 已attempted且hash精确等于候选，组完整合法 | 调用方须先完成该组健康检查；Store本身不评估服务健康；更新confirmed后persist |

### 签名及材料格式

[verify / verify_payload](https://github.com/Viper-Boss/RustSimHub/blob/e48072ba9760c18f10c44847a1c6d94863eafd91/crates/update/src/lib.rs#L88)：签名覆盖精确manifest字节；format只允许1/2，固定5/6文件及顺序。BOARD=ufi003-v02、arch=aarch64-linux、MAX_MANIFEST=8192、总payload上限24MiB；未知字段拒绝。
当前CLI schema=6。新合成样例显式使用schema6，不照抄旧单测默认schema5当当前契约。只使用合成Ed25519密钥，不生成/读取生产密钥。
合成64-byte AArch64 ELF头仅用于原verify_payload结构校验，不执行，也不能证明真实ARM程序可运行。

### Store测试、CLI、维护准入分开

[CLI run](https://github.com/Viper-Boss/RustSimHub/blob/e48072ba9760c18f10c44847a1c6d94863eafd91/crates/update/src/main.rs#L45)：除--build-info之外先检查EUID0；不得删门禁或把正常用户Store库测试说成Root CLI准入验证。
[admit_stage](https://github.com/Viper-Boss/RustSimHub/blob/e48072ba9760c18f10c44847a1c6d94863eafd91/crates/update/src/admission/programs.rs#L179)：真实维护准入涉及签名程序、数据锁和有效systemd；C13只读参考，不伪造健康JSON或改其生产实现。authorization_verified=false不代表维护权限已通过。
UPD-01已由主线按真实设备记录收口；UPD-02维护授权与C26健康评估是后续独立范围。

### 发布阶段与失败观察

阶段P0：锁定/验证；P1：逐文件写入versions/hash并同步；P2：修改句柄内存candidate/floor；P3：写新state.next→sync父目录→rename state.json→sync父目录；P4：choose持久化attempted后返回路径；P5：selected.next→selected；P6：精确confirm持久化。
版本目录并非整体原子rename；可执行资格由持久化台账与verify_version共同决定。禁止宣传“任何失败都无新文件”。

### 待主线确认的静态风险（未复现，不称已证实缺陷）

R1：stage/choose/confirm在persist之前修改self.state；persist失败可能导致同句柄status与磁盘台账不同。CLI遇错退出降低后续复用机会，但库调用方的错误后可复用/必须重开语义需要确认。准备案例会分别记录内存、state.json、state.next及重开结果，不预设原子内存回滚。
R2：select_runtime先choose再发布链接；后半段失败可能已消耗candidate attempt。应验证旧selected保持与下一次回退，不延长预算或自动重复候选来让测试通过。
R3：open可能清除无效next或接纳有效next；重开是实际恢复动作，观察前先留原始文件摘要和清单，不能把正常清理误报为来源零变化。

## C13 拟补矩阵：7类候选用例，尚未实现/执行

编号用于方案追踪，不是完成量。format1/2可作为参数，不能把组合数量折成新业务能力或新增进度。

| ID | 故障/调用 | 验收重点 | 增量与优先级 |
|---|---|---|---|
| C13-P01 候选目录创建真实EACCES | 普通用户真实create_dir写失败；stage(合法C2) | 返回Error::Io，不返回成功hash；A1所有原文件及state.json摘要不变；C2目录未创建；仅恢复测试目录权限后drop/reopen仍floor1、confirmed=A1、无candidate；choose为A1 | 已有fallback后的真实写失败；现有坏包/孤儿目录测试不覆盖；P0 |
| C13-P02 stage末端state.next冲突 | create_new(state.next)真实EEXIST；stage(合法C2) | 返回Io；旧state.json/A1摘要保持；记录C2完成但未获台账资格的目录清单；分别记录同句柄status/floor与磁盘值，不预设内存回滚；drop/reopen由产品处理torn next；仍floor1/confirmed A1，无candidate；choose不得采用孤儿C2；保留C2而非删库通过 | 真实stage执行到持久化失败，而非只手工造残留后open；P0 |
| C13-P03 choose尝试标记持久化失败 | attempted内存更新后的persist EEXIST；choose() | 本次返回Io，绝不返回候选可执行路径；state.json保持未attempted候选，floor2不降；记录内存attempted而非把它当持久成功；drop/reopen后第一次成功choose才返回C2并持久化attempted；再次重开后选择A1；不能把失败调用计作已执行候选 | 选择路径的真实持久化失败，区别于正常第二次回退；P0 |
| C13-P04 confirm提交失败不伪确认 | confirmed内存更新后的persist EEXIST；confirm(C2精确hash) | 返回Io；旧磁盘台账仍confirmed A1/candidate C2/attempted true，A1/C2签名文件保持；同句柄status可能与磁盘不同，作为静态风险的待验证证据记录；drop/reopen不能宣称C2已确认；随后choose应回退A1，floor保持2；不删state.json/清floor重来 | 确认成功边界与磁盘原子性；不是新增健康判断或维护授权；P0 |
| C13-P05 choose之后运行链接发布EACCES | 通过路径预检和choose持久化后，创建selected.next真实写失败；select_runtime(runtime,payload_root) | 返回Io，原selected仍精确指向A1；没有对外返回C2；磁盘floor2、candidate C2/attempted true；不误断言失败没有消费attempt；仅恢复测试runtime权限后drop/reopen并select_runtime应回退A1、清candidate、floor仍2；不重置预算以重选C2 | 区别于现有runtime预检查拒绝和手工预置selected.next；P0 |
| C13-P06 重复确认的持久状态保持 | 重复确认无候选的拒绝路径；再次confirm(C2)，再drop/reopen观察 | 返回Denied；state.json/签名组摘要不变，confirmed仍C2、candidate为空、floor2；新Store的choose保持C2；禁止把拒绝改成健康确认成功 | 现有错hash/未choose断言的持久状态强化；若审查认为重复则只补原测试，不另立冗余用例；P1 |
| C13-P07 有效但陈旧或错信任state.next | 格式合法但低floor或不同key_sha256的next；Store::open(原key, schema6)，先观察再choose | 产品拒绝采纳next、按原实现清理；原state.json摘要、floor2和原key绑定保持；原candidate/confirmed关系按原台账保持；不得复活旧候选/降低floor/采纳异key；不用手工篡改签名或把解析失败误当key/floor守卫已覆盖 | 补已有台账时的合法陈旧/异信任next；既有torn/缺台账恢复不等同此项；P1 |

### 通用观察规则

每个用例独立私有临时根、普通非Root UID；只用合成A1(epoch1)、C2(epoch2)、后续C3(epoch3)。各次Store::open使用原公钥/schema6。每个权限故障先验证EUID非0，否则拒绝本用例，避免Root绕过权限得到假通过。
在动作前、错误返回后、drop/reopen前后分别收集state.json/state.next/versions清单摘要、Store::status/floor、selected目标及API返回值。摘要和状态摘要可公开，不打印密钥或材料正文。
内存和持久状态分开；open/choose本身可能恢复或改台账，观察顺序固定。保留完整/部分孤儿目录，不通过删state.next、清state.json或改floor通过测试。恢复本用例目录权限属于显式清理，须先保存失败证据。
直接调用原Store API，不自建更新器，不调用系统服务/设备CLI，不执行合成ARM头。C13通过也不表示健康判断或Root授权通过。

### C14单列，当前不执行

| 后续ID | 需要的真实证据 | 执行约束 |
|---|---|---|
| C14-D01 | 独立tmpfs写满的实际ENOSPC、更新器错误及重开状态 | 预设2MiB只作用于私有mount/net命名空间；Root仅准备，测试降权；先核对tmpfs/容量/UID；不能填宿主盘。具体容量是否足够保留有效基线须实施时实测 |
| C14-D02 | 子进程在有记录的发布边界退出/被终止后，同目录由原Store重开 | 父子握手/精确子PID；不能仅随机sleep后kill声称命中特定fsync；无公开同步点时先提最小测试接入供审 |
| C14-D03 | 签名/schema/outbox/回执兼容的真实模块证据 | 复用现有升级兼容与转发测试，不能只检查自造JSON；不改其他成员或主线生产格式 |
C09～C12阶段跑过的forwarder ENOSPC不是更新器ENOSPC；已有Root metadata也不是C14完成。

## 最小失败样例方案（未执行）

以下都是可审阅的复现设计，observed_result为空；没有创建数据库、签名密钥或故障目录。不能作为已证实缺陷/已通过测试证据。

### C13-P01 · 候选目录创建真实EACCES

前置：通过install_factory建立A1/floor1；保持Store开启，仅把其versions目录临时改为0500。
动作：只调用原`stage(合法C2)`，故障点为普通用户真实create_dir写失败。
必须观察：返回Error::Io，不返回成功hash；A1所有原文件及state.json摘要不变；C2目录未创建；仅恢复测试目录权限后drop/reopen仍floor1、confirmed=A1、无candidate；choose为A1。
控制对照：另一全新临时根保留可写权限/不预置next，运行同一真实API应正常完成；不在故障样本上删除证据后重命名为对照。
来源：[stage/persist/choose/select_runtime/confirm源码](https://github.com/Viper-Boss/RustSimHub/blob/e48072ba9760c18f10c44847a1c6d94863eafd91/crates/update/src/store.rs)。

### C13-P02 · stage末端state.next冲突

前置：A1/floor1；预置私有普通state.next为损坏JSON {torn，禁止覆盖/删除该注入输入后伪装成功。
动作：只调用原`stage(合法C2)`，故障点为create_new(state.next)真实EEXIST。
必须观察：返回Io；旧state.json/A1摘要保持；记录C2完成但未获台账资格的目录清单；分别记录同句柄status/floor与磁盘值，不预设内存回滚；drop/reopen由产品处理torn next；仍floor1/confirmed A1，无candidate；choose不得采用孤儿C2；保留C2而非删库通过。
控制对照：另一全新临时根保留可写权限/不预置next，运行同一真实API应正常完成；不在故障样本上删除证据后重命名为对照。
来源：[stage/persist/choose/select_runtime/confirm源码](https://github.com/Viper-Boss/RustSimHub/blob/e48072ba9760c18f10c44847a1c6d94863eafd91/crates/update/src/store.rs)。

### C13-P03 · choose尝试标记持久化失败

前置：A1；C2已成功stage，floor2、attempted=false；预置torn state.next。
动作：只调用原`choose()`，故障点为attempted内存更新后的persist EEXIST。
必须观察：本次返回Io，绝不返回候选可执行路径；state.json保持未attempted候选，floor2不降；记录内存attempted而非把它当持久成功；drop/reopen后第一次成功choose才返回C2并持久化attempted；再次重开后选择A1；不能把失败调用计作已执行候选。
控制对照：另一全新临时根保留可写权限/不预置next，运行同一真实API应正常完成；不在故障样本上删除证据后重命名为对照。
来源：[stage/persist/choose/select_runtime/confirm源码](https://github.com/Viper-Boss/RustSimHub/blob/e48072ba9760c18f10c44847a1c6d94863eafd91/crates/update/src/store.rs)。

### C13-P04 · confirm提交失败不伪确认

前置：A1；C2成功stage且choose后attempted=true；预置torn state.next。
动作：只调用原`confirm(C2精确hash)`，故障点为confirmed内存更新后的persist EEXIST。
必须观察：返回Io；旧磁盘台账仍confirmed A1/candidate C2/attempted true，A1/C2签名文件保持；同句柄status可能与磁盘不同，作为静态风险的待验证证据记录；drop/reopen不能宣称C2已确认；随后choose应回退A1，floor保持2；不删state.json/清floor重来。
控制对照：另一全新临时根保留可写权限/不预置next，运行同一真实API应正常完成；不在故障样本上删除证据后重命名为对照。
来源：[stage/persist/choose/select_runtime/confirm源码](https://github.com/Viper-Boss/RustSimHub/blob/e48072ba9760c18f10c44847a1c6d94863eafd91/crates/update/src/store.rs)。

### C13-P05 · choose之后运行链接发布EACCES

前置：A1已select_runtime；C2成功stage；runtime改0500且保留原selected=A1；versions与payload_root同dev/ino。
动作：只调用原`select_runtime(runtime,payload_root)`，故障点为通过路径预检和choose持久化后，创建selected.next真实写失败。
必须观察：返回Io，原selected仍精确指向A1；没有对外返回C2；磁盘floor2、candidate C2/attempted true；不误断言失败没有消费attempt；仅恢复测试runtime权限后drop/reopen并select_runtime应回退A1、清candidate、floor仍2；不重置预算以重选C2。
控制对照：另一全新临时根保留可写权限/不预置next，运行同一真实API应正常完成；不在故障样本上删除证据后重命名为对照。
来源：[stage/persist/choose/select_runtime/confirm源码](https://github.com/Viper-Boss/RustSimHub/blob/e48072ba9760c18f10c44847a1c6d94863eafd91/crates/update/src/store.rs)。

### 首次审查要确认的具体事项

1. R1错误后同句柄可复用契约：是否要求库在persist失败后恢复内存，或明确要求丢弃重开？当前源码只证明先修改后persist，本阶段没有真实复现或定性。C13先如实记录，并保证不误把status当磁盘确认。
2. R2链接发布失败后的attempt消耗：方案按当前“先记录attempt后发布链接”设计验证回退；若想改变重试行为，另立生产兼容提案，不能在测试里放宽。
3. 是否保留C13-P06为独立用例，或把持久状态断言补到原错误确认测试，避免重复测试。
4. C14精确退出边界若无法通过公开接口稳定观察，是否允许store.rs现有cfg(test)区最小同步接入；不新增默认生产feature或替代Store实现。
这些问题供未来接口/用例审查，不要求用户现在批准生产改动；先完成当前PR10复审前置。


## 实施路径与待审事项

R1：persist失败后同句柄是否允许复用，或要求调用方丢弃重开？先如实记录内存/磁盘差异，不在测试里私自承诺回滚。
R2：select_runtime后半段链接发布失败已消费候选尝试，按当前行为验证回退；改变重试语义属于另立生产提案。
R3：重复confirm用例是否只强化已有测试，避免重复？
R4：C14精确persist内部中断需要最小cfg(test)同步点，是否允许接入？未获审前不加生产feature或替代文件系统。

后续允许路径：crates/update/tests/、tools/test-update-failure-matrix.py、docs/update/及本人报告；确需私有API时仅既有store.rs cfg(test)区的最小接入。全部实现等待本接口PR审核；不改生产store/admission/Root单元或第三方依赖。先接公开API的普通UID用例，再单列需要命名空间的C14专项，不因缺Root静默跳过后称全过。

WSL固定Git源码归档，source/TMPDIR/target在Linux原生目录；现有wsl_test.py没有update suite，不能使用虚构的--suite update。后续真实命令包括cargo fmt --all -- --check、cargo test -p rustsimhub-update --locked --offline、该包all-targets严格Clippy；新的驱动命令仅在工具实现后提供。它们在本提案阶段均未运行。

已有13项kernel/DTB隔离测试和C09～12的15项恢复测试属于各自历史范围，不计作本包新更新器测试。SIGKILL不能证明硬件掉电/fsync断电持久性，合成ARM头不执行，Root/ARM/实际升级恢复由主线验收。

来源许可：项目Rust/工具原SPDX与音频/内核等各自许可均保留；本提案不复制第三方实现或新增依赖。Codex辅助静态盘点/整理，未估造有效工时。整体固定P0～P7仍80%。
