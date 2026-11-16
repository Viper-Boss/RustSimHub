# A1：固定用途本地订阅签名与Core原授权候选

已有Go跨实现向量此前不能被生产Rust组件直接使用。本包新增真实Rust订阅编码、签名和链摘要，以及Core当前Owner/realm/参与代次/有限预算门禁。
它是A1.1～1.4/1.8的本地组成部分，不是完整A1、统计上报或A包93项完成；准确接口/剩余项见[调用边界](../offline-a/statistics-local-signing-v1.md)。
负责人已允许继续离线候选，不代表批准生产Root新能力、空attributes策略或长期授权。

## 固定来源与文件

- yh1030本人，同仓库`codex/offline-a-statistics-signer-20261116` → main，不自合并。
- 起始main `6e1cd19bc9dd05fd150157c422590c1a801cc0b0`，含产品基线`2ae840fbffff04691ccf8cedd49d554093dd4032`。独立于未合并的PR263/264，没有夹带网页或短信审计。
- 实现HEAD `b4e9650248ac8ac1748c524df9b94a48c9fda979`；最终受测tree `fbc9e6d449afe997ced7bcaf4b3642737c36a8c8`；完整Git归档SHA256 `5897e03a13220c9c501460f0b6586ad47d39c673168cca2cd270a0794a9d594b`。
- 实现后只补本报告与调用边界，产品/测试字节保持；当前完整HEAD在PR正文，避免自引用。
- 9个实现/测试文件：Core/usage-stats两份Cargo.toml、两份lib.rs、usage identity最小内部签名方法、`usage-stats/src/transaction.rs`及其测试、`core/src/nkn_statistics.rs`及其测试。
- 不改依赖版本/Cargo.lock、原CheckInTransport/状态格式、wire/Root/SDK操作、firmware/B工具、coordination、主线进度或他人代码。没有设备/NAS、公网、真实通信/生产密钥、签名发行或安装。

## 关键行为

原统计独立种子只在原FileStore/StatisticsIdentity内部使用。编码仅接受原严格意图和固定主题/duration/meta/fee/attributes；unsigned链字节与protobuf发送字节分开，Ed25519签名单SHA、链摘要双SHA。
没有公开的任意消息签名/种子导出API，也没有网络提交或SDK便捷Subscribe调用。
Core使用真实Auth/持久realm、原Owner有限证明及独立参与Ledger；OFF、错误Owner类型/代次、realm变动、许可重存/撤销、取消/截止、错误统计公钥或损坏存储均不返回签名交易。
Auth和Ledger原锁约束操作，结束再检查；不创建/修复身份、不产生隐含永久授权。
Core尚不核Root注册/nonce来源，不能越过后续适配；`reporting_available:false`、未决链确认门禁和300秒权限保持。

## 最终实际验证

WSL2 Ubuntu24.04、普通UID1000，独立Linux原生source/target/TMPDIR，Rust/Cargo1.98.0、原锁/offline。
全部测试目标检查实际非零用例，doc/禁用feature的0用例不计通过数。

| 实际命令 | 退出码与结果 |
|---|---|
| `cargo test -p rustsimhub-usage-stats --locked --offline -- --nocapture` | 0；90通过/0失败/0忽略 |
| `cargo test -p rustsimhub-usage-stats --features transaction-candidate --locked --offline -- --nocapture` | 0；92通过/0失败/0忽略，含原90和两个新增测试，不累加成182独立用例 |
| `cargo test -p rustsimhub-core --lib --features usage-transaction-candidate --locked --offline nkn_statistics:: -- --nocapture` | 0；5通过/0失败/0忽略，内部循环覆盖许可/身份/realm/代次/预算矩阵，不扩报用例数 |
| `cargo clippy -p rustsimhub-usage-stats --all-targets --all-features --locked --offline -- -D warnings` | 0 |
| `cargo clippy -p rustsimhub-core --all-targets --features usage-transaction-candidate,email-production-candidate --locked --offline -- -D warnings` | 0；包含完整原生产候选依赖，只编译既有测试，不运行热点专项 |
| `cargo check -p rustsimhub-core --no-default-features --all-targets --locked --offline` | 0；默认关闭组合可编译，不称为新模块已执行 |
| `cargo fmt --all -- --check`、`git diff --cached --check` | 0 |

新增Rust向量测试实际比较三个窗口×nonce0/1/128/2^32/u64::MAX共15组：canonical meta、unsigned bytes、签名hash/签名、protobuf交易、链摘要逐字节等于固定SDK向量；另用标准Ed25519严格验证并拒绝改过的hash。
禁用策略不读时钟；错误身份、陈旧周期、时钟错误均无准备结果。没有把意图JSON hash当链摘要。

Core测试使用实际临时SQLite/Auth/参与Ledger/FileStore及原CheckInMachine：
未授权/OFF/Client/NAS不创建身份；准备真实摘要落盘后测试端口才得到单次submit；丢回执保留原Pending；重启先保留原300秒Due冷却，再用**测试逻辑时钟**推进到prepared_at+300，只查询原摘要，prepare/submit计数0。
没有等待真实300秒、改主机时间或延长Owner权限。OFF后ON旧快照仍拒绝、持久realm/Owner generation变化拒绝；私有SQLite故障注入不等于实际整机恢复。
submit/query为明确本机端口，不把它称为真实Root/SDK或链上证明；没有第二套签到库。

### 固定上游与无外网验证

按原`tools/nkn-build/usage-vectors/run.py`，Go精确1.27.3 linux/amd64，SDK/NCP固定31文件及原go.mod/go.sum校验保持；没有换版本或改上游。
普通UID1000在临时无外网network namespace中，原版本/gofmt/module verify/build/vet全部0；15向量与向量文件逐字节一致、30次新对象签名/主题篡改拒绝、四个原错误编码反例各按行为退出1、HTTP guard计数0。
本次Go来源tree为`3271e18db71a8b164065eb42694ebf0c84d6cc26`；其全部SDK/向量输入与最终tree逐字节一致，已核Git对象差异，不冒称重新运行了不变输入。

最终Rust两个transaction测试另在临时network namespace以UID1000、空有效能力、NoNewPrivileges执行通过；记录实际namespace、UID/GID及仅lo设备。
纯编码函数本身无I/O/网络接口，Core门禁只读本地授权/存储。没有系统调用跟踪，不能把此结果扩大成未来Root/RPC提交适配零网络的证明。

### 受控故障反例

这两项是**本候选的受控mutation，不是旧生产实现**。旧版本不存在该签名接口，不能把缺接口编译失败冒充行为负对照。
最终所有测试字节不变，私有索引各仅改一个生产文件：

- `310b42831c5d4dc6d2dc1aaa4cdf0d659398dc36`：摘要改为单SHA，向量测试实际行为失败101，`SDK mismatch digest_hex, nonce=0`。
- `49eaca5b926204c6412a8ce563c6ed70740ad681`：只比许可enabled而忽略完整快照/代次，Core OFF/ON旧Signer测试实际失败101。原授权/其他断言保持，不是编译失败。

## 原失败与边界保留

1. 初稿Core借用MutexGuard未明确解引用，且新测试误调Storage私有call，编译101。修复借用并只用测试独立SQLite连接注入当前fixture的realm/代次，**没有开放Storage接口**。
2. 后续四项Core通过，重启用例却立即期待Pending；原库真实返回Due，保留失败。补充冷却断言并推进测试逻辑时钟后原ID查询通过，未改产品重试间隔或删除断言。
3. 最小root-provider feature的全target检查暴露原nkn_uid_tests缺迁移API。候选改为依赖已有完整product-providers-candidate，原受保护模块/迁移API不改；默认与真实候选组合检查通过。
4. 早期缺少strace，只记录未具备系统调用跟踪；没有安装替代工具、略去预算或声称跑过跟踪。

本轮私有日志/源码摘要/namespace边界及SDK实际输出归档66项、60,627字节，ZIP CRC核验通过，SHA256
`bace56ab0035654fdd9a56c4e2a117fca3879199753edc3175b3ce7cac4a79de`。
只含公开合成身份/测试输出，不含真实钥匙、通信或硬件日志，原始归档保留本任务忽略目录，不上传二进制。
固定SDK/NKN core及原probe来源/许可证保持；新Rust代码/文档AGPL-3.0-only，AI辅助实现/测试。

仍未完成：生产身份初始化/恢复、有限Root注册/nonce预检/SDK三操作/真实TLS-RPC适配、实际网络submit/query、链确认信任/深度、长期授权、共享SDK真实引用释放、ARM/B发行/设备验收。
给B仅交精确源码/feature和上述命令，不更新工作流/Rootfs/签名或build格式；缺能力保持不可用，不自行部署或合并。
下一步继续这些A1接线及剩余27项NKN/统计/远程逐ID审计；热点专项延期。A包尚未全部完成，整体已验收 **81.5%** 保持。
