# D-23前置交付：持久回执兼容接口与真实失败样例

2026-10-30。按用户自主持续推进要求，#101提交后从最新main建立独立小包。
不是D-23执行实现，不迁移/部署；待接口审核后接实现，其他独立D包继续推进。

- 仓库/分支：Viper-Boss/RustSimHub / `codex/gtw-recovery-execution-contract-20261030`，PR到main。
- 起始main：`6b5de5f3ca5ff706adacd8f25ed7f638546739a9`。
- 实现/受测及最终完整40位HEAD提交后写入PR正文。
- [完整候选接口](../ble/recovery-execution-proposal.md) · [不进入生产的SQL候选](../ble/recovery-execution-schema9.sql)。

## 实际范围

仅Store recovery_grants测试模块及本人proposal/SQL/报告。生产代码只有cfg(test)模块声明，
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

最终整仓结果/原日志摘要结束后补充，不把执行中或仅提案写成实现。
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
