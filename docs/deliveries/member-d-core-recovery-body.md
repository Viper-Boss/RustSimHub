# 成员D：Core同认证正文参数绑定

2026-10-31（Asia/Shanghai），分支codex/gtw-core-recovery-body-20261031，目标Viper-Boss/RustSimHub main。起始#139最终`2f29a919ab72aa4d425e3821f9bc78164bbf8039`，包含已正常合并的main `342adaa2fee6f3ea36a82f0145dc7931e12e36de`及本人待审依赖。代码和完整正式受测HEAD **`48f2a7eb37d9ec320e0e100b617837605125e0b8`**，tracked_dirty=false；之后仅本人说明/报告。

[应用正文候选v1、私有类型及边界](../ble/core-recovery-body.md)。实际新增Core可选库模块：解析前核真实能力，动作从私有cap取得；SSID/口令、控制trial和查询UUID均从这份确切已认证正文解析，不接受额外可替换参数。封闭结构拒绝未知/重复/伪造role/token/动作字段，复用原正常Wi-Fi校验，解析字符串Zeroizing、无Debug/Serialize。Core Write→Pending→Claim→原trial/control保留同一参数；Existing无票据，Lookup不能额外指定另一个目标。

这是Core本地应用正文候选API `decode_v1`，不宣称A的加密wire已冻结，不选择ECDH/HKDF/AEAD或改A库。没有监听/生产host注入/普通AuthCommand/原Wi-Fi actor调用或第二网络事务。主机后续须按共同审核的应用正文接口交付Auth已核body/动作/UUID；新feature默认关闭。

新增 **14项真实Core库/Auth/Identity/签名Owner/Store9测试**全部通过；外部A verifier仅cfg(test)合成。覆盖准确正文接力、独立控制、查询目标和只读快照字节保持、字段/JSON/UTF-8/类型/截断/正常Wi-Fi规则、正文篡改、前后断连/撤权/跨Auth、旧Reserved重连与原始空白/转义指纹规则、动作混淆、旧7拒绝、重开Unknown、完成备份无明文SSID/口令。没有新增ignore、放宽期限、假Principal或假Core业务状态机。

最终整仓 **1194通过/0失败/6继承忽略**，整仓严格Clippy/fmt/diff通过；默认Core **172/0/0**，原grant+transport库配置编译通过。相较#139的1180仅本包14项增量；主线其他模块的测试不算本人新增。

首次整仓尝试在编译Core bin test时因执行环境ENOSPC中断，未执行测试，未计通过；保留原日志。确认无其他Cargo进程后，仅清理本人工作树`target/debug/incremental`可重建缓存（约22GB），源码/日志/数据保持。随后在相同代码HEAD以命令级`CARGO_INCREMENTAL=0`完整重跑成功；没有改变项目默认构建配置或测试断言。

```sh
source /workspace/toolchains/activate-rustsimhub.sh
cargo test -p rustsimhub-core --lib --features recovery-execution recovery_body --locked --offline
CARGO_INCREMENTAL=0 cargo test --workspace --all-features --locked --offline
CARGO_INCREMENTAL=0 cargo clippy --workspace --all-targets --all-features --locked --offline -- -D warnings
CARGO_INCREMENTAL=0 cargo test -p rustsimhub-core --no-default-features --locked --offline
CARGO_INCREMENTAL=0 cargo check -p rustsimhub-core --lib --features recovery-grants,recovery-transport --locked --offline
cargo fmt --all -- --check
git diff --check
```

| 本地日志 | SHA256 |
|---|---|
| workspace.log | ff4226c4c15bfb148d1e28c75da294d79ec0e259c84253156351369c1b04c9ae |
| clippy.log | 6522fb9f0d4319bd37f5e4ea2a09d7ae5abb42c5fab564740a0ad691d04f4925 |
| core-default.log | d41b00bf3401a610079881860b6bdbcbf19022dc0240ec9e47cc00f32bc52f5d |
| core-old.log | 2512526220eafb594e60cfa4edd8a45027da33f3749fdaa3e1d01bce4422ebea |
| workspace-disk-full.log（中断，不通过） | 7a9e690fdfc917f57c847ac7f2273e41117621e99c61e26885432c51717a10f1 |

最小Cargo feature只增加已锁定uuid的一条Core直接依赖边，无新版本；未修改其他成员或全局台账、Storage DDL/默认入口、普通Core业务/USB/NM/WWAN/IMS/APN、Root/实际UID/服务、发行9兼容或部署。无生产库/真实凭据、设备/系统D-Bus/Wi-Fi、电话/短信/蜂窝或网络业务操作。AGPL-3.0-only、来源作者保留、Codex辅助，实耗未知。普通suite不包含manual kernel UID目标，本批未冒称重跑。

还须让原唯一Wi-Fi actor消费这里的私有trial/control，接Normal/Recovery所有权、保存前后门禁及#139真实观察，再接已批准来源的USB恢复分支/T5映射。A实际安全会话、host/Root/UID/发行兼容和设备验收保留。完整阶段新增0，仍 **21/26已提交、剩D22–26五步，固定P0–P7约80%**。CI按最终HEAD单独回读。
