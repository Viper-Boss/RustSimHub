# 成员D：D23身份私有域执行请求指纹

2026-10-31（Asia/Shanghai），Viper-Boss/RustSimHub，分支codex/gtw-recovery-private-fingerprint-20261031，目标main。起始本人组合提交`33594198e5add9e2e0c8dc579500b4fdc133489e`（含#131/#135），main基线`80db24e9b101dd3fb4b8c951aa7f1e0e1858e65d`。实现及完整正式受测HEAD **`bf9d95dcc9353a6a6a0f7561f52d48da276b7e1b`**，受测时tracked_dirty=false；其后仅本人说明/标准库向量参考脚本/报告。

[编码、能力门禁及边界](../ble/recovery-execution-fingerprint.md)。已合并#103的Owner流程批准不等于技术验收。本包仅实现可选Auth私有准备对象、Identity隔离HMAC域及固定v2向量、Storage格式门禁。没有reserve/claim/Lookup/业务Wi-Fi或A标准安全会话；不凭指纹重发请求。Core未来必须从同一已认证正文验证target语义，本包仅检查target形状并承诺其值。

新增12项实际Auth执行测试+1项Identity向量测试；真实Identity、Ed25519 Owner签名登录、当前grant、Store7→9及Backup API，外部A verifier仅cfg(test)合成适配器。正文仍在原Zeroizing分配，准备前后完成备份的库字节相同、不含合成口令；重连/完整Auth+Store重开稳定、跨Auth/改正文/过期/撤权/替换verifier拒绝均实际验证。Python独立标准库参考核对4组原始transcript与HMAC，包括NUL正文。没有新增ignore、租约延长或生产错误映射修改。

```sh
source /workspace/toolchains/activate-rustsimhub.sh
cargo test --workspace --all-features --locked --offline
cargo clippy --workspace --all-targets --all-features --locked --offline -- -D warnings
cargo test -p rustsimhub-auth -p rustsimhub-storage --no-default-features --locked --offline
cargo test -p rustsimhub-auth --features recovery-grants --locked --offline
python3 docs/ble/recovery-execution-fingerprint-vectors.py
cargo fmt --all -- --check
git diff --check
```

实际整仓 **999通过/0失败/6继承忽略**，严格整仓Clippy/fmt/diff通过；默认Auth/Storage87通过，旧grant-only Auth70通过；参考脚本输出key_matches=true和transcripts_and_fingerprints_match=4。普通cargo test不包含显式manual kernel UID目标，本批未冒称重新运行它们；上次3359419实际Core/GATT跨UID记录见#114。

| 本地日志 | SHA256 |
|---|---|
| workspace.log | 6cdcd930fc899f94afbe2fec8a700d214e45d9d6f9fb5e2ad09375490bd8b99f |
| clippy.log | 3d2729a8e4ed54cc017f6b9ba69ef0f32cb0ac21cdafc2ee154e028ce3915a74 |
| default.log | 77d242cf2864c08ed3ea77b89c399909290cdb0d47e1a4d56b7c7f4fcc3dc46e |
| grants.log | 0fcca2bb30e64c39e8e0a1ce6e479481158f1fc3367c4942fd3b94f3a7ec7359 |

新feature仅增已有锁定uuid依赖的一条Auth直接边，无新依赖版本。其余依赖/源码来自本人已交栈，原作者/许可证保留。范围仅Auth/Storage独立恢复接点/最小Cargo及本人文档，无全局进度/其他成员/Root、生产UID/服务、默认启动、发行9兼容或设备部署修改。没有读取生产库、真实口令、SIM/网络/电话/短信或蜂窝。AGPL-3.0-only，Codex辅助，实际工时未知；CI按最终HEAD独立回读。

完整阶段新增0；仍 **21/26离线包已提交、剩D22–D26五步，固定P0–P7约80%**。下一包是同SQLite writer下持久reserve/一次claim/只读Lookup及Auth原子存活/单调fence，再接原唯一Wi-Fi actor；A、Root/发布兼容及设备最终验收保留。
