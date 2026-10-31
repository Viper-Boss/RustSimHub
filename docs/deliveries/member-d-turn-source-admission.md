# 成员D：D25固定TURN源码准入证据

2026-10-31（Asia/Shanghai）。仓库Viper-Boss/RustSimHub，分支codex/gtw-turn-source-admission-20261031，目标main。基线main `80db24e9b101dd3fb4b8c951aa7f1e0e1858e65d`。

先仅对旧Viper-Boss commit `31ec213840566c8715b741bcb318943169f8196a` 的docs/remote/relay-media-proposal.md使用file-scoped git format-patch/git am，保留原Author/日期/正文，产生 `477c24d6d51c471857e54aaf0131f1dcccf1ccfd`；没有引入其全局进度、HANDOFF/任务书或旧交付数据。本人来源清单/准入复核/最小摘录复现受测完整HEAD **`b4ae2660976736b1710d5931a0b7e05e9a881651`**，tracked_dirty=false，后续仅本报告。

[API及准入复核](../remote/turn-source-admission.md)、[14份源文件Git blob/SHA256清单](../remote/turn-source-admission.json)、[固定源码摘录复现器](../remote/turn-debug-excerpt-repro.py)。候选d8d127ab1e37aef14f95a9f6b529073c31ee04a5，固定文件全部核对实际Git对象，版本/锁文件/许可说明区别整个workspace与未解析的生产features闭包。原锁209包不冒充生产209依赖。

真实执行：14份固定源文件Git blob/SHA256/字节核对；原凭据类型derive/struct精确摘录只用std实际编译执行，断言Debug包含合成密码（输出仅布尔）；修改源文件一字节被拒绝且未进入编译；JSON读取/diff检查通过。没有下载、安装或构建TURN/stun/rustls依赖，没有跑上游测试、真实TLS/TURN、ARM或资源测试，不借用其他分支967结果。

```sh
source /workspace/toolchains/activate-rustsimhub.sh
python3 -B docs/remote/turn-debug-excerpt-repro.py --types-source work/turn-review/turn-types__src__lib.rs
git diff --check
```

| 证据 | SHA256 |
|---|---|
| 来源/状态清单 | d92296e2698609f56a3b5ae69070fa31239f706c879da88441a61887c97a12ab |
| 摘录执行结果 | 50348e0f21a11c5be351dd1d0475e18e16981edb5af63533653066932f0b9446 |
| 受测HEAD/14源校验/正反例结果JSON | de6b77512b74ef6d71d9294d0d1c8988f4b9d717b3c1da327e7cb810064fe72c |

核对了真实TLS/provider接点、完整性验证委托、allocation/permission/channel/事件、I/O、39.5s/60s期限和delete排队语义；证据缺口是实际删除确认、有限资源/背压/分片、正式provider/生产依赖图/许可/漏洞及与Core Permit/str0m集成。Debug摘录只证明原类型语义，不宣称现产品泄露或完整候选有已复现网络漏洞；现产品尚不依赖该库。整个候选源码和第三方许可正文只本地work，未复制进库。

无产品Rust/Cargo/Cargo.lock、服务/Root/UID/权限/全局台账/其他分工变化，无提供商/服务器/设备/NAS/生产数据、真实电话/短信/蜂窝、签名部署。AGPL-3.0-only/原提案作者保留，候选MIT OR Apache-2.0来源/许可已链接，Codex辅助、无工时虚报。CI按最终HEAD回读，账户启动前失败不计通过。

完整步骤新增0；仍 **21/26离线包已提交、剩5步，固定P0–P7约80%**。D25依赖/协议仍待具体准入；本包提供审查事实及最小失败样例，不自行批准。继续D23已合并批准接口的显式存储实现；A实际安全会话、Root角色/设备及TURN联通仍各自保留。
