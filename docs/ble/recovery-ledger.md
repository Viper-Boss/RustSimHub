# D-23局部交付：持久请求回执与单次领取

日期2026-10-29。代码`a9f5f59`，与已合入main的Claude提交`677ba57eaebaae9aede4968e32758a754b9703fa`同步的合并提交`b9a73d2`。独立分支`codex/recovery-ledger-20261029`基于PR61；按用户允许先离线实现、合并前审核执行。不操作设备/服务，不修改Claude工作区。

**这是D-23存储/Auth部分，不是完整Wi-Fi恢复通道。** A生产安全会话、浏览器恢复key、真实IPC、Core唯一Wi-Fi执行器接线/USB与IMS协调、Root保存证据等仍待；不扣D-23整步，lead20/D余6/总118、20/169和原81.5%保持。

## 实际源码

- [账本与单writer事务](../../crates/storage/src/recovery/ledger.rs)、[严格候选DDL](../../crates/storage/src/recovery-ledger.sql)、[迁移/启动恢复](../../crates/storage/src/lib.rs)。同一个Store/SQLite，不新增平行业务库。
- [Auth私有能力到回执的入口](../../crates/auth/src/recovery/authority.rs)：reserve/claim每次复核真实能力，只允许begin/confirm/cancel；request UUID、realm、grant/代次、动作、正文摘要必须来自同一能力。只读能力不能创建写请求。
- [Store真实测试](../../crates/storage/src/recovery/ledger/tests.rs)、[Auth接线测试](../../crates/auth/src/recovery/authority/tests.rs)。只有已有外部证明适配器在cfg(test)内合成，数据库、权限、回执与执行领取均为生产模块。

## 已实现语义

| 阶段 | 持久动作 | 防止重做的边界 |
|---|---|---|
| reserve | 按realm/grant/generation/request UUID保存prepared及动作/摘要/原trial | 相同输入返回Existing；同ID换动作/摘要/原trial拒绝，不返回新执行许可 |
| claim | 同一writer复核当前grant和完整prepared快照，事务变成accepted后才返回私有字段permit | 6线程真实竞争只有1次领取成功；无Clone/Deserialize/公开构造器；accepted不能重领 |
| finish | 消耗permit后记录ready/saved/rolled_back/rejected/unknown及有界trial | 数据库写失败仍保留accepted，不回到prepared；完成事实允许在撤权后写入，不能因此授权新动作 |
| reopen | 启动事务把prepared/accepted统一隔离成unknown | 不自动恢复执行，不清幂等墓碑；terminal和已有unknown保持 |
| lookup | 当前恢复grant只查询本realm/代次；近期Owner可核对其已撤权旧回执 | 备份恢复后旧realm和重新批准的旧代次拒绝；不另建第二Owner |

- 最多256条回执，包含unknown/终态墓碑，满时拒绝新ID；原ID仍可查询。没有静默回收或重置额度的维护入口。
- 仅三个固定变更动作。请求UUID规范且非零；trial为64个小写hex；摘要32字节。字段/状态组合严格校验，未知格式/缺表/损坏数据失败关闭。
- 不存Wi-Fi名称、口令、私钥或会话密文，只存摘要和最小关联信息。
- begin完成后可以补回实际trial，重复原begin仍返回同一回执；confirm/cancel不能换原trial。动作与结果组合不合法则拒绝。
- Auth绑定的是收到的准确正文摘要；**正文如何解析出SSID/口令/trial仍须由后续唯一Core控制器严格实现**，不能把另一个来源的trial字段混入。此层尚未接正常Wi-Fi执行器，不宣称已完成此语义接线。
- ready/saved/rolled_back是可信Core提交的事实结果，不是Root证据验证器。后续只有核验现有Root/事务回执后才能调用finish，不能让前端传saved=true代替真实保存。
- 本批防止领取同一请求第二次执行许可，不等于整个系统已有“恰好一次”网络副作用。丢确认仍需核对原trial/Root记录，unknown必须保留。

## 显式候选schema8与兼容限制

不暗改上一批schema7含义：保留其grant/审计DDL，新增独立schema8账本迁移。普通Store/Core仍schema6，只有显式`open_recovery`/`restore_recovery`进入候选8；6→8的新增恢复表/账本在同一事务，7→8只追加账本。迁移内验证失败整体回滚；真实7输入损坏时仍保持7且没有新表。未来9拒绝。

schema7迁移测试通过当前库删除账本并设回7构造与前一DDL一致的夹具，保留真实grant/Owner，再执行真实升级；**没有运行旧schema7二进制**。普通schema6读者实际拒绝新恢复格式，原迁移失败/schema拒绝测试继续运行。旧7读者按其固定源码会拒绝8，此项只为源码结论。

生产的现装/候选/回退三方兼容、材料准入和健康确认仍未解决；**不得因此升级生产库或启用恢复服务**。默认Core build-info依然`database_schema:6`、`development_fixtures:false`；本轮合入Claude后首次配网契约保持其3600秒，不修改为恢复授权的300秒上限。

## 验证证据

隔离Linux/Rust1.98，实际UID65534、无网络/设备、空capabilities，只读源码/缓存。冻结归档和原日志仅在忽略的私有目录。

```text
cargo test -p rustsimhub-storage -p rustsimhub-auth -p rustsimhub-core -p rustsimhub-policy --all-features --all-targets --locked --offline -j3
cargo clippy -p rustsimhub-storage -p rustsimhub-auth -p rustsimhub-core -p rustsimhub-policy --all-features --all-targets --locked --offline -j3 -- -D warnings
cargo fmt --all -- --check
cargo run -p rustsimhub-core --locked --offline -j3 -- --build-info
```

1. 账本初版313项通过，归档`3e7181c57af8b59894f1a7d431b9182c629d23ed47e519848843d665b078f1ba`、日志`fcf22bb0e03d8ef56c51c1fce0c3c7732a1655c6674509c5dd2dcd6893ce692a`。
2. 加Auth实际能力接线与finish失败后316项通过，归档`4e7f428da65b4929e5e93718bacd7494336725ab14ab327192d6c09d9f1a80d8`、日志`ca56590a5494efc857f145fbf3f1b042532b24c70402bd0c5021a07bca745127`。
3. 同步Claude main677ba57后**Auth62+Core176+Policy1+Store85=324项全部通过，0忽略**，严格Clippy、全仓fmt和默认Core构建信息通过。归档`7c1a4145bc6aa58a4a0a8bd981f8b75285639d07df123ccdc78dbe5eb5bcae50`、日志`663038435d5d94f17624308fb85254efb47cdf2dd06ed38a0b456ef7a0738720`。增量8项来自已合入的主线，不冒记为本批新测试。

本批新增14个入口（含1个只在父测试子进程中执行的辅助入口，不算14个独立业务场景）：Store12、Auth2。覆盖重复输入/改变摘要动作trial、6线程争抢、重开隔离、真实进程领取后exit37、256容量、7→8迁移及回滚、五种腐败、reserve/claim/finish数据库故障、撤权后完成事实/Owner核对、旧备份realm、私有能力替换与只读拒绝。真实子进程没有执行网络操作，也没有代替Wi-Fi/Root验收。

## 主线同步与剩余

主线新增43提交已合入本分支：workspace同时保留ble与mdns，文档冲突保留双方历史；未重写作者。通过team.py重新读取GitHub实际PR状态并按main677ba57重算blame，产品合计49592行，lead48428、yh1030为1164，其他三位仍0；97.7%/2.3%是代码统计，未合并的本批不预加，分配/自测不当验收。

下一步仍是D-22真实IPC/凭据与A会话，以及D-23把私有能力/本账本接既有Wi-Fi唯一锁/事务/Root回执；begin、confirm/save阶段撤权/断线/并发网页、USB原门禁、电话协调和WWAN/IMS/APN边界要完整跑真实模块矩阵。D-24/25/26继续保留；本批没有真实BlueZ/ARM/手机/设备听音或网络测试。

## 本批投影验证

最终任务书/归属/统计归档`a0b5f5515c6f6e841d8fde9df5a6e0ad31a820c14eb8539aa8cd8ac966efba2b`、日志`11e1292ea2ca78974515f33b7af1c9df892c6f115a5c85482d2793fe4e8e840a`：Linux进度Python24项0跳过与生成一致性通过；Windows Node个人任务/唯一归属/计算/转义及diff检查通过。此后只补本段验证记录，不重计Rust测试。

完整提交定位：本批代码`a9f5f592ab1a84a326bb8e78a9f08a4f64318c1a`，合入既有Claude main的提交`b9a73d21d5f92210f1de335a41fbf789106d5a3f`。公开快照固定在PR63的`579268da8f7ce57b0f62a40d5d748e9b9fa3ddf6`；后续只补上线证明，不把截图/推送当设备验收。
