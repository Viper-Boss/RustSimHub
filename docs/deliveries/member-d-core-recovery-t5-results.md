# 成员D：原恢复执行结果接 T5

2026-11-01（Asia/Shanghai）。分支 `codex/gtw-core-recovery-t5-results-20261101`，目标 `Viper-Boss/RustSimHub` main。起始本人 #141 最终 `b513f4c9130ff2a21d9638546127fb0b340e3e07`，main 基线 `ce9ccb8debb61db49860fe795c1f3e471b749f4c`。首个实现 `fb4bbcb4f5b796b36f20ea66e38352f28ec269ef`；修正 Saving 证据后，最终完整受测代码 **`daa21d653ec70caf1d6d5af216579a9d8ff48b90`**，tracked_dirty=false；之后仅本人接口说明/交付报告。最终推送 HEAD 在 PR 正文回读，待审依赖 #131/#139/#140/#141 等栈保留，不自合并、不沿用旧 HEAD 审核。

[应用结果接口与证据映射](../ble/core-recovery-t5-results.md)。新增显式 `recovery_v1`，真实获核请求/同正文解码接**同一个原 dispatcher/actor/Store**，返回冻结 OperationV1 / LookupV1。私有字段仅 Serialize，无不可信 JSON/RecoveryReply 到成功响应的公开转换；没有复制 Wi-Fi 状态机、伪造 Principal 或新业务库。

活跃回执须匹配原 actor 的完整绑定/状态、当前原 Auth 门禁和原绝对期限；缺 actor、普通 owner、过期、死连接、Reserved/Claimed/Rejected/Unknown 不升级 Ready。只读匹配函数比较 realm/设备/Owner/grant 代次与 trial，不签发权限。保存/精确补偿由原 actor 实际完成并成功记账后才有终态证据；Saving 保存证据为 null，Unknown 的证据和期限均为 null。not_found 不当未执行证明。Query 整体 3 秒截止，工作池晚到只读解码不启动后端；写调用、丢回复排空和原有限回滚不变。有限错误不含正文、SSID/口令或其他身份元数据。

**新增 11 项测试**，复用真实原 actor/全局锁/Auth/Identity/SQLite；仅外部 A verifier 与 NM backend 为 cfg(test) 合成。覆盖原开始/Ready/独立确认保存/取消、部分保存/补偿失败、孤立 Reserved/Claimed/Ready、原期限/普通 owner、不同库相同公开 UUID/trial 的完整绑定拒绝、Saving 时断连重连不接管、Unknown 不升级、实际 Store 重开后的 saved/Unknown 原 ID、真实单阻塞工作池下查询 3 秒截止、正文篡改/默认关闭及有限错误。

36 份实际 Rust 序列化结果（7 operation、18 lookup、11 error）直接通过未修改的 `configurator/src/contracts.js`；脚本另核 Saving/Unknown 证据。向量包含在以上 11 项测试中，不另计新增测试或主线已验收阶段。没有改前端校验规则，也没有把手写期望 JSON 冒充互操作。

初稿实际失败：在代码 fb4bbcb 上仅加入当前最终测试的两个额外证据断言，将原 actor 的外部 commit 挂起，真实 Saving 响应得到 Bool(false) 而不是 Null，Rust 用例 **FAILED、退出101**。修正仅将在途 Saving 的 settings_saved 保留 null；同一断言保留并通过，跨语言脚本也核 null。此为本次新映射初稿缺陷，不称旧生产漏洞。另有对初稿实际输出的独立 Node 断言失败记录，不混充完整 Rust 回归。

```sh
source /workspace/toolchains/activate-rustsimhub.sh
CARGO_INCREMENTAL=0 cargo test -p rustsimhub-core --lib --all-features t5_ --locked --offline -- --nocapture
node docs/ble/core-recovery-t5-contract-check.mjs work/core-recovery-t5/targeted.log
CARGO_INCREMENTAL=0 cargo test --workspace --all-features --locked --offline
CARGO_INCREMENTAL=0 cargo clippy --workspace --all-targets --all-features --locked --offline -- -D warnings
CARGO_INCREMENTAL=0 cargo test -p rustsimhub-core --no-default-features --locked --offline
CARGO_INCREMENTAL=0 cargo check -p rustsimhub-core --lib --features recovery-grants,recovery-transport --locked --offline
cargo fmt --all -- --check
git diff --check
```

以上最终命令均退出 0。**整仓 1221 通过/0失败/6继承忽略，整仓严格 Clippy、fmt、diff 通过；默认 Core 173/0/0，旧 grant+transport 库配置编译通过；专项11/0/0、跨语言36/0。** 没有新增 ignore。首次 fb4bbcb 整仓也通过，但最终报告及摘要只用修正版 daa21d6 的独立复跑，不用初稿的绿色格式检查覆盖语义失败。

| 本地证据（work/core-recovery-t5） | SHA256 |
|---|---|
| targeted.log | eaeb41ad4a6ce26a9c952cdd193fd50b57d0bc6c50121b473e473b9f4ff521e8 |
| js-contract.log | bbb51f66686d4f1de9fb8198dd07b9e12d3d095660e19a6411a0be20e79c5592 |
| workspace.log | a928a191eb71e09f0af192a3d560aafb03a1450792e374ce9e8ec860ce45d8d2 |
| clippy.log | 7a7a26e7c224d192c1b41994d8d0a47e04daeabb3e714254b0ae41aea536d8cc |
| default.log | f688c76dcca1f8615a405af0a8e9c5ec26fcf7d62ce95d5f216257743bfd2eca |
| old-library.log | 22e0df986057b9ba5be07526ac5fb348862a3dedef42108383cc3e088acbb889 |
| saving-native-negative.log（真实 Rust FAILED） | e41e244d3ad08b51c047211060efb0ed35cd230cd1265f6190ff80f0ad5d11db |

范围为本人 Core 恢复结果层、最小 Auth/Storage 只读比较及本人文档/验证脚本。没有新依赖、DDL、schema版本、正常命令/权限、其他成员文件或全局台账变化；默认 Store 仍6，普通 USB、电话协调、唯一网络锁、WWAN/IMS/APN不变。AGPL-3.0-only、来源作者保留、Codex辅助，实耗未知。没有生产库、真实凭据、设备/系统 D-Bus、NM/蓝牙、Root/UID安装、电话/短信/普通蜂窝、签名或部署；未重跑此前 manual kernel UID 实验。

这是供后续 A 安全主机接线的**应用结果层**。实际 A 会话/host/真实 UID 整装、ReadStatus/前端安全 provider、Root/发行 schema9 兼容、媒体依赖与 NM/硬件联验仍缺；crypto_wire_frozen=false，不选择 A 密码套件。关闭 transport 不声称取消或回滚成功。完整阶段新增0，仍 **21/26离线阶段已提交、剩D22–26五步，固定P0–P7约80%**。CI按最终推送HEAD独立回读，不以本地通过替代远端结果。
