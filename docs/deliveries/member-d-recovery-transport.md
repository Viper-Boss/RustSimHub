# D-22局部交付：Core有界Unix运输实现

2026-10-30，按用户再次“继续”推进#88中的可独立运输部分。开始时#88尚待审核，
随后实际回读已合并并正常接续main；未将合并视为A密码协议或安装/账户授权。

- 仓库/分支：Viper-Boss/RustSimHub / `codex/gtw-recovery-transport-20261030`，PR到main。
- 原始前置：`1e8bb695ebd0252826148d3776c6f92216886b7b`（#88完整HEAD）。
- 已合入main：`81a822e3f916dd1324e2cac24145d3f426159807`。
- 实现HEAD：`9f7d7399298b3aa5ca1fd1e8630d687dc2878e30`。
- 最终代码/夹具HEAD：`bfdeda6410d4ecea5939884e28b5a902b29ec115`。
- 当前PR完整40位HEAD见PR正文；报告提交仅补证据。
- [实现、API与未完成边界](../ble/recovery-transport-implementation.md)。

## 实际实现

同一Core包新增库目标，显式`recovery-transport` feature才在Linux导出；无默认运行调用。
生产二进制未增加CLI/服务/监听器/安装路径或Wi-Fi派发，无Auth/Store/wire/schema更改。
不增加workspace成员/第三方依赖，Cargo.lock保持。

运输帧按#88候选：4字节BE长度、版本/kind两字节、最大2048字节opaque应用信封。
长度/版本/kind/Close零payload在分配正文前校验；Packet用Zeroizing且不Debug/Serialize。
完整读/写使用固定5秒绝对期限，开始和返回时复核，不靠分段重置期限。
取消已poll future、截断/EOF、坏帧、超时或写失败会永久fence，禁止重同步/自动重发；
未poll的future无副作用，Close是终态。写错误不冒充“未执行”，业务Unknown留后续Core账本。

角色检查要求当前非Root Core与Edge/forwarder/network/BLE互不混用，拒绝0和uid_t(-1)。
SourceGuard实际核UnixStream内核peer后才分配连接上下文，生成非零随机nonce；
每个Guard一个持有中的PeerConnection，失效但保留的连接不提前释放占用，描述符先关闭再释放槽。
PeerContext私有字段/无Clone或Deserialize，但仅是运输来源，不可当作Owner/grant/会话能力。
FramedIo正例只验证编解码，没有伪造UID或手造可信上下文来冒充成功准入。

没有A安全端口实现/默认Mock、私有请求能力、GATT接线、网络锁/保存Gate或请求账本。
Root/NSS/实际不同UID合法准入、socket组继承、FD传递及实际准入后的nonce/槽生命周期未运行；
构造Roles不表示账户存在或许可已安装。没有完整D-22/合法BLE恢复证明。

## 验证与精确范围

环境Linux x86_64/普通UID1000/Rust1.98，真实临时UnixStream::pair，合成字节。
只用本地锁定缓存，不访问D-Bus/设备/生产数据或公网。

已执行且通过的主要命令（均退出0）：

```sh
source /workspace/toolchains/activate-rustsimhub.sh
cargo test --workspace --all-features --locked --offline
cargo clippy --workspace --all-targets --all-features --locked --offline -- -D warnings
cargo test -p rustsimhub-core --lib --features recovery-transport --locked --offline
cargo clippy -p rustsimhub-core --all-targets --all-features --locked --offline -- -D warnings
cargo run -p rustsimhub-core --no-default-features --locked --offline -- --build-info
cargo check -p rustsimhub-core --lib --no-default-features --locked --offline
cargo fmt --all -- --check
git diff --check
```

初始整仓760通过/0失败/4项原有忽略；正常合入最新main后再次整仓：
**764通过、0失败、6项继承忽略**，包括本批14个新运输测试及#88 Core二进制174项。
增加的忽略来自主线C14子进程worker/私有tmpfs夹具；本批未新增ignore。
其他保留忽略是C06子进程worker、forwarder私有tmpfs满盘和updater两个Root/capability用例。
源码级整仓严格Clippy通过，既有proc-macro-error2 future-incompat提示如实保留。

14项新测试覆盖最大/空/连续帧、逐段拆包、每个截断点、无body坏长度/版本/kind/Close，
绝对截止及已就绪过期，未poll/已poll取消、真实发送缓冲填满后的取消、写失败/终态、
角色别名/Root/sentinel与真实内核peer拒绝。全部0失败/忽略。
整仓后仅收紧发送夹具：先实际writable再填到WouldBlock，并增加真实受阻write的计时超时；
14项库测试与Core严格Clippy在最终夹具字节再次通过，生产源码未变。
初次13项、收紧为14项以及最终检查均通过，没有放宽拒绝断言。

默认build-info仍database_schema=6、development_fixtures=false；默认库feature关闭也编译通过。
未运行ARM/Native、Root不同UID、A互操作、真实BlueZ/Wi-Fi/设备或部署检查。

原日志在work/recovery-transport，未提交生成物。主要摘要（只为比对本机证据，不替代远程CI）：

```text
integrated-workspace-tests.log a52188273636f2a393337cd8e642a46372388f59c0dcdfd7162c97087235a33e
integrated-clippy.log          24fa527fb300b91c15bef68b742a44f2abb152f976db54247d37c77e429b9be1
library-final.log              049e38b9ea474c8b297be176aa91bb6ecbdf2369c21a27153a0ad0af819744c1
core-clippy-final.log          26f11980550e0149a413341a86ae34b665ab770cf13a604217388430a09ed271
default-build-info.log         48c3def5555f7ee2cb3f5b8a0b80d5f81e1bcda85e5f61c1600d4ad4e3bb852f
```

## 数量与后续

本批是D-22运输层离线实现，**新增完整完成步骤0**；累计仍21/26离线包已提交，
剩D-22～26共5步。具体实现/14测试可以审核，不把部分实现或真实编解码成功算完整认证会话。
原P0～P7约80%、main台账19/169保持，未修改全局看板/名册/其他成员文件或分支。

下一步仍需要获审A安全会话、同一Auth/grant/连接绑定、实际GATT宿主与Root角色验证；
D-23唯一Wi-Fi事务/持久Unknown、D-24服务许可、D-25媒体和D-26综合矩阵分别保留。
缺A时不把运输PeerContext升级为能力，不用Mock或另一个Owner/业务库填补。

源码AGPL-3.0-only，复用既有Tokio/rustix/getrandom/zeroize，无新依赖/密码协议。
OpenAI Codex辅助实现和实测，无工时估造或作者重写；没有生产数据、真实短信/电话、
外网业务、Root账户修改、签名或部署。回退本feature/库无需设备或数据恢复。
PR最终HEAD和GitHubCI以回读为准，不以本地通过替代CI/负责人审核。
