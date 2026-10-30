# D-23前置交付：持久回执兼容接口与真实失败样例

2026-10-30。按用户自主持续推进要求，#101提交后从最新main建立独立小包。
不是D-23执行实现，不迁移/部署；待接口审核后接实现，其他独立D包继续推进。

- 仓库/分支：Viper-Boss/RustSimHub / `codex/gtw-recovery-execution-contract-20261030`，PR到main。
- 起始main：`6b5de5f3ca5ff706adacd8f25ed7f638546739a9`。
- 提案/夹具和整仓受测HEAD：`c5c9737fef11ca25e1caf0c0996dc6c4005ded77`。
- 当前最终完整40位HEAD见PR正文，后续只补本人报告。
- [完整候选接口](../ble/recovery-execution-proposal.md) · [不进入生产的SQL候选](../ble/recovery-execution-schema9.sql)。

## 实际范围

仅Store recovery_grants测试模块及本人proposal/SQL/报告；既有测试新增cfg(unix)子模块声明，生产代码未变。
没有版本/迁移入口/feature/API/Root或Wi-Fi派发变化，不改其他成员文件或全局台账。
从旧#63读取实际ledger源码，不复制其旧schema7/8。旧ledger SQL逐字夹具仅cfg(test)引用：

```text
source ad8b87a812317018994bcb5f6df4d00ce3a1842f:crates/storage/src/recovery-ledger.sql
blob   8de2ba8111c13865b1241411154a0a2a69c098a1
sha256 f27762490288549feff40ddc09050f050835f9d7c6851adf032bf59935aa0ac1
```

候选独立7→9、meta/format2、固定授权字段及私有域HMAC指纹；只在真实SQLite内存副本
执行SQL验证约束，**没有实现HMAC、迁移/claim/执行或认证**，候选格式尚待审。
混合8样例为“当前7+旧账本DDL”，非原旧#63库，不伪改字段把它称兼容。

## 验证

Linux普通UID1000/Rust1.98、锁定离线缓存、合成数据库/身份；新增4个真实Store/SQLite样例。
原7项run_trial生产函数测试运行通过，外部backend/Gate按已有夹具模拟，不造第二状态机。
默认仍schema6，显式grant7；不支持8恢复失败后保留源和隔离目标，不能称目标激活/恢复完成。

```sh
source /workspace/toolchains/activate-rustsimhub.sh
cargo test -p rustsimhub-storage --features recovery-grants --locked --offline execution_contract
cargo test -p rustsimhub-core --bin rustsimhub-core --all-features --locked --offline network_trial::provision::tests
cargo test --workspace --all-features --locked --offline
cargo clippy --workspace --all-targets --all-features --locked --offline -- -D warnings
cargo fmt --all -- --check
git diff --check
```

上述命令全部退出0：新增4项通过、原Wi-Fi 7项通过；整仓 **778通过、0失败、6项继承忽略**，
整仓严格Clippy/fmt/diff通过，保留既有proc-macro-error2 future-incompat提示。
本分支来自main，未包含待审#97/#101，因此总数对应main的774+本批4，而非另一分支798。
没有新ignore；六项来自C06/forwarder/Root/C14。运行日志仅本地work/recovery-execution-contract：

```text
workspace.log        9cbf772dd3d9c5de32fc8c2971c0156a87f94b556c3ec7cb23584b35f3425a05
workspace-clippy.log e0b0638720145f2206633f5fbd25e698d22ccda12873a174f58cd9a8a4d343fc
contract-final.log   1f083a54f7abd82d58a77d4f1a4b0f7ff656021a008af6df13136ea9d82f7e79
wifi.log             31bbfef69eb85fcd7f060543dddbc90f41a1fabf9ed22adb314019f1154661e1
```

SQL候选外键实际开启，在真实SQLite副本中检查；源7和grant DDL均保持，未启动迁移服务。
未运行Root/A互操作/GATT/跨UID合法接线、NM/设备/真正Wi-Fi/ARM/Native、更新回退9兼容。
没有生产秘密/设备/NAS或业务网络、短信/拨号/流量、Root安装、签名或部署。

## 数量和后续

新增完整步骤0；仍 **21/26离线包已提交，剩D-22～26共5步**。这是D-23第一PR接口/失败
样例，不将SQL候选或测试数计为D-23完成。固定P0–P7约80%、main19/169保持。
后续：接口/指纹/claim线性化点获审后实现显式9库；再接现有唯一Wi-Fi actor和真实授权。
当前A会话/UID/GATT、#97/#101审核及发行/回退兼容仍需后续，继续其他可独立D包。

沿用AGPL-3.0-only；旧SQL保留来源，使用现有SQLite/代码，无新增依赖/密码原语。
OpenAI Codex辅助核对/编写/实测，无作者改写或工时虚报。原日志仅work本地。
远程CI按最终HEAD回读；不自合并、不以本地通过/合并替代接口或设备验收。
