# 成员D：D23显式schema9存储准入交付

2026-10-31（Asia/Shanghai）。仓库Viper-Boss/RustSimHub，分支codex/gtw-recovery-execution-schema9-20261031，目标main。基线main `80db24e9b101dd3fb4b8c951aa7f1e0e1858e65d`，含已合并#103。按Owner已有APPROVED及合并记录进入实现；记录明确未逐行技术审核/独立测试，不把流程批准当技术/实机验收，不再虚报尚待首次批准。

实现起始 `d8430fe26ea1ce13228d57fa2a20e88343d76231`；修正目录大小写拒绝及最终正式受测完整HEAD **`8e2c1dea82db2bab1e7c4bd23841131324204b10`**，tracked_dirty=false；之后仅本人说明/报告。默认6和旧显式7仍拒绝9，新feature只提供显式7→9备份迁移/open9/restore9、严格目录/行/关系/容量准入和Unknown恢复，不提供请求写入/claim/指纹密码或Core-Wi-Fi派发。

[接口/迁移/失败矩阵及未实现项](../ble/recovery-execution-schema9-implementation.md)。19项新增真实SQLite/actor/Backup测试通过；全featureStorage81通过；最终独立分支整仓 **865通过/0失败/6继承忽略**，严格整仓Clippy/fmt/diff通过；默认Auth/Storage **87通过/0失败/0忽略**，仅旧grant feature的Storage **62通过/0失败/0忽略**。未引入新ignore。该分支未包含新BLE待审栈，不借用/混加其967计数。

```sh
source /workspace/toolchains/activate-rustsimhub.sh
cargo test --workspace --all-features --locked --offline
cargo clippy --workspace --all-targets --all-features --locked --offline -- -D warnings
cargo test -p rustsimhub-auth -p rustsimhub-storage --no-default-features --locked --offline
cargo test -p rustsimhub-storage --features recovery-grants --locked --offline
cargo fmt --all -- --check
git diff --check
```

migration的专用Source模式在备份前不运行任何普通recover/line重置；公开备份失败源库字节保持；同writer下真实SQLiteFull使DDL完整回滚7并保留可恢复7快照；恢复9拒绝旧入口、坏目标保留.restore-pending、源备份不变。跨grant/trial、规范UUID/pin/代次/终态证据/容量正反例、混合8/目录别名、非终态Unknown和终态保持均实际调用真实模块。初稿Recovery目录GLOB大小写变体实际失败后修正，原失败日志保留；无删断言、假SQL状态机或生产fault开关。

| 本地证据 | SHA256 |
|---|---|
| workspace.log | b90684d43181ebe30c32a85f7c574f4e78e3fe0203fcb8bb9ed204787551f0d3 |
| clippy.log | d0d3e959ef65d1e7e3308a5f95a426e9e019903e67f753f3450dbd00a5838b1a |
| default.log | 5fa28e87faa1f984403c96b9f1489663584199202db3fd132082446b6b3529a3 |
| grants7.log | 8a7504e6c052bde8d417ca5146ccf2ea24aa0a75575899a3773849c4dec95c89 |
| case-negative.log | 25b43d3f460a2a4f06b0443af4a55d4c5b6406f3985967306eac4bab9298a596 |
| case-positive.log | db24af9b4d8494cfa156691fd8a12af1efc5db748f7ab9cbad6a79bc33310f20 |
| result.json | f53436e0c47ff3f9b0458146d7c1356341e08a794dfe6b483e18e6691c1be8ed |

DDL逐字来自#103候选SQL（仅执行版注释不同），本包不选择HMAC私有域/derive新key或声称指纹已实现；格式描述不等于能力。未来reserve/claim必须继续使用Auth私有能力/单调fence和原唯一Wi-Fi actor，不能把Unknown回执/SQL active布尔或假Principal当授权。

范围仅Storage独立Recovery功能/最小Cargo feature及本人文档，Cargo.lock/依赖版本不变；无其他成员、全局台账、Root/UID/服务/发行兼容修改，无生产数据/设备/NAS、NM/网络/Wi-Fi/电话/短信/蜂窝、签名部署。既有生产和发行回退组仍只声明6，不能安装这份9实验库。AGPL-3.0-only/来源保留、Codex辅助、无工时虚报。CI按最终HEAD回读，启动前账单失败不计通过。

完整步骤新增0；仍 **21/26离线包已提交、剩5步，固定P0–P7约80%**。D23格式准入本包落地，尚无实际幂等账本写入/一次claim/安全指纹/业务派发，A和设备/Root/发布验收不代替。下一包先合并本人已交组件验证相容性，再继续已批准的私有指纹及领取接口范围。
