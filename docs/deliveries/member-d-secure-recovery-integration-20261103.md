# D22/D23：真实安全会话候选、GATT 与原 Core Wi-Fi 闭环

2026-11-03。用户已批准 #53/#56/#132 离线实施，沿用本人公共入口 PR #170，不修改他人分支或自合并。**D22/D23 固定候选的离线实现与验证包本轮提交完成，累计26/26离线阶段、本人可独立离线剩余0；固定PROGRESS-SCALE仍80%。** 不是主线已审核、协议已冻结、生产启用或设备验收，不改全局台账/权重。

完整正式受测源码：**`7ec74d1bd82c5d0c71bb1168be5b21066b44b3e7`**，tracked_dirty=false。后续最终包装提交只更新交付/机器证据，最终推送完整HEAD以PR回读为准，不把旧运行冒充包装HEAD重新全量运行。[逐来源、角色和日志/产物hash](../ble/member-d-secure-recovery-integration-evidence.json)。

依赖不是自行编写的密码适配：正常合入未合并的 [A候选PR #178](https://github.com/Viper-Boss/RustSimHub/pull/178) `25bcaa4671278f25720b5c2fb9c2ca720a1375ef`，保留原作者/提交；再正常合入main `1e5cadd9c32c6576e77f10c9e65f0eab87076098`。A的Noise/Ed25519/AEAD、Verifier/Sealer及其普通NKN桥保持原实现，仅有一处上游测试rustfmt布局适配；没有接管算法/会话库或把候选批准为产品。`crypto_wire_frozen=false`、`production_admitted=false`保持。

实际组装为：非cfg(test)的D BLE service/固定CoreEndpoint/UnixSecurePort → 内核真实UID/PID、原SourceGuard和Core通道 → A实际密码会话/真实Auth、Store和恢复grant → 原Core应用分派与唯一Wi-Fi actor → 正常NM接口。Core测试进程组装这些实际库模块；没有声称安装/启动了生产Core daemon。蓝牙低权进程不读取设备身份/解密业务，不创建Owner/第二授权、租约或网络事务库。

生产固定路径沿用D已审材料 `/run/rustsimhub/core-recovery/recovery.sock`，不改为A另一个候选转接器的路径。夹具UID1001/1002/1003只存在容器NSS，分别运行Core、BLE及外部BlueZ/NM/MM/原生密码客户端；不是生产账户分配。保守20B attribute、原12B头、真实AcquireNotify SEQPACKET FD和WriteValue均经过实际D库，没有广播Value信号充当通知，也不把本地FD提交当无线/浏览器收件确认。

## 已复现并最小修正的三处问题

| 原始干净失败源码 | 实际触发与修正 |
|---|---|
| `2613a6a1bad3a5fb5915ad8258d21f8d2c3dfc71` | 客户端收到首个握手回复后立即继续，旧通知的发送后来源复核尚未结束，Write被Busy关闭、Core报告Closed。service保留现有回调并在其**原捕获期限内**排空旧通知，再接下一个输入；原Transport重叠请求仍拒绝，没有加队列或跳过复核。修正e7a55cb。 |
| `e7a55cb2f596630e2e098ea0a7d7aa30e4d782f8` | Driver仅在进入step时poll一次，后续I/O唤醒落在select中，碎片回复每个DBus往返都等10ms tick；原4秒查询测试过期。改为每次I/O唤醒poll原持有future，10ms只作到期兜底；没有延长期限/帧预算。修正5fb9ca5。 |
| `5fb9ca5d7f39466a898d3f2dd5496d55468d3612` | 实际Update2已保存、回执前Owner撤权，旧NM commit仍先销毁检查点，真实回滚调用为0，未返回Saved。现在在创建/激活/保存及销毁检查点前后通过**同一Auth/原trial**复核；迟到保存回执不能批准丢弃回滚柄，撤权后仍允许原补偿。实际保存1/回滚1/删除1且无成功保存回复通过。修正79b1796。 |

没有修改A密码原语、旧scope/USB权限、产品租约、I/O/握手/回调期限、帧/结果/控制队列上限。正常非恢复NmProvision保持原门禁；恢复后端仅多持同一个Auth引用，未持Auth锁跨外部I/O。没有电话续租/拨号、WWAN/IMS/普通APN写入。

## 正式验证

环境Rust1.98，所有Rust测试/构建locked/offline。A固定snow0.10.0最初不在本机缓存，先按原lock下载公开依赖，再离线构建；无新密码版本选择或自批。原Native锁SHA256仍`eded8a32e629f0b8e1fae7ab287390c1b2bb6823d7ef51c55067fe8297567a14`，原活跃闭包/registry版本不变；原OSV快照日期和DTLS风险不改写为新扫描。

- 全仓 `cargo test --workspace --all-features --locked --offline`：**1494通过/0失败/9忽略**，63个suite。相关normal Core角色和NKN两个角色另行实际执行；其余忽略保留各自父测试/runner边界，不全部冒称手动执行。
- 全仓all-targets/all-features严格Clippy、显式recovery_candidate_uid严格Clippy、默认Core no-default-features check、Root/Native fmt/diff：通过。既有proc-macro-error2 future compatibility提示保留。
- `python3 crates/core/tests/run_recovery_candidate_uids.py`：下表**12场景全部通过，36次角色libtest执行、3个角色函数**，不算36个独立新增测试。容器network none/read-only/256MiB/32pids/2CPU；演员cap0，Root编排仅SETUID/SETGID/CHOWN/FOWNER、无CAP_KILL。
- 复用A的 `run_recovery_kernel_uids.py --secure`：7次真实密码/原Core查询角色执行通过；`--nkn`：9次普通身份/成员/原Core执行通过，均绑定本轮完整源码，不挪用A旧HEAD证据。
- Native默认 **34/0/0**、turn-relay **42/0/0**；对应严格Clippy及两个manual目标严格Clippy通过。实际TLS/TURN/worker矩阵 **38/0/2**，两个专用角色入口另实际执行；原Core正常身份/CallLease→非测试Host→实际worker/双向Opus及撤权也通过。128MiB/16pids/2CPU，普通矩阵UID1000/cap0，跨UID演员984/985/986 cap0。

| 实际安全会话/GATT场景 | 真实结果 |
|---|---|
| 开始→查询Ready→确认 | 两条实际SQLite回执Saved；NM保存1、回滚0；不输出密码 |
| 取消 | 原actor回滚1/删除1；Begin和Cancel均RolledBack |
| Ready后断连 | 保持接收端FD存活，实际Device1断连触发service/原Core撤权、回滚1；两个FD关闭无迟到数据 |
| 确认前撤权 | 保存0、回滚1，旧会话不能确认 |
| 保存已发生、回执前撤权 | 保存1、回滚1、删除1；Begin/Confirm持久RolledBack，没有加密Saved回复 |
| 旧Begin密文重放 | 新T5消息ID使请求确实到达A防重放；仍只有1次NM创建，回滚后关闭 |
| 错pin、篡改 | 无NM创建/保存，恢复操作表无新行 |
| 网络锁、通话互斥锁已被占用 | 真实共享Core锁拒绝；Query为not_found，无持久预约/外部创建；不称真实网页或真实外呼 |
| 普通USB门禁 | 无效普通token拒绝；真实Owner普通配网在无USB环境回滚、无NM副作用；随后真正恢复grant可以走同一个Wi-Fi actor |
| 回滚验证失败 | 保留两条Unknown及null结果字段、实际查询仍Unknown；新Begin被拒绝，创建仍1、未知检查点/配置不冒称恢复成功 |

正例使用明确的外部交错：暂停**私有NM只读GetSettings**，等实际Core控制记录已claimed且Auth临界区结束，再释放该读。这让原控制排队/迟到继续执行得到确定验证，没有替换Auth、业务、Verifier或Sealer。早期未同步运行真实返回Busy并留下Begin RolledBack/Confirm Unknown；该拒绝和日志保留，不改产品Busy语义、不重试业务来洗成成功。原“Auth长时间忙也必须回滚”测试仍在全仓通过。SQLite只读观察是夹具结果核验，不制造权限。

其他夹具失败保留：Root编排最初不能穿越角色组目录、公共backend元数据对BLE不可读、误用了另一个候选socket路径；均修正测试材料，未放宽产品目录/UID校验。只读SQLite观察器最初错误借用非Sync连接使编译失败，改为任务独占移动，未加unsafe。缓存空间不足时对可再生成的本项目中间程序做可校验压缩/迁移，正式程序/原日志保留，没有清空全局缓存或用户资料。

## 未验收与后续主线工作

本包只完成固定安全候选的离线库接线/测试交付。A密码/依赖/内存擦除审查与协议冻结、Core正式监听启动、Root实际角色/NSS/cgroups/迁移/安装、签名/发行组装、真实BlueZ/HCI/RF/浏览器、Wi-Fi/WWAN/PCM/设备/外网TURN及两端听音、生产预算和长测均未据此完成。新安全闭环没有声称ARM运行；旧ARM媒体证据仍按其原来源和已记录失败/预算边界保留。原DTLS `crypto-bigint0.5.5` / RUSTSEC-2026-0387未收口，production_admitted=false。

保留AGPL/上游来源/作者及merge历史，Codex辅助，未知实耗不编造；未改C更新器/NAS、B UI控制器、A算法或其他成员分支。main/global台账及workflow仅正常导入主线原提交，本人无台账/workflow增量，不重写贡献或自合并。最终CI按推送HEAD回读到PR；此前账户计费/额度失败不能当本轮CI成功。
