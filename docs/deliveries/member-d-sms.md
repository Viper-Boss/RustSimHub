# 通信观测工作包交付与主线接回台账

2026-10-29 [D-21请求授权与私有能力](../ble/recovery-capability.md)：代码6c018ca把认证请求限定到当前Auth/设备/连接/realm/grant代次、动作/正文及单调期限，真实Store前后复核，过期/重放/断线拒绝。新增11项、完整302项与严格Clippy/fmt通过。生产验证器默认缺失，A会话/浏览器/IPC/Wi-Fi与持久操作账本仍待；默认Core schema6、设备未动。D仍余6、lead20/118、原81.5%、20/169保持。

2026-10-29 [D-21恢复授权管理局部实现](../ble/recovery-owner-management.md)：用户允许先独立分支离线实现、合并前审核。代码c5f9ea7已实现显式候选schema7、真实Store事务/CAS/备份隔离及近期Owner管理，新增16项、总291项与严格Clippy/fmt通过。默认Core仍schema6；实际authorize/私有能力、浏览器恢复key、A会话和IPC/Wi-Fi未接，不动设备。D仍余6、lead20/总118、原81.5%、20/169保持，未将局部代码算整步完成。

## 2026-10-29 D-26局部修复与公开看板

[蓝牙清理失败责任](../testing/ble-cleanup-quiescence.md)：真实库最小反例先失败，再在代码99a7f163c23e7afa7cb344bdad4b48b3d66e596d中修正quiescent漏查cleanup_failed。新增4项/25向量，BLE42+灯控24、严格Clippy/fmt和Python24通过；无新依赖、协议、安全端口实现或设备操作。D-26整体仍待其他生产前置，不扣步，lead20/D6/118保持。A按用户要求继续原分工。原固件经独立复核为81.5%，行动台账20/169、10.6%。

[独立公开看板](../progress/PUBLICATION-20261029.md)已built、部署与实际HTTPS摘要核对通过，原固件仓库仍私有。发布看板不是将产品PR合入main；每批完成仍据实际验收扣账，不用截图/CI/分支标题冒充交付。

## 当前结算 · 2026-10-28 GATT 离线库批次

**最新同步后截面：** 已接续PR50更新 `2b13b25f4ffad0fed04643fedadbdec60fed9f64` 与Claude main dfd11e9，保留用户新灯色及UPD-04/STAB-03/SMS-05，最新台账20/169、加权10.6%、余149、原80%；lead20/D6不变。新截面归档 `4898875176e75cec26e30f532eb398b7361f64ade94beb604e6d4921800e3c5a`、日志 `d8ba77e0d818a2881b839aa9cd810d82a88b9a8da61ad93aa02440638cdd3d3f`：蓝牙38+灯控24、严格Clippy/fmt及Python23全部通过，Node看板/生成一致性通过。下文旧数量/23灯控是先前截面证据，保留作历史。未宣称Pages或main已包含本PR。

**D-17～20再由主线交付4步，累计lead接回D-01～20共20步；RedamancyGTW现余D-21～26共6步。四位成员98+lead20=118不变。** BLE3-01/04离线责任移lead，整项仍未闭环。D-21～26按原要求继续做，其他成员范围、实际作者和已审数量不动。[接线文档](../ble/gatt-adapter-integration.md)明确生产缺口。

| 步骤 | 实际交付 | 验收边界 |
|---|---|---|
| D-17 | crates/ble的固定manifest/UUID、预算/帧类型、拥有型Send future端口，测试直接核对冻结JSON | 未改冻结接口JSON，无生产依赖；serde_json仅复用现有开发依赖 |
| D-18 | 实际Lifecycle的注册/部分失败补偿/撤销/Release/epoch/连接代次、5秒截止及迟到清理 | 缺安全端口拒绝；无法证实清理保持unknown，不假装真实BlueZ已接入 |
| D-19 | 实际Transport接生产分片/生命周期与注入SecurePort，cfg(test)外部端口验证洪泛、过期/迟到、通知确认丢失、同peer重连/旧owner/第二peer | 未实现密码握手/Core恢复权限；测试opaque回复不是认证；不自动重发Unknown |
| D-20 | 38个真实Rust库测试、严格Clippy/fmt、固定向量/预算、外部夹具入口和主线接线报告 | 真实BlueZ/HCI/ARM/密码互操作/安装待验，BLE3-01/04不标done |

最终代码截面见本批代码提交be02191；源码归档27678878ec04ca0efdcae203ba15083dcb1858223484bc5cc369b7ae637e81f6，日志990a51a6b4b34ec71ea5823394c687014403381797419f87254ec3fe0d5460ae。隔离Linux/Rust1.98、UID65534、无网络/设备：蓝牙38+独立灯控23、严格Clippy/fmt、Python进度23全部通过，后续20/6分工与当前PR HEAD验证另补。原Windows缺link.exe和两处早期测试lint失败均保留；不关闭lint，不伪称Windows原生编译通过。

20/6分工、交付报告与看板生成后的最终归档 `31dcca7e8d1e0eff57781fb962d7f49edf4f168b74d2ccf33fe439656631b523`、日志 `67e8c9bb4a90d4bb89d8edb815adc3da3396b918bf9d71dc3f4d0e5a8c5b7359`：蓝牙38+灯控23、严格Clippy/fmt、进度Python23全部通过；Windows Node面板/唯一归属检查、生成一致性、冲突标记零与diff检查通过。后续仅补本段结果；精确代码SHA为 `be0219119add5517161a8429328b5c0d1740641d`。本批PR堆叠在D-13～16的PR #50（66bb456c7768d1790dd79e0432941e87c6ef4dc0），前置正常合入前不绕过其审核。

本批没有Core/Auth/policy/wire/media生产接线或服务/UID/Root变更，不动Claude设备任务。D-11持久身份、LED提示代次与全部新增模块授权缺口继续保留。台账仍18/169、加权9.6%、余151，原P0–P7约80%；这些是交付分支截至合并3d7f848的截面，不是新增功能验收或当前Pages发布证明。以下灯控/短信批次为历史结算。

## 当前结算 · 2026-10-28 灯控输入批次

后续同步已合入的Claude main `dfd11e9e75d7b532de5fcaeacbdb9fcd87e33215`：保留UPD-04实机回选及用户接受的STAB-03证据，未改其AP地址/其他生产实现；按用户新未读配色更新本批策略断言（短信黄紫黄紫、未接红紫红紫各1.2秒，双组红黄紫重复1.8秒），不倒退生产配色。相加已有SMS-05后，本分支最新为20/169、10.6%、余149，原80%；下段18/169为前一次main截面，非当前计数。分工仍lead16/D10，蓝牙后续在独立PR #51结算。

新增接回 D-13～D-16，详见[真实输入边界与失败证据](../testing/led-input-boundaries.md)。D-13盘点实际策略/Core/Store与权限；D-14补公开策略和真实文件矩阵；D-15固定26向量对真实Store验证；D-16完成256项、严格Clippy/fmt、23进度检查及报告。复现并修复Core/灯控打开FIFO会在类型检查前阻塞；两个真实子进程反例失败后，最小加入O_NONBLOCK。没有新增依赖、灯色/节奏/权限/租约或产品路径。

**本批再减4步：lead累计D-01～16共16步，RedamancyGTW剩D-17～26共10步；四位成员102+lead16=118，总量不变。** LED-07/SEC-06离线责任随本批移到lead，D-26仍补新增跨模块输入。未知/未实施明确保留：业务提示无SIM epoch、瞬时跨进程失效未实现，真实GPIO/冷启动未测，LED-07/SEC-06不能标done。D-11持久通话身份仍仅提案。

代码6078a3766361cacfe4d7d059867a27ae4ed7881b，最终源码归档f9be9c74a3ca7f543a6c41f9b7af7d1ea061394e5189cc63e7c409b65f512f93、日志6a33a43617dc6c0fea0189fac79e0e72ac8bf4c6b7d4c45cd6e2610ed20d4110，执行环境/完整命令见报告。256项0失败/忽略，严格检查与23进度测试通过。早期lint/夹具失败保留，未包装为全绿。后续分工文档/看板一致性另补，当前接回待PR精确HEAD审核，不给成员预加已审数或代码贡献。

合并Claude主线3d7f848的UPD-01实机闭环后，工作分支同时保留SMS-05，故为18/169、加权9.6%、余151、原里程碑80%；这不是本批新增功能验收。设备工作由Claude独立执行，本轮未接触设备/签名/安装。以下第二轮和首轮为历史结算。
## 当前结算 · 2026-10-28 第二轮

主线新增交付 D-05～D-08 的短信内容/候选/持久发送恢复包，以及 D-09～D-12 的通话重观察审计、失败矩阵、防串线最小修复与持久身份提案。**D-11 的跨启动完整去重方案仍是提案，尚未实现；交付该提案不等于去重功能完成。** 所有接回步骤待对应PR当前HEAD审核，不记为 RedamancyGTW 完成。

**本轮接回8步，累计主线12步；RedamancyGTW 从22步再减8步，当前保留 D-13～D-26 共14步。** 118步骤总量不变：四位成员持有106步，主线持有接回12步。SMS-03/05/06与CALL-08离线责任移到lead，与此前SMS-02/04一致。另按[SMS-05原判据验收](../VALIDATION-SMS05-CANDIDATES-20261028.md)单独关闭疑似重复/恢复查询动作：本交付分支为17/169、加权9.0%、余152；main在本PR合入前仍为16/169。原80%和其他功能/实机状态保持。

| 步骤 | 本次实际交付 | 范围和未完成项 |
|---|---|---|
| D-05 | 实际MM属性解码→Store：中文/emoji/组合字符/ZWJ/空正文/二进制、4096字节边界、缺字段/错类型/混合载荷/未完成及非接收状态 | 新增回归，已有行为通过；依赖MM解码，不新增PDU解码器，未测真实运营商分段 |
| D-06 | 真实SQLite重开/备份恢复中，不同来源同正文保留独立ID；候选边、删除/恢复保持；跨卡不关联；旧Owner在恢复后被隔离 | 已有行为补验；不修改网页、权限或schema |
| D-07 | prepare/begin事件失败、bind/permit写入失败、确认失败；6阶段真实进程退出；真实Auth/Store/发送Worker重建后查询同一Unknown | 不会重新Create/Send；正常重开Prepared仍待显式授权提交，备份恢复将旧Prepared隔离为Unknown；不声称自动重发 |
| D-08 | 分类别记录、固定源码、完整命令、失败夹具修正及实机补验清单 | 223项阶段回归通过，待当前PR审核 |
| D-09 | 读取Core和Store实际来源链、MM owner/line/path/revision/UUID图；明确持久身份缺口 | 见[身份边界与提案](../testing/call-reobserve-identity.md)，不按号码猜合并 |
| D-10 | Store并发/终态重观察/重开/backend变化/换SIM/事务失败；真实Core peer的无关SMS通知和Call退休 | 4个Store+1个peer新增测试；没有真实MM/硬件观察 |
| D-11 | 修复跨SIM来源碰撞会重写旧历史的问题，修复前复现失败、修复后通过；交持久身份/迁移候选提案 | 最小保护已实现；跨启动全场景去重未实现、schema方案待审 |
| D-12 | 通话代码提交、228项完整Core/Store、严格Clippy/fmt及23项进度回归 | 离线包提交；CALL-08不标done，不修改拨号/接听/挂断/媒体/租约 |

### 本轮固定来源

- 基线与前置：PR #34 的 `8edb20334fa05e97e1d7efe33e1f87990fa5e7b2`；第二批分支 `codex/lead-member-d-sms-content-20261028`。PR #34未合入时，本PR先以其功能分支为base，避免把首批误称为本轮新增；前置合并后正常接续main。
- D-05～08代码：`07a98395da961898d199d57c0f65db26315875a5`；D-09～12代码：`adb7bb6e1598e1a2a5a130821c996733d19c4e65`。
- 本轮新增19个Rust测试入口（含1个仅供子进程调用的入口），其中6种实际进程退出场景由父测试分别启动、核验；不是19种实机测试。
- 223项阶段归档 `1c40a40ef6818bd8b15e52d36c02ff7919afcbec9e8ca0cfbac53e70c785dbb4`；加入通话修复后228项归档 `4a26803a5e5fce5765ba2b3bc25eeb9bf3949086d73299e12967527f0143ea6f`，日志摘要 `f9deae6610fb03cb277d091ef84439c374b57255fa34fd99e02734737b8a7cc1`。
- 环境：同首批WSL/Linux、Rust1.98、UID65534/无capabilities、无网络/设备、只读源码/缓存，临时库与私有peer。没有签名、部署、真实发送或拨号，不与CC争用设备。

### 结果与补验

```sh
cargo test -p rustsimhub-storage -p rustsimhub-core --all-features --locked --offline -j3
cargo clippy -p rustsimhub-storage -p rustsimhub-core --all-targets --all-features --locked --offline -j3 -- -D warnings
cargo fmt --all -- --check
python3 -m unittest discover -s tools/progress -p 'test_*.py'
python3 tools/progress/build.py --check
```

最终阶段：Core167 + Store61 = **228项通过，0失败/忽略**；Clippy/fmt、23项Python进度检查、Node短信管理/按人看板与生成一致性通过。包含12/14分工、SMS-05闭环和测试分类修正的最终测试截面摘要为 `3572f4cf81cc4dabe86fd944e44ccb9b9f3881e5ffbdb43fc59a35321ae3afcf`，日志摘要为 `4121535cc2cd5da2fd4d4a5564e242f56aa358ebd5ec566142579ce58cbed201`；后续仅补本文验证结果。贡献统计把已核对的Core cfg(test)独立夹具改列测试，重算固定main历史，不将其算产品码或预记本分支行数。PR HEAD及CI实际结果在PR说明核对。

保留的夹具失败：未提交确认场景最初将Connection传给需Transaction的event助手，随后分支中已移动operation ID未显式声明不返回；改为真实Transaction且强制退出不展开Drop。MM完整PDU类型夹具最初用i32，真实解码器正确拒绝，改成与接口一致的u32后通过。均未为测试放宽生产类型/约束。通话跨SIM测试在原实现上确实失败，才增加持久line/fingerprint保护。

新发现并修复：通话历史跨SIM误更新。新增测试/已有行为补验：短信内容、候选、发送事务与进程恢复、持续观察下的通话去重。未实现：MM原始PDU解析、无可靠身份时的跨启动完整通话去重方案。未运行：真实发送授权、真实分段/SIM满、双端补同步、换卡、真实MM重建/通知丢失、ARM、冷启动与当前设备安装。恢复后Owner隔离测试只使用可信离线恢复入口，不代表开放网络认领或新建Owner。

后续主线按真实结果集成/发布；D-13～26的灯控输入、GATT及恢复/远程媒体仍保留待办。下面首批记录是历史结算，不覆盖本节当前12/14数量。

## 2026-10-28：主线完成 D-01～D-04 的离线实现

用户明确说明 RedamancyGTW 尚未开始，并授权主线直接接手。执行者为项目负责人 Viper-Boss（本会话 Codex 辅助），不计入 RedamancyGTW 已完成工作；不更改历史 Git 作者或他人提交。

| 原步骤 | 本轮交付 | 当前负责人 | 边界 |
|---|---|---|---|
| D-01 | 实际 source/线路/ID/游标关系与不变量见下文；给出可重放的最小输入序列 | lead | 已完成离线分析，待本 PR 审核 |
| D-02 | 真实 Store/SQLite 的 70 条消息多页、乱序重观察、删除恢复、事务失败、查询失败、重开及恢复游标测试 | lead | 4 个新增测试，通过；不是双端/运营商验收 |
| D-03 | 真实 observer 的私有 peer RPC 重现对象读取与退休竞争；补代次检查及列表失败/缺失对象恢复 | lead | 3 个新增测试，通过；未连接真实 ModemManager |
| D-04 | Core/Store 共 209 项、严格 Clippy、格式检查；源码与失败/成功证据固定 | lead | 本 PR 提交交付；审核和设备验收分别记录 |

**数量结算：RedamancyGTW 原 26 步，减去本轮接回 4 步，余 D-05～D-26 共 22 步；主线接回 D-01～D-04 共 4 步。** 转移的是这四步及 SMS-02/SMS-04 离线实现责任，不是把短信模块或整个固件判定完成。个人已审步骤仍由 PR 当前 HEAD 对应的审核记录计算，不把执行者自测写成成员验收。

名册的 118 个步骤总数不变；四位成员当前持有 114 步，主线持有接回的 4 步。下一批 D-05～08、D-09～12 及后续 BLE/媒体包仍为 RedamancyGTW 待办；后续主线继续完成时逐批在本文件追加减账，不一次性撤销全部任务。原 D 包 33～75 小时为 26 步历史计划预算，本次未把其当作当前 22 步剩余工时或实耗。

## 固定来源与代码

- 起始 main：`7407976c8f566df96394310e690db1650c10598e`。
- 代码提交：`17a63d56df4bd7a3edb281dc5a788cb901e86578`。
- 分支：`codex/lead-member-d-sms-recovery-20261028`。
- 生产修改：`crates/core/src/observer.rs`、`observer/watch.rs`。
- 测试：`observer/checks/lifecycle.rs`、`crates/storage/src/inbox_recovery_tests.rs`，既有模块仅接入测试。
- 来源许可：沿用本仓库 AGPL-3.0-only；无复制外部实现、无新依赖、无 schema/协议/颜色变更。

## D-01：实际状态关系

1. Core 的 `session` 绑定 broker UUID、MM 唯一 owner、modem/SIM 路径；SimFingerprinter 派生身份摘要。`Stable` 观察稳定后，由 Store 串行 writer 提交 Ready 线路。
2. `Sources` 使用 SMS 对象路径和 incarnation（`path#n`）作为 source；完整内容摘要仅用于识别同一对象内容变化。`saved_sources` 在同一验证过的 line 下恢复来源映射。
3. Store 以 `(line_epoch, source)` 找既有记录；相同来源/内容返回原 UUID，不新增接收事件；新来源即使正文相同也保留，只标 duplicate_candidate。
4. `inbox` 的 cursor 是消息 seq，`inbox_events` 的 cursor 是事件 seq；两者共享 db_epoch，但不能混用 seq。事件与入库、删除/恢复在同一 SQLite 事务提交，客户端只在成功处理后保存自己的事件 cursor。
5. 同一 db_epoch 的历史游标可追赶仍保留的记录；格式错误、超过当前表最大 seq 或恢复后旧 db_epoch 的游标被拒绝。备份恢复更换 db_epoch，因此客户端必须重新同步，不能沿用恢复前游标。
6. 换 SIM/owner 失联等由线路 fence 阻止旧异步写入；本次进一步覆盖 SMS 对象自身退休，不将“线路未变”当成“对象仍是原代次”。真实同内容短信不会自动合并。

## D-03：已证实缺陷与修复

最小序列：已 Ready 的同一 SIM → 开始 `GetAll` 读取 SMS/1，并用信号量保持正文回复 → 观察器收到 SMS/1 的退休通知 → 放行旧回复 → 原代码确认入库 1 条，预期为 0。

修复前测试明确失败于“旧回复不应成为已确认短信”：实际 1、预期 0。修复在 session 已消费的 `(fence, revision)` 上绑定整个 reconciliation 周期；SMS 读取完成后检查路径是否在该 revision 后退休，以及队列是否溢出要求全量重建。实际 writer 中的 `receive_sms_if` 再检查一次，覆盖等候写入期间的失效窗口。其他路径退休和普通 Wake 不影响本条消息；下一周期按新代次重新读取，可正常入库。

这仅处理已收到/处理的对象生命周期通知；完全丢失通知、同路径同正文且缺少其他来源证据时，不能据此宣称能识别所有物理新短信。既有周期 List、内容摘要、线路隔离和不确定时保留候选的语义不变。

## 测试矩阵与结果

| 输入/故障 | 真实被测入口 | 观察结果 |
|---|---|---|
| 70 条合成消息，删除前 34 条，反向重观察 | Store receive/inbox/events/delete | 36 条可见，UUID 不变；1/16/32 页大小返回一致；105 个持久事件不重复 |
| 重开数据库后恢复第一条 | Store open + restore visibility + event catch-up | 旧游标仍可追赶唯一 sms_restored；恢复原 ID，其余删除标记保持 |
| 收件事件插入触发 SQLite ABORT，再重开重试 8 次 | 真事务 + inbox_event_head | 失败无消息/事件；同一来源成功后只有 1 次 sms_received |
| TEMP VIEW 制造实际事件读取/解码错误，再解除 | 真 SQLite 查询 + inbox_events | 原游标保留，恢复后仍读到同一个持久事件；重复读取 seq 不变 |
| 非法/溢出游标，0/33/最大页长，恢复前旧 realm | Store cursor 校验 + backup/restore | 拒绝且不推进；恢复后可全量读取原消息 ID |
| GetAll 中途退休，随后新周期重读 | 真实 reconcile + 私有 Unix peer + 信号量 | 旧回复 0 条，新读取正常成为 1 条 |
| 其他路径退休、普通唤醒、退休队列溢出、SIM fence、断连 | 生产 sms_valid/Notice | 只使匹配路径或明确失效范围无效，关闭 watcher 后不发布成功 |
| List 失败，再恢复且有一条无法读取的对象 | 真实 reconcile/Store/saved_sources | 原 2 条/ID/事件不变，缺失对象增加读错误，未伪装成本地空箱 |

```sh
cargo test -p rustsimhub-storage -p rustsimhub-core --all-features --locked --offline -j3
cargo clippy -p rustsimhub-storage -p rustsimhub-core --all-targets --all-features --locked --offline -j3 -- -D warnings
cargo fmt --all -- --check
```

执行环境：WSL/Linux、Rust 1.98.0、UID 65534、空 capability、无网络、无设备、只读源码及缓存、临时 SQLite 和私有 peer Unix socket。结果 **209 项通过、0 失败、0 忽略；Clippy/fmt 通过**。验证归档摘要 `1470c3cb64aaf5a4298cc29cd6bbcbd9d054c201e4906128367ed1fb19deda5f`，日志摘要 `f8ad1914ef1a914ec7393187fedcbb492edc015e7fe4efb3872cf8cf6fee1d90`。该归档是代码验证截面，后续文档/面板及最终 PR HEAD 的检查另列，不冒充自指的完整提交归档。

保留的前置失败：首版夹具的 ObjectManager 在注册时也读取属性，干扰了门控；改为无 ObjectManager 的最小 peer 后才得到真实生产缺陷复现。另一次新测试误以为 InboxCursor 实现 PartialEq，编译失败，改为分别比较 realm/seq；未修改生产游标类型来迁就测试。

## 未完成与后续

最终包含分工/面板改动的源码截面摘要为 `3c3d191d40e824c066317afcb69f546f60cb859f88a5e28d3ce8f819c12724e7`：同一隔离 Linux 环境再次通过209项Rust测试、严格Clippy/fmt、23项Python进度/真实Git归属测试和生成一致性；Windows Node面板检查通过。首轮Python在Windows沙箱临时Git目录受限而失败，另有新增测试使用公开快照缺少git_names的夹具错误，修正为真实名册后Linux23项全部通过；保留前次失败，不宣称Windows全套通过。

看板会显示主线接回4步、成员D22步；已审数仍为0，等待本PR当前HEAD审核。代码贡献仍统计于 `origin/main` 的真实Git截面，本PR未合并前不将新增行数预记主线。当前全局为原80%、16/169（加权8.4%，剩余153），本次不增加完成动作。

- 未运行真实 MM 对象重建、真实 SIM 存储满、双客户端、真实发送、换卡、ARM 或冷启动；SMS-02/04 不据此标 done。
- 未改发送器、幂等键、Unknown/提交恢复、权限、schema、Root 服务及更新器。
- CC 正在从固定旧主线制作应用 53；本修复不属于该候选包，未参与其签名、安装或设备测试，后续单独集成。
- D-05～D-26 保留；主线每完成新批次再更新本表和看板。不把 Markdown 勾选、Git 合并或测试数量换算为固件功能百分比。
