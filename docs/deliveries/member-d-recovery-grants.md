# D-21独立恢复授权离线实现交付

日期：2026-10-29，成员D接续工作。用户要求继续推进；前置PR #80已实际合并，
合并提交`4e42d9883132abba22f0f1c8d4f8c35a477e4511`，未伪称GitHub存在额外APPROVED review。此前D-01～20四条PR仍未合并。

- 仓库/分支：Viper-Boss/RustSimHub / `codex/gtw-recovery-grants-20261029`，目标main。
- 起始main：`a52edb358c1c30c9b9cd0340bf241f5b6b77ce78`。
- 实现完整HEAD：`f271d0d901b57d7d8d8f03d5b7d54272ff97370f`。
- PR当前完整40位HEAD见PR正文；后续报告提交不改变上述实现/受测源码。
- [实现和生产启用边界](../ble/recovery-grants-implementation.md)。

## 实际实现

1. Store新增独立恢复grant/回执，固定wifi.recovery、16密钥/256回执，原十位scope不变；
   同一writer原子CAS/事务与准确防重，容量/代次均保留撤销空间，墓碑不重用代次。
2. 显式feature和显式迁移/打开/恢复入口：先持锁完成Backup API，再schema6→7事务；
   普通生产open/restore即使全feature仍保持最高6，拒绝7、不自动升级。检查固定DDL、外键、
   损坏/更高版本、异常回执/密钥。恢复保留旧证据而轮换realm、停用身份，不自动复活授权。
3. Auth全部管理方法要求近期Owner，执行前Store再比较完整当前Owner；独立公钥不会成为
   普通Owner/Client/Nas，也不能用已有普通身份密钥代替。复用既有Ed25519公钥检查。
4. 同一Core/Auth的四条管理派发与严格wire解析，generation以canonical十进制字符串传递。
   默认/普通schema6拒绝，显式合成schema7环境的真实Core IPC批准/查询/撤销通过。
5. 浏览器独立非导出CryptoKey/IndexedDB绑定、pin/realm命名空间、revision CAS和授权引用，
   不覆盖并发生成密钥、不以损坏存储或缺API静默降级，不复制密码Owner私钥。

没有安全握手/可信BLE上下文、Wi-Fi恢复控制、UID/Root服务、媒体/TURN或生产启动/部署接线。
浏览器模块尚未挂接生产UI/HTTP管理适配器；存储的grant元数据不是Core授权证明。
UPD-01 schema7当前/候选/回退契约仍需主线与成员C独立审查，本批不改其更新器/发行格式。
只有后续明确兼容的生产启动入口才能打开已迁移库，不能安装当前默认Core并声称支持7。

## 验证

环境：Linux x86_64、普通UID1000、Rust/Cargo1.98.0、Node24、真实Chromium。
Rust使用锁定本地缓存及`--locked --offline`；新浏览器夹具仅允许精确本地回环来源，
实际跟踪并断言无外源请求。全部使用临时合成状态/密钥，不访问设备/生产库/D-Bus。

通过的命令（均退出0）：

```sh
source /workspace/toolchains/activate-rustsimhub.sh
cargo test --workspace --all-features --locked --offline
cargo clippy --workspace --all-targets --all-features --locked --offline -- -D warnings
cargo test -p rustsimhub-auth -p rustsimhub-storage -p rustsimhub-wire --no-default-features --locked --offline
cargo test -p rustsimhub-core default_schema6_core_rejects_grant --no-default-features --locked --offline
cargo test -p rustsimhub-storage recovery_grants::tests::restore_rotates_realm --features recovery-grants --locked --offline
cargo clippy -p rustsimhub-storage --all-targets --features recovery-grants --locked --offline -- -D warnings
python3 tools/fixtures/recovery-authorizer/browser.py
cargo fmt --all -- --check
git diff --check
```

完整workspace：**733通过、0失败、4项原有忽略**。其中Auth49、Store55、Core168、wire20。
本批新增Rust入口20个（Store12/Auth5/Core2/wire1），不是20项实机验收。
默认三包44+43+20=107通过，默认Core拒绝路径1通过；这些是不同配置，不能重复加成新功能数量。
整仓通过后只补强已有restore测试，新增“默认恢复7拒绝/源备份不变/旧prepared变Unknown”断言，
该精确恢复测试及Store严格Clippy再次通过；生产源码不变，没有把未复验变更算通过。
最终浏览器夹具实际复验13个场景，覆盖并发创建、未批准、pin/realm、错误公钥、大u64、非导出、
真实签名核对、CAS、刷新、撤销引用、忘记、损坏/不支持存储；不称为产品握手互操作。

四个忽略项如实保留：compat的有界子进程worker入口（由父测试调用）、forwarder私有tmpfs真实满盘、
updater的两个隔离Root/capability测试。没有为本批移除或新增忽略来使检查通过。
第三方proc-macro-error2的既有future-incompatibility提示保留，当前严格Clippy退出0。

故障证据包含：实际DDL两阶段碰撞回滚、回执写入触发器失败后grant/成功回执均无新增、
旧Owner排队期间换代拒绝、真实并发CAS仅一方成功、回执容量最后一次仍可撤销、代次耗尽、
六种旧schema历史保留、坏表/格式/回执/高版本拒绝、真实库重开和Backup/restore。
未逐点做真实进程SIGKILL迁移、ARM/Native独立构建或设备/BlueZ/真实Wi-Fi/听音验收。

开发中失败保留在`work/recovery-grants/`：初次编译发现rusqlite不支持u64/usize直接取值，
改为边界受检的SQLite整数映射；wire测试的JSON数组宏语法错误和一个Clippy冗余匹配随后修正。
没有删断言或放宽权限来通过，之后完整workspace/严格Clippy成功。日志不提交生成物。

主要本机日志SHA256，便于核对（不是独立审核或远程CI证明）：

```text
workspace-tests-first.log ef713789a588436d4808966fe99e1b8d27376d19a032ce5e934dda2d0ab41bee
clippy-final.log          a44e4f5a5d8b140187c4e588ab6885c2cb492edf12cf21e538ea82e174ce6c4b
browser-final.log         e0488a185c4061fdb586d97a8eeafd7e1f9b4c53fdad819c28f450d2b20f3fe0
default-tests.log         65905f7c9d0d282fd5f48e4931c1948f2e054c227ab501c5de222ed12033d5cf
default-core.log          7f04589785cc37d1338604b011520b0fe68b7a55c9020b5e84f37836575649a6
restore-final.log         208f436d980a5f7d4816853aead2c85c0baf1bb9a68f99385291e587c3691c95
```

## 数量、后续与集成

累计按**离线包已提交**口径20→21/26；本批新增D-21一包，余D-22～26共5步。
这不是21步已合并或BLE全功能可用；#34/#37/#50/#51仍open，本PR需按精确HEAD审核。
任务包步数不修改全局验收：原P0～P7约80%，main固定台账19/169不变。
未修改全局看板/名册/权重或其他成员分支，不声称Pages更新。

下一包D-22需要成员A已审安全会话proof接口及真实Unix来源绑定；D-23继续同一Wi-Fi事务，
还须证明撤销与保存线性化；D-24账户/许可先审，D-25远程媒体和D-26综合矩阵分别推进。
缺少上述依赖时实际生产路径保持拒绝，不安装测试Mock作为安全后端。

审查应先确认feature/默认拒绝、显式迁移/回退边界和恢复记录不是普通scope，
再核对管理派发/真实IPC、数据损坏/失败证据以及浏览器非导出与CAS。
当前生产库为6且未启用，本批回退代码无需数据恢复；显式测试库7不可删表伪降级。
任何设备迁移都需先解决版本/回退契约，并另行安排负责人授权窗口。

来源：本仓库现有Auth/Store/Core/wire、PR #80接口及冻结任务书；浏览器使用标准WebCrypto/IndexedDB，
不自创密码协议、不修改A密码库或B UI、C更新器。新增代码沿用AGPL-3.0-only、无新依赖/锁文件变化。
OpenAI Codex辅助编写与实测；未虚报工时、重写RedamancyGTW或他人Git作者。
没有生产数据/设备/NAS访问、真实短信/拨号/转发/公网业务、Root安装、签名或部署。
远程CI状态在PR精确HEAD回读后另记，不能用本地733项替代。
