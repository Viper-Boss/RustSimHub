# PR170 最新主线整合与 PR162 清理

2026-11-03（Asia/Shanghai），账号wnfhsj88。用户明确要求“不合并的删掉、要合并的整理”，本轮范围为前文讨论的PR162/170；仍不自行合并、签名或部署。

## 交付关系和源码

PR162原HEAD `9bc7be8099840f9e5e41e176ff5bc6ee1afc8eb2` 是PR170原HEAD `3f377e48d006e2cf99caeb5c5dd33be0c8d7630f` 的祖先。
PR170尚有main缺少的实际TURN/TLS、中继worker、正常Core媒体授权和D安全恢复整合代码，故集中保留170供审核，162不再重复合并。
已正常合入最新main `06964f3803cdf84fba424ae6be5d38aff9d2f542`，其中负责人已合并PR191/192/193；保留原作者、提交历史和新主线实现。

- 合并提交 `a3ab422def19126e494324fd7909bd62bedc8c3d` 解决根锁、BLE manifest、BLE模块入口和RTC四处文本冲突。
- 正式受测Rust源码 `da0927973f0aed0111d39aaa78195d30358b6088`：补齐D新增BindPeer对既有A主机的编译影响。A主机保留原先预绑定路径，对不属于其模式的BindPeer明确拒绝；D service继续在真实Device1 lease绑定后才能确认。
- 私有联调runner提交 `3665becef4e113a5ea3c8f5a3f97715e1cf8ca39` 只修正两个Python runner的严格计数：保留PR192新增五项共享RTC回归后，TURN应为28项、worker矩阵43项及2个独立角色入口。实际`--list`为28/45；没有放宽零失败、忽略数或角色证明断言。该提交与da092的Rust源码、manifest及lock逐字节一致。
- 162已按用户要求关闭，原始交付正文保留；确认170远端包含原HEAD后，以指定旧HEAD的lease保护删除162远端分支。无其他开放PR以该分支为base；未修改其他草稿。原提交仍可由170历史及固定SHA复核。

## 整合后的行为

BLE同时保留显式`service`和已合并`product-host-candidate`，合并重复依赖的特性集合，默认仍为空。
D服务固定Core路径为`/run/rustsimhub/core-recovery/recovery.sock`；既有A候选是`/run/rustsimhub/recovery/core.sock`。两者是独立候选契约，本次没有静默改路径、自动选择、启动两套服务或批准Root安装组合。

RTC的LAN发送和中继入队共享原权限及编码前后60ms检查；过期帧不写RTP，未来时间、缺权限和撤权仍拒绝。
LAN只在实际入队后排空传输，保留PR192行为；中继保持自身出口/原期限检查。原过期和未来帧测试在中继特性下也覆盖入队入口。
主线191的SDK/Root/状态/恢复listener与原D的Wi-Fi副作用边界撤权复核均保留，未把不同恢复竞争诊断宣布为已解决。

## 实际验证

所有Rust命令使用Rust1.98、`--locked --offline`；为适应本环境磁盘，统一设置`CARGO_INCREMENTAL=0 CARGO_PROFILE_DEV_DEBUG=0 CARGO_PROFILE_TEST_DEBUG=0`。没有改编译目标、断言或生产默认特性。

| 命令/范围 | 实际结果 |
| --- | --- |
| `cargo test --workspace --all-features --locked --offline` | 1527通过、0失败、11项专用入口保留suite忽略，64个suite；不把全部忽略项写成已执行 |
| 全仓all-targets/all-features严格Clippy | 通过 |
| Core默认no-default-features及不含dev-fixtures的product/email候选检查 | 通过 |
| `python3 crates/core/tests/run_recovery_candidate_uids.py` | 12场景、36次角色执行通过；实际D GATT/service→A密码→同Auth/Store与原Wi-Fi actor，外部NM/BlueZ/MM为合成 |
| `python3 crates/ble/tests/run_core_port_uids.py` | 12个真实UID/属主/文件权限/超时/取消/长度/版本等场景通过 |
| Native默认 / turn-relay | 分别39/0/0、47/0/0，不相加冒称独立测试数；两配置严格Clippy通过 |
| `python3 native/audio-alsa/tests/run_private_turn.py` | 实际私有TLS/Coturn/RTC 28项通过；含共享源回归，不称28项全新TURN场景 |
| `python3 native/audio-alsa/tests/run_private_remote_worker.py --build-fixture` | 43通过、0失败、2个角色入口另行实际执行；普通Core身份/CallLease→实际worker双向Opus和撤权通过，原Auth/Controller/Authority未mock |
| BLE有限材料生成器Python回归 | 19项通过，未实际安装Root材料 |
| 看板维护回归 | 23项Python、Node及生成一致性检查通过；178个ID、所有状态/依赖/验收/权重及审核记录不变 |
| 根/Native fmt、diff及来源比较 | 通过；原162和最新main均为整合HEAD祖先，Native lock仍为原`eded8a32e629f0b8e1fae7ab287390c1b2bb6823d7ef51c55067fe8297567a14` |

新鲜来源、每组日志/制品摘要、角色证明边界见[机器证据](../ble/member-d-pr170-main-sync-evidence.json)。完整原始日志留本机`work/pr162-170-review/`及各runner输出目录，未上传真实数据或私钥。
先前a3合并编译的BindPeer缺臂失败、默认调试信息构建ENOSPC和第一次镜像构建空间失败均保留；修正代码、转移可重建静态缓存至RAM并关闭调试信息后重跑通过，不以旧日志冒充新组合。

## 保留的验收边界

12个安全恢复场景的FD关闭和GATT注销已核验，但`clean_shutdown=false`与原170证据一致，完整字段保留；不冒称服务最终干净退出/正式停机预算通过。
Docker中的UID/NSS、CA、凭据、音频及网络均为隔离夹具，未修改主机账户/安装或访问真实设备，没有公开TURN/NKN、短信、拨号、蜂窝、真实PCM、ARM执行、签名、发行或部署。
`crypto_wire_frozen=false`、`production_admitted=false`保持；原DTLS依赖审查风险、Root角色/路径/预算/迁移/正式装配、真实无线与两端听音、长期运行仍须主线分别审核验收。

D原交付已声明26/26离线阶段提交完成，本次纠正看板仍沿用22/26的过时记录；提交不等于逐HEAD审核或全局动作完成。
原P0–P7仍81.5%，行动台账20/169、加权10.6%，分母/权重/验收/审核记录不变。云端CI以最终PR HEAD回读单列，不将本机通过当作云端通过。
来源/许可保持原AGPL及上游声明，OpenAI Codex辅助整合、执行和核对；未知实耗不编造。后续由Viper-Boss审核PR170，不自行合并或部署。
