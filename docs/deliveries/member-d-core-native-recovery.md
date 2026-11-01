# 成员D：恢复命令接原Core Wi-Fi actor

2026-11-01（Asia/Shanghai）。分支codex/gtw-core-native-recovery-20261101，目标Viper-Boss/RustSimHub main。起始#140最终`c54e7b3e2d136deab6378961969bc493a1062632`，正常合并main `ce9ccb8debb61db49860fe795c1f3e471b749f4c`，不改写来源作者。首个实现`29d907ef55bc55aee5d77ef081b4c92048ef1e29`；修正记账与原deadline后，最终完整受测代码HEAD **`efe79b14b06e0a98b71a4c3b3b7eb83bb42d0d21`**，tracked_dirty=false；之后仅本人说明/报告。仍需本人待审栈及新增接口的逐HEAD审核，不等于设备/发布验收。

[具体接线、兼容变化、USB及关闭边界](../ble/core-native-recovery.md)。实际将合法私有Core正文/原trial/独立控制能力接入原`run_trial`，正常和恢复共用同一份ACTIVE/TRIALS/coordination、原NM backend及有限checkpoint。旧模块由库编译、二进制直接导入，没有复制状态机或全局锁；为跨crate调用扩大原Rust函数可见性，不新增远程普通命令。Normal保留原完整Principal/指纹及文件Audit；Recovery不造Principal。

Begin在原网络/试连/电话排他检查之后才登记领取，控制在同原会话/target/Ready/期限及原单格队列检查之后才消耗Claim。原actor在保存前后核独立Confirm，进度/同事务saving/终态走#139。后端失败只在实际补偿返回成功时rolled_back，未知保持Unknown。普通USB仍必需；只有原backend持有获核私有Core trial的Recovery分支可以省略USB fence，其他NM/MM owner、电话、wlan0/profile/WWAN/APN边界保持。

新增 **14项恢复actor+1项普通USB+1项Storage deadline测试，共16项**。已有13项普通网络/配网测试和原coordination测试随同一实现迁到库中，断言保持、不重复算新增。外部A verifier和NM backend仅cfg(test)合成；测试使用真实原actor/锁/Auth/Identity/SQLite和正文接点，不冒称A标准密码、系统NM/蓝牙、真实无USB配网或硬件清理验收。

覆盖独立确认/取消、保存前后撤权/断连、部分持久化及补偿失败、Unknown拒绝新副作用、普通与恢复身份互拒、原电话/试连忙不登记、未Ready/控制槽位满不领取、丢回复后actor/锁继续持有直到实际完成、缩短绝对期限、可信shutdown、错误普通控制证明及终态不再由Session保留凭据。Weak引用测试只证明排空路径的引用释放，不是全部内存副本擦除/进程崩溃验收。

新接线初稿发现并修正的真实失败：在原actorReady后，由真实其他线程持有Auth mutex，backend仍实际回滚；原记账也尝试取Auth锁，测试得到Unknown而不是已验证RolledBack。新增断言实际FAILED，日志保留。修正为原私有trial携带原Store记账接点，无需Auth mutex；进度/saving的writer fence另取原trial绝对deadline最小值，不能借长会话越过窗口。修正后同一回归通过；没有删除断言、延长期限或把客户端布尔当事实。

```sh
source /workspace/toolchains/activate-rustsimhub.sh
CARGO_INCREMENTAL=0 cargo test -p rustsimhub-core --lib --all-features network_trial --locked --offline
CARGO_INCREMENTAL=0 cargo test -p rustsimhub-storage --all-features recovery_execution::ledger::observations --locked --offline
CARGO_INCREMENTAL=0 cargo test --workspace --all-features --locked --offline
CARGO_INCREMENTAL=0 cargo clippy --workspace --all-targets --all-features --locked --offline -- -D warnings
CARGO_INCREMENTAL=0 cargo test -p rustsimhub-core --no-default-features --locked --offline
CARGO_INCREMENTAL=0 cargo test -p rustsimhub-auth --all-features --locked --offline
CARGO_INCREMENTAL=0 cargo check -p rustsimhub-core --lib --features recovery-grants,recovery-transport --locked --offline
cargo fmt --all -- --check
git diff --check
```

最终整仓 **1210通过/0失败/6继承忽略**，整仓严格Clippy/fmt/diff通过。默认Core **173/0/0**（原默认172加普通USB拒绝一项；库/二进制分布变化不重复计数），Auth全feature **112/0/0**，旧grant+transport库配置编译通过。专项原actor及既有网络测试28通过、Storage观察11通过；这些包含在整仓数内，不能重复相加。没有新增ignore。

| 本地证据 | SHA256 |
|---|---|
| workspace.log | 84faaa70e6c4e0121cbaf3e6df2a7fbe461475c220deffc9e77494d0e2e4ca77 |
| clippy.log | 44ed84f3133b98d64d7a944921816afd8e9445248a3c7bf650ea664c26eef468 |
| default.log | eb82a790e59127fd0386aaa0e01730692537b8120907962d557f5ed6b2d2c653 |
| auth.log | ada5994760e55bfb94d63d7ba051a3735af077220b742184f3ad123098a069f4 |
| old-library.log | 4cf86278c1e6d659943a46a3d7e3da429ac15be90f0f17742e24ef85228e7d3e |
| native-auth-busy-negative.log（FAILED） | be3969a133af6d773d1b7c180df30d9f69dc162fc32dbfd4b8b3f25d0b7bac37 |
| native-auth-busy-positive.log | 97242cf0bbe5859fbe0e2727c4ce4364424ad7f0de8797848e4f483195add3a2 |

范围为Core独立恢复/唯一网络协调接线、Auth原trial记账和最小Storage writer fence及本人文档。无新依赖/DDL/版本，默认Store仍6，正常AuthCommand/身份/权限、其他成员/全局台账、Root/实际UID/系统安装、发行9兼容和部署均未改。无生产库/真实凭据、设备/系统D-Bus/真实Wi-Fi、电话/短信/蜂窝操作。AGPL-3.0-only，保留复用代码来源作者，Codex辅助，实耗未知。本批未冒称重跑manual kernel UID试验。

这次已实际接原执行器，但仍不是完整可安装的BLE恢复：实际A安全会话/host/真实UID整装、T5 wire/前端映射、Root与发行schema9兼容、私有NM/硬件故障和设备联验仍缺。只有内部Rust RecoveryReply，不冒充T5 authoritative响应或设备保存证明。完整阶段新增0，仍 **21/26离线阶段已提交、剩D22–26五步，固定P0–P7约80%**。CI按最终HEAD独立回读。
