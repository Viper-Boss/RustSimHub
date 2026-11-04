# WEB4-COMPAT：执行模式与数据兼容交付

日期：2026-11-04（Asia/Shanghai）。窗口7；AI辅助实现，保留已有迁移作者与 AGPL-3.0-only 来源。任务书为 `codex/web-product-delegation-20261103` 的 WEB4-COMPAT 与本批总书；不修改全局台账、169项分母或其他窗口入口。

## 阶段一：模式元数据与显式三方比较

起始 main：`8f9cfeff1a4b5a804b8b138e890f7386caf5a006`。正常快进消费 PR205 `c3a67120cb91f8c6095579a89764466a101d9433`，随后正常合入 main `a2044edfef6aeeced3d2c2eb8743791b34e4ccdc`，没有合入整份任务分配分支。元数据代码提交 `a4717f2c932bdf8010810859e4a42cd275fdf83f`；固定受测完整代码 HEAD：`e9de4c2cf95e7edb6d352c8f037406933030e63d`，其 `git archive HEAD` SHA-256 为 `895a2b5fe8b48a7e7b0ac106ff79ea03858b54101bec0152a23ed38f60d2bbee`。报告提交只补证据，最终 PR HEAD 在 PR 描述回读记录。

### 已有行为与有限转换

| 模式/历史 | 实际业务库 | 固定启动前缀 | 维护与回退约束 |
|---|---:|---|---|
| legacy-v1 | 6 | 无 | 原默认行为；不能读取7/9 |
| product-v1 / product-email-v1 | 6 | `--enable-product-providers-candidate` | 由实际 email 编译特性决定唯一产品模式 |
| 历史 recovery-grants | 7 | 不新增常驻运行模式 | 原 Store 6→7，只能作为显式维护中间态 |
| product-recovery-v1 / product-email-recovery-v1 | 9 | 上述前缀＋`--enable-secure-recovery-candidate` | 原 Store 6→7→9 或7→9；候选写9后，回退必须实际支持9 |

原 worker4 读写 outbox3；smtp 候选 worker5 读写 outbox4，原维护子进程负责3→4。候选可能写入的队列版本参与回退比较；旧 worker 不因新请求自报数字而获得新队列能力。原 format1、默认 Core `--build-info` schema6 和无 profile 路径保持。

新增 `crates/core/src/runtime_compatibility.rs` 纯函数读取真实编译特性、真实 source stamp 与 forwarder 合同。固定模式在打开数据库之前判断；只读元数据无数据库/服务/自动迁移副作用。新增 launcher `runtime_compatibility/` 严格解析显式 format2，检查 mode/前缀/schema/编译特性、源码、fixture、worker协议、未知/重复字段与预算。当前、候选及候选写后回退分别评估，保留退出码0/3/4。离线报告始终 `authorization_verified=false`，`mode_metadata_verified=false`；签名及同字节探测由下一阶段适配器提供。

`tools/product-upgrade/harness/` 是独立 `[workspace]` 验证清单，直接编译同一份拟接入模块；没有复制测试实现。其 runtime-probe 是同源原生验证入口，不冒充已接通的正式 Core CLI。

### 固定代码 HEAD 实测

在上述 HEAD 的独立归档上，Rust1.98，`CARGO_INCREMENTAL=0 CARGO_PROFILE_DEV_DEBUG=0`，将上述归档摘要设为 `RUSTSIMHUB_BUILD_SOURCE_SHA256`；全部使用 `--locked --offline`：

- `cargo test -p rustsimhub-core-launcher`：40通过，0失败/忽略。
- `cargo clippy -p rustsimhub-core-launcher --all-targets -- -D warnings`：退出0。
- `cargo test --manifest-path tools/product-upgrade/harness/Cargo.toml --features FEATURES`：default、product-providers-candidate、product-providers-candidate,email-production-candidate、dev-fixtures 四种分别1通过；fixture模式明确拒绝元数据。
- 同四种 feature 的 harness `cargo clippy --all-targets -- -D warnings`：全部退出0。

原始日志保存在私有 `.local/logs/stage1-fixed-*.log`。未运行 ARM 真程序/设备；本阶段未编译全 Core，未验证已安装 Root、签名或健康自动确认；CI 须以 GitHub 实际回读为准。

### 窗口8最小接续

待窗口3交付后，Core lib 导出纯模块，main 在任何业务/激活解析之前添加唯一有限入口 `--product-runtime-compatibility MODE`，参数必须恰好两项，调用纯函数，输出 JSON；错误有限退出4。该入口不接受路径、schema或数据库输入。用真实签名 ARM Core/worker 的同字节执行输出补端到端证明；不能将 harness 输出文件、metadata JSON 或安装报告当授权。

本阶段是任务动作1～3的模块交付；动作4～12的签名绑定、原守卫与原迁移协调在下一阶段继续。整体正式验收仍81.5%，20/169，加权10.6%；未部署、未签名生产制品、未自行合并 main。

## 阶段二：签名模式、原守卫与原迁移协调

阶段一 PR [#210](https://github.com/Viper-Boss/RustSimHub/pull/210)，完整 HEAD `19f9844fd76ff1597b69a694b2e86dff3561989b`。阶段二固定代码 HEAD **`59f9349ad5d184f89439605888456234753938f2`**，归档 SHA-256 **`da8f002c2d214175be17790063c6589bf09fccb839f91d2cfb70969a76290c6f`**；下述最终检查、编译与矩阵全部绑定此代码提交及此 source stamp。随后只追加本交付报告，不修改受测源码；最终 PR 完整 HEAD 见 PR 描述及回读。独立分支保持原作者/依赖历史，不改共享入口或窗口6安装器。

### 本次新增，原实现复用

`crates/update/src/product_admission/` 新增不可反序列化的 `SignedProductPrograms`，先走原 `SignedPrograms::inspect` 验签与载荷核验，再执行同一密封字节的固定 mode/worker 探测。源码、签名、每个载荷摘要、编译模式/feature/前缀/worker 和签名 manifest schema 边界一致才构造；manifest只准6时不能靠元数据9扩大权限。有界16KiB/3秒/子进程回收；生产探测降UID986。可信钥匙选择、实际槽位、floor和有效systemd由原维护动作另验，报告明确未验证，不信包内/浏览器提交的信任或stdout。

`ProductDataAdmission` 借用原 `MaintenanceSnapshot`，以原对象产生的实际 application_id/user_version/文件及Root代次事实构造 format2 比较，前后均重验；不会为比较再开业务writer。实际容器测试中，原业务writer锁竞争阻止观察，持有准入/原snapshot持续阻止writer，Root文件变代使准入重验及报告失败，直到原守卫释放才可取锁。这个测试的签名和元数据传输为明确标记的合成替代，实际Root快照/DB/flock是真实实现；没有冒称真实签名ARM程序已贯通。

有限 coordinator 的邮件函数调用原 updater `admission::migrate_email4`，沿用旧签名/备份/固定子进程/R-G-D握手与队列状态语义，不重写发送器或数据库迁移。邮件先于恢复：原邮件适配器仍要求base6，消费原snapshot后调用者不能假称仍持有它。

恢复 `prepare_recovery9` 按值取得原snapshot，计划对象自己连续持有同一守卫；`migrate_recovery9` 仅接收这个不可序列化计划，执行同一受验候选Core字节及固定child选项。Root在G前重验受限handoff。新同源 `tools/product-upgrade/recovery_child.rs` 实际以UID/GID986、空附组/能力、NNP核对直接Root父socket、source/key+realm绑定、期限与writer锁后，调用原 `Store::migrate_recovery_grants` 和 `Store::migrate_recovery_execution`，保留原备份及授权撤销语义；没有新DB引擎或Root直接写Store。公开JSON不是许可，固定路径与私有Root父握手不可省略。

Root系统日志按 Email4/Recovery9 固定分目录、锁和原子fsync提交序列化，支持两项有限迁移而不删除成功记录。业务子进程另保留private journal及原备份。中断查询区分not_started、committed、unknown和明确的grants_committed；提交后、未知和已存在日志不自动重试，不恢复覆盖新业务数据，不复活短信/出站任务。旧Store与worker无法读取新版本时明确拒绝。

完整可消费 API、最小共享入口/依赖清单、固定命令与输入准备见 [tools/product-upgrade/README.md](../../tools/product-upgrade/README.md)。新增 `run-matrices.py` 固定本地daemon/已存在镜像与只读合成输入；镜像 `python@sha256:2d44738ebf4ce5f63b9d9ea871e9c7cfdc0426f7b0c97cac915fd88ded1d86bb`，容器无网/NNP/临时固定路径，子进程核验空能力。原邮件9场景driver逐字复用，未修改。

### 固定59f代码 HEAD最终结果

工具链Rust1.98；`CARGO_INCREMENTAL=0 CARGO_PROFILE_DEV_DEBUG=0`，`RUSTSIMHUB_BUILD_SOURCE_SHA256`为上述真实归档摘要；Cargo命令均 `--locked --offline`，独占外部target。源码冻结后执行前后 `git diff --exit-code` 都退出0。四种FEATURES为default空串、product-providers-candidate、product-providers-candidate,email-production-candidate、dev-fixtures。

| 实际命令 | 结果 |
|---|---|
| `cargo check --manifest-path tools/product-upgrade/harness/Cargo.toml --lib --features FEATURES` | 四组全部退出0；正常非cfg(test)生产模块编译 |
| 同harness `cargo test --features FEATURES` | 四组各8通过，0失败；各1个需要专用Root容器的显式ignored，随后在容器单独实跑通过 |
| 同harness `cargo clippy --all-targets --features FEATURES -- -D warnings` | 四组全部退出0 |
| `cargo test -p rustsimhub-core-launcher` | 40通过，0失败/忽略；format1及显式format2 |
| `cargo test -p rustsimhub-update --features email-production-candidate` | 59库＋4事务＋7权限，共70通过；4个原有专用ignored如实保留，未记作通过 |
| `cargo test -p rustsimhub-storage --features recovery-grants,recovery-execution recovery` | 78通过，0失败/忽略；51个其他测试被明确过滤 |
| `cargo test -p rustsimhub-auth --features recovery-grants,recovery-execution recovery` | 73通过，0失败/忽略；45个其他测试被明确过滤 |
| 同harness build `--features dev-fixtures --bin store6-fixture --bin recovery-child`，lib test `--no-run --message-format=json` | 退出0；实际同源child与原守卫专用test可执行文件 |
| 原forwarder build `--example outbox3_fixture --features dev-fixtures`；原forwarder build `--features smtp-production-candidate` | 各退出0，先复制旧程序再改特性，未混用旧/新worker |
| `python3 tools/product-upgrade/run-matrices.py --inputs OWNED_SYNTHETIC_INPUT_DIRECTORY` | 退出0：恢复16/16＋实际原守卫1/1＋原邮件9/9 |
| 两个新Python工具 `python3 -m py_compile`、`git diff --check` | 退出0 |

Root恢复16场景：真实6→7→9、sealed memfd执行、包含已撤销恢复授权的7→9，及拒绝G、Root EOF、错source/binding、许可之后实际改key/realm、业务忙锁、非Root peer、过期/迟到G、未知字段、重复键、符号链接。每个场景回收child并比较全部旧受保护表摘要；拒绝场景原库逐字保持。成功后原Store6拒绝9，备份真实存在，重复调用不改变数据。普通同源测试另覆盖prepared、commit前后日志边界，明确Unknown并拒绝自动重提；原恢复授权对象迁移前后都拒绝使用已撤销grant。旧邮件原9场景真实outbox3→4，保留旧meta/requests/jobs及Unknown、不重发，旧worker拒绝新队列。

原始证据仅 `.local/logs/final-*`；合成制品/输入只保留私有临时目录，没有提交二进制、原始DB、seed或短信内容。初次环境失败如实保留：早期测试只读连接未设immutable而产生WAL旁文件，改为正确只读夹具后通过；容器初配driver权限、PID1和旧driver的`/owned`挂载冲突分别在initial/second日志保留，未改动迁移断言规避失败。最终固定HEAD全部矩阵通过。原updater4个ignored为两个专用Root capability测试、tmpfs ENOSPC测试及专用子进程入口，本轮不单列为新增通过。

### 十二动作与剩余责任

| 任务书动作 | 本包交付与验证 | 仍由窗口8执行 |
|---|---|---|
| 1～3 | 有限schema转换表、真实编译纯元数据、严格显式format2三方比较，40launcher回归 | 正式Core固定CLI、真实签名ARM输出正例 |
| 4～5 | 原验签同字节探测适配器、借原守卫准入、真实原guard/flock/Root变代反例 | 安装信任、槽位、有效单元与正式已安装程序绑定 |
| 6～8 | 原邮件适配器接入；原Store恢复child；有限持久协调、中断/回退拒绝；真实撤权保持 | 原snapshot受限提取DataFence/Root代次与business writer的持锁交接 |
| 9～11 | 真实临时DB及原迁移程序、默认不迁移/中断/保护行/错key/realm/锁/Root变化反例，四feature与原矩阵 | 生产协调器完整同链与ARM/设备验收 |
| 12 | 阶段一PR210与本阶段PR、固定完整HEAD、API/入口接续、测试与拒绝边界 | 负责人审核、主线合并及最终集成 |

**本包独立模块和隔离验证已提交，整体完成标准仍有窗口8接线前置，不能声称生产schema9已准入。** 现有原snapshot没有安全的公共writer extraction，Window8需要实现保留实际停机/Root/slot/outbox围栏的 `RecoveryMigrationHandoff`。安装写Root后旧完整快照必然失效，需保留DataFence并仅对已证实签名Root写集重采样；禁止丢旧锁重新observe或忽略无关Root变化。共享Core main/lib、updater main/lib/admission、workspace清单在本包均未改；正式CLI因此仍未接入。生产协调器与真正签名ARM/Core/worker同链未运行，ARM/QEMU条件不足记未运行。未触设备/NAS、真实key/宿主NSS服务或生产private；未安装、外发、改floor/attempted/预算或自行合并。

PR210云端CI实际回读run `41769377208` 为failure，两个job `steps=[]`、runner_id0；check注释明确“job was not started because recent account payments have failed or your spending limit needs to be increased”。这是账号计费阻止启动，不能称云端测试通过，也不冒称代码测试已运行失败。本阶段CI在PR创建后另外回读。整体正式验收81.5%、20/169、加权10.6%保持。
