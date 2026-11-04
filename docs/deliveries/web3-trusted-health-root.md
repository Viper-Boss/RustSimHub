# WEB3-UPD 第二阶段：Root 组合准入与受限确认

日期：2026-11-04（Asia/Shanghai）。提交审核，不自行合并、签名、部署或验收；整体仍 81.5%、20/169、加权 10.6%。

## 来源与 PR 顺序

起始 main `8f9cfeff1a4b5a804b8b138e890f7386caf5a006`；固定 PR200 `a690213cec950b273632f595b96a0bea8f8e5e64`（包含197）和 PR198 `32d340f9701cb73d36fbdc443bfcf6ef7c08ff6a` 的正常合入关系见第一阶段交付。未修改它们的原分支、作者或超限反例。

第一笔为 [PR206](https://github.com/Viper-Boss/RustSimHub/pull/206)，产品受测源 `5988d416dd05a4590559d087b971f67acc8cefd2`、文档 HEAD `fe65bf3e1fd02375ed6fcd38a595271c33180caa`。本笔堆叠在该 HEAD 上，分支 [codex/web3-running-candidate-confirm-20261103](https://github.com/Viper-Boss/RustSimHub/tree/codex/web3-running-candidate-confirm-20261103)，只审 Root 组合/确认及直接相关边界测试；待主线按依赖顺序审核。

**本笔最终受测产品 SHA：`5a8fcf02b6972d852599bbf38e22e0aa8bf04a11`**，`git archive` SHA256：`9c7cabffcd9d15ac0bb65026b848e0df571c2b00f958445e3d0b6785a11a7a02`。五包完整测试源为 `330a3f29b7bb2e20a546fef5c59f32f7d146a5e6`（归档 `1d161b49ba2de9b19c47631055b643601ee46c94fbfbbb1544b4c068ae931134`）；其后唯一代码差异是将 systemd transport 函数移到内联测试模块之前以满足严格 lint，函数体和行为不变。最终5a8源重新运行Updater默认/全特性、严格五包Clippy、实际隔离确认及只读矩阵，分别记录，不混用来源。后续报告提交仅含文档，当前完整 PR HEAD 在 PR 正文回读记录。Git 作者保留 GTW，实际 GitHub 登录 wnfhsj88；未重写贡献或估造有效工时。

## 产品行为与可信字段

任务书十二个动作由两笔 PR 联合交付：第一笔提供真实 Core/Auth/Store/控制器来源与固定协议；本笔提供 Root 的活组合对象、兼容门禁、锁交接、原账本写入及有限触发。它不把旧报告的 false 改成授权。

| 调用点 | 可信来源与约束 |
|---|---|
| `RuntimeObservation::observe` | 复用197/200固定路径、已验签实际槽位、原账本/信任、Linux boot、systemd invocation/cgroup/UID/能力和实际 exe/start ticks；原五秒起点不刷新 |
| `RunningCandidate::observe` | 持有同一个观测、原已尝试候选/floor、经验签的回退组、精确 Root 模板和实际数据身份守卫；不能从 JSON/用户 hash/PID/path/time 建能力 |
| `SignedPrograms::running_inspect` | 复用原签名/载荷与 sealed memfd 的 `--build-info` 执行，子进程限250ms并回收；候选及回退都须支持当前明确 profile 和保留的健康前缀 |
| `Core::connect` | 固定私有 socket 安全目录/模式/inode；非阻塞连接；内核 UID986/peer PID 必须等于197捕获的实际已签名 Core；回复来源 SHA 必须匹配签名 manifest |
| Core snapshot | 当前同进程 Auth instance、原 actor 的真实 schema/realm/line/Owner generation、数据 dev/inode、现有 forwarding controller/queue/session；同挑战/连接和完整快照一次 Recheck 必须完全相同 |
| `running::Data` | 独立核对固定真实数据库、WAL、writer lock、restore-pending 和 identity 文件/目录元数据；不读取私钥/转发凭据正文，也不另开业务 writer |
| `running::compatible` | 沿用原 Root/data/回退兼容比较；只支持明确 schema6、format2、forwarding profile，不把7/9当6；缺来源、旧 Core、AP/provider/未知 override 或不兼容 outbox 均拒绝 |
| `RunningCandidate::confirm` | 消费能力；错误不可复用。只确认已尝试、当前实际运行、floor等于该候选epoch的精确组，保留签名/进程/Root/data/连接守卫 |

转发策略与实际进程分开比较：格式2含签名 worker 并不要求 ON；实际 OFF 可无 worker。ON 必须是同代次已准入 worker，Root 还核对进程/cgroup；unknown、孤儿和不一致拒绝，不为健康检查启动 worker。原 `inspect-runtime` 的授权/业务/data及 may_confirm 仍为 false；旧只读入口无新的恢复或写入。

## 锁交接、提交边界与崩溃

原 updater 的共享 flock 先于 Core Auth → forwarding management → 原 Storage actor 读事务取得。转换同一已打开锁文件为独占时可能短暂失去共享锁，因此独占成功后重查全部原 inode/内容/账本守卫；锁竞争或变动立即消费能力，不通过 `Store::open` 创建/恢复后再回头验证。

准备的确认 JSON 先写入预创建、Root0700、与 app 同设备的独立 `system/update-health/confirmation.next` 并 fsync。此时 app/state.next 尚不存在，原 Store 不会恢复这个准备文件。**最后一次 Core Recheck 在准备完成后**，继续持有原 Auth/controller/SQL lease，Root 再核对原槽位、签名、实际进程、Root/data、连接和单调截止。通过后才把授权意图移动到原 app/state.next，并执行原账本 rename/fsync 语义；floor不增加，candidate清空、attempted清除仅发生于确认提交。

这是有界提交决定，不承诺未来硬件持续可用。门禁前失败/进程退出保持原账本和attempted，残留 scratch 保留且后续拒绝，需显式维护检查；不自动清理、重试或轮换身份。授权意图发布后若崩溃/fsync/最终释放应答失败，结果可能已经提交；返回错误不能解释成“没有写入”，应查询原 inspect-state/维护恢复，不能盲重试或删意图。原恢复 namespace 的语义保持。

## 窗口4固定装配接口

完整接口与精确材料表见 [候选 profile 合同](../../firmware/system/update-health/README.md)。只在明确候选中装配；默认 overlay 与发行工具未修改。

- 首前缀 `--enable-update-health-candidate`；socket `/run/rustsimhub/update-health/core.sock`，Core986/0700目录、0600 socket、Root内核peer门禁。
- Root命令 `/usr/libexec/rustsimhub/rustsimhub-update confirm-running-candidate`，无参数。非Root/错误参数在可写 Store 或恢复前拒绝。
- 协议1，8192字节、整帧250ms、观察绝对5000ms，不续期。
- 明确候选 Core drop-in、tmpfiles、一次触发 timer/service 独立存放。timer显式启动一次、20秒触发、service预算6秒、Restart=no，无默认enable、循环或失败重试。
- Root触发只保留私有socket/读取跨UID进程与受限签名元数据子进程所需能力，`ProtectSystem=strict`、只允许写原app和scratch目录；Core能力仍为零。Core-owned0600 socket的连接需要Root触发单元明确包含DAC_OVERRIDE。
- 老回退 Core 不认识新前缀时本 profile 不支持；先走原明确维护路径获得兼容 baseline，不自动替旧程序加开关。

## 冻结检查

Rust/Cargo1.98.0；按原 Cargo.lock，无依赖版本升级（lock只增加本包直接本地/已存在依赖关系）。统一设置：`CARGO_BUILD_JOBS=2 CARGO_INCREMENTAL=0 CARGO_PROFILE_DEV_DEBUG=0 CARGO_PROFILE_TEST_DEBUG=0 CARGO_PROFILE_DEV_OPT_LEVEL=1 CARGO_PROFILE_TEST_OPT_LEVEL=1`，`RUSTSIMHUB_BUILD_SOURCE_SHA256` 为每次对应受测提交的 git archive 摘要。默认/全部特性指 feature 集，不把重叠测试相加。原始日志只在忽略的 `.local/`。

五包集合是 `-p rustsimhub-update -p rustsimhub-core -p rustsimhub-auth -p rustsimhub-storage -p rustsimhub-wire`。

| 实际命令/场景 | 完整源码来源 | 退出码与结果 |
|---|---|---|
| `cargo test` + 五包集合 + `--locked` | 330a3f29b7bb2e20a546fef5c59f32f7d146a5e6 | 0；449通过，0失败，9专项忽略；包含1项编译失败doc测试 |
| 同上 `--all-features` | 同330a完整SHA | 0；694通过，0失败，14专项忽略；与默认重叠，不相加 |
| `cargo test -p rustsimhub-update --locked` | 最终5a8完整SHA | 0；95通过，7专项忽略（含原崩溃/失败矩阵及1编译失败doc测试） |
| 同Updater `--all-features` | 最终5a8完整SHA | 0；96通过，7专项忽略；不相加 |
| `cargo clippy` + 五包集合 + `--all-targets --all-features --locked -- -D warnings` | 最终5a8完整SHA | 0；未加allow或降低警告等级 |
| `rustfmt --check --edition 2021 --config skip_children=true` 对本笔Rust文件；`python3 -m py_compile tools/test-update-health-native.py` | 最终5a8完整SHA | 0 |
| `python3 tools/test-update-health-native.py --tests target/debug/deps/rustsimhub_update-7b202bc7911ea3c3 --updater .local/web3-upd/last-updater` | 最终5a8完整SHA | 0；真实隔离确认、连续CLI查询、异常退出与15组反例通过，工作树clean |
| `python3 tools/test-update-runtime-observation.py --updater .local/web3-upd/last-updater --image rustsimhub-web3-python-runtime` | 最终5a8完整SHA | 0；31组/49次检查通过，保护树不变，may_confirm=false，工作树clean |
| 原 `tools/test-update-state-inspection.py --updater /fixture/updater`，私有Root/no-network/chroot容器内执行 | 最终5a8完整SHA | 0；14组通过，9个错误调度在可写Store前拒绝，原status恢复兼容保持 |

Python3.13隔离镜像由scratch与已安装的解释器/标准库/运行库构造，没有拉取镜像。状态矩阵使用Root私有网络、只读镜像、256MiB tmpfs、512MiB内存及有限PID；runtime矩阵原有31组/49次检查与所有保护树比较均保留。容器是专项隔离，不能当真实PID1或设备。实际CLI与lib测试程序摘要记录在下面；不把文档HEAD冒充编译产物来源。

- 当前lib测试程序 SHA256：`47d654a575c55f6b3f25ba9016e2efc0065a07d31944ebdc24838022bc2751f0`。
- 实际原生CLI SHA256：`4dcc5f021ed6a09db75a9066a728395aadd56d6b5b8e1e4b486dedede08b682e`。两份产物均声明最终5a8归档，隔离驱动回读完整HEAD及clean状态。

端到端使用临时签名组与真实固定Unix socket、实际UID986/985/984及PID/exe/start/NNP/零能力、真实Core健康生产者/Auth/Storage actor与原Store账本提交。成功恰好一次，实际CLI连续查询结果相同、floor仍2。失败覆盖业务不就绪、Root材料、realm/代次、撤权、schema、身份文件、签名/载荷、OFF/ON不一致、连接/实例重放、实际exec（进程仍活着时拒绝）、退出、真实过期和外来socket清理保护；均不推进原账本。

另有真实进程exit91发生在scratch fsync之后、最终授权门禁之前；原Store重新打开不提升该scratch，原attempted/floor保持。锁竞争、锁 inode 替换、回退签名/载荷、floor变化、部分scratch及不安全目录由真实文件/flock测试覆盖。原Store授权意图后的恢复由原崩溃/状态矩阵覆盖，未在新路径每一个fsync点强制注错；不宣称穷尽所有物理故障。

Native容器没有mock整个健康生产者和Store确认。明确合成部分是MM/NM业务就绪、PID1/unit/cgroup成员来源、签名build-info执行输出和仅cfg(test)存在的native ELF适配；生产路径始终校验ARM并执行sealed签名程序，native载荷在生产校验中仍拒绝。未完成真实ARM/PID1服务正例、强制内核PID复用、真实ON worker全链及设备验收，均not_run，不用native成功背书它们。

## 失败保留与边界

第一阶段5988的完整all-features Core恢复T5曾出现Storage失败，未接管窗口2或改其actor。早期访问器编译、隔离镜像模式/动态库、旧调试大夹具超过五秒，以及私有夹具重复chmod非本UID目录失败均保留；最后通过是修正准备/采用明确优化构建后的结果，没有放宽产品断言、权限或预算。复核后补DAC_OVERRIDE并加强实际exec/CLI查询测试，旧bb9检查因源变化明确中止143，不冒称冻结证据。330a首次严格检查因items_after_test_module失败；仅移动定义位置后最终5a8严格检查通过。旧状态矩阵自建镜像曾缺导入依赖、64MiB临时空间不足，补齐镜像/扩大有界tmpfs后重跑，原脚本和产品断言未修改。

ARM/PID1、正式签名/epoch、实际部署和最终设备准入留给负责人。未访问NAS/棒子、改宿主NSS/系统总线/防火墙或发起真实通信。回退可停用明确profile并正常revert本笔独立提交，保留账本、scratch与原恢复信息；不得通过降低floor或重置attempted回退。本包新增代码沿用AGPL-3.0-only，没有引入外部源码；OpenAI Codex辅助实现与检查。审核结论待负责人按PR完整HEAD填写，未自行批准或合并。

云端CI单独记录：PR206的Rust runs `41677771840`、`41677723966` 均无执行步骤，检查注释明确为账户付款/消费额度导致未启动；不是代码测试结果，不称CI通过，未改计费、保护规则或重跑绕过。第二笔PR的回读另在PR正文记录。
