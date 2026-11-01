# 成员D：实际恢复通道的应用主机与回复围栏

2026-11-01（Asia/Shanghai）。分支 `codex/gtw-core-recovery-application-host-20261101`，目标Viper-Boss/RustSimHub main。起始#145最终 `5a4a2f716bae63ea365e0acc66ceb8e62d3828a9`、main基线 `ce9ccb8debb61db49860fe795c1f3e471b749f4c`。首稿 `6bb5240ba78ff51787467a728d0f7d5b811b7938`，夹具失败清理 `66fdd1db347f3d3fd5f2ce100093b836a5ec063a`；修正真实EOF及有限阶段后的最终完整受测代码 **`294aae889bc5f599c19703acaf3b13250edec97c`**，tracked_dirty=false；之后仅本人说明/报告。最终已推送HEAD列在PR正文，不沿用旧HEAD审核、不自行合并。

[主机顺序、取消与A接口](../ble/core-recovery-application-host.md)。实际accepted RecoveryChannel自己读帧，经原Auth prepare/专用worker/finish，原请求派发原Core应用；原回复Scope及一次SealingRequest交显式A封装接点，原worker保留实际socket/slot至排空，同Scope关联和前后门禁通过才写回。没有新默认verifier/sealer、明文fallback、假Principal或第二Wi-Fi引擎。仅是A握手后的应用接点，**A真实握手/密码/密钥实现仍未提供**，不能称完整BLE互操作。

新增 **5项常规Auth测试**，整仓 **1234通过/0失败/6继承忽略**，整仓严格Clippy/fmt/diff通过；默认Core **173/0/0**、旧grant+transport库编译通过。手动target另做显式严格Clippy（test=false不会由普通suite覆盖），未新增ignore。

另运行**1个显式真实UID应用试验、16个执行场景**，以及旧来源试验**1个、13场景**；它们不加进1234或完整阶段数。两份手动结果均是代码294aae889bc5f599c19703acaf3b13250edec97c、tracked_dirty=false。实际内核Core1001/BLE1002及root/Core/Edge/forwarder/network/foreign六种拒绝来源；源目录2750/socket0660及SO_PEERCRED来自真实进程，不制造PeerContext。应用场景包含缺sealer且verifier0调用、原Core获核query/有限错误、排队worker取消/5秒截止、A封装worker取消/5秒截止、真实EOF且有额外未读帧、纯关闭、回复前实际撤权。Core原Store9/Identity/Auth/解码/T5实际运行，外部A verifier及密文输出仅cfg(test)合成；未跑实际NM/无线试连，不把常量假密文字节称AEAD。

Root只在network=none/read-only临时Docker夹具内设置数字UID/GID，无NSS/账户/主机权限修改。保持原cap-drop=ALL后仅SETUID/SETGID/CHOWN/FOWNER、32pids/256M/2CPU/tmpfs128M，未新增CAP_KILL。固定Python镜像 `python@sha256:2d44738ebf4ce5f63b9d9ea871e9c7cfdc0426f7b0c97cac915fd88ded1d86bb`。控制器缺CAP_KILL的失败清理改为先切换至该已登记子进程同UID再发信号；失败路径也复跑，无PermissionError遮蔽原断言。

真实失败对照：从首版代码仅移除调用方guard，真实排队worker试验FAILED“waiter cancellation must immediately fence queued ownership”；恢复guard通过。再在未加EOF监视器的版本仅加真实peer关闭断言，A工作挂起且有未读排队帧时实际FAILED“actual peer EOF must fence before the adapter deadline”；添加真实FD关闭监视器后原2秒断言通过。未放宽租约、队列、超时或拒绝断言。这两条是本次新主机候选的本地问题，不称旧生产漏洞。

```sh
source /workspace/toolchains/activate-rustsimhub.sh
CARGO_INCREMENTAL=0 cargo test --workspace --all-features --locked --offline
CARGO_INCREMENTAL=0 cargo clippy --workspace --all-targets --all-features --locked --offline -- -D warnings
CARGO_INCREMENTAL=0 cargo clippy -p rustsimhub-core --test recovery_application_uid --features recovery-transport,recovery-execution --locked --offline -- -D warnings
python3 crates/core/tests/run_recovery_kernel_uids.py --application
python3 crates/core/tests/run_recovery_kernel_uids.py
CARGO_INCREMENTAL=0 cargo test -p rustsimhub-core --no-default-features --locked --offline
CARGO_INCREMENTAL=0 cargo check -p rustsimhub-core --lib --features recovery-grants,recovery-transport --locked --offline
cargo fmt --all -- --check
git diff --check
```

最终命令全部退出0。普通workspace不自动Docker/变UID，手动编排始终显式调用。首次debug手动日志tracked_dirty=true，**不**用作最终受测HEAD证据，以下摘要只取最终独立复跑及清楚标注的失败记录。

| 本地证据（work/recovery-application-host） | SHA256 |
|---|---|
| workspace.log | 7bd7c828a9c3dcca44c17971359ab1a406dd22c8ee9b5e619dcc7e6a8af7c8c9 |
| clippy.log | ea66ad86832af440a3a889f502b283a99c5661458b79ab9b2c2db6a994502e7b |
| manual-clippy.log | 23ecf9f9a5f59e26a8130df11befc27e8c1f56ec1fb83649c3809067c5a513f2 |
| manual-application.log | b8aecd25bf3b8fbfeb454051d01eb224bc23e5a5a72cae5ad4098d6588a32a67 |
| manual-original.log | b19b039d12dd1358bc98b37c9dd3bb1486baf2456a924f338bfec217c53a5f9c |
| default.log | 972600ade11fcd2d74c9d7df05abc5b24a474a826461d61409640b2398c07edf |
| old-library.log | 80a9b10336c0a27f164e453872a3ae1fc59fbaee85200700878b6db724486ed4 |
| guard-negative-cleanup.log（FAILED） | 58d6219e05950faed87a6cf35fa04bc9f8306e21c7cfc317b7b1d8720e465a70 |
| eof-negative.log（FAILED） | 55399000386ebe254e16dc0ec297728f79a769f0cb4e797d6e1967f44b96df7f |
| 真实应用container.log（work/recovery-kernel-uids/application） | 98d07f52c5708b43c01e5c78da9b7f2e16dd82ce1552caa85cd38d51c65dfa02 |
| 原来源container.log（work/recovery-kernel-uids） | 2f0141868b8f7495735ed1d638d8309575ea57cac32c3681b0637ff874b63a3e |

无新依赖/DDL/默认启动/正常权限或协议改变，旧私有帧格式及正常业务保持。API新增只读Scope/失效guard/关闭观察器，不能转换执行权限；Auth内部cap字段保持私有。原3秒查询/40秒trial/45秒checkpoint和普通8秒控制不延长，主机应用等待5秒不等同取消或成功。无系统D-Bus/真实设备、生产库/凭据、Root安装/UID保留、电话/短信/普通蜂窝、签名部署。AGPL-3.0-only/来源作者保留，Codex辅助，实耗未知。

用户已批准#53/#56/#132的**离线**实施，下一包继续D24服务及D25媒体，不再索取相同批准。A标准会话/主线启动与真实Root/NSS/发行9兼容、ARM/cgroup/HCI/RF和设备/两端听音仍主线；不接管其他成员密码库，不修改全局台账。完整阶段仍 **21/26已提交、剩D22–26五步，固定P0–P7约80%**，以原完整验收条件计，不能靠拆包/测试数提高。远端CI按最终HEAD单独回读。
