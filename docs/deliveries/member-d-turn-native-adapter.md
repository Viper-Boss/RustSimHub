# 成员D：D25实际TURN/TLS与强制中继RTC适配

2026-11-01开始。用户已批准#56/#132离线实施；分支codex/gtw-turn-tls-driver-20261101，目标main。承接#162最终`9bc7be8099840f9e5e41e176ff5bc6ee1afc8eb2`，本人源码提交 `f029d2f46767e2f4c6fb9fb6193c731767bca7e4`，正常合并main `172d6659491c20674485ec800b3061cc8090bb9e`，完整正式受测源码 **`dac5f9f7970394e78c95f178533c06823ea7333c`**，tracked_dirty=false。之后仅本人说明和经实际构建验证的ARM夹具Dockerfile，无生产Rust/lock改变；最终推送HEAD见PR。

[实现/接口/边界](../remote/turn-native-adapter.md)、[实际依赖及许可闭包](../remote/turn-native-dependencies.json)、[复跑入口](../../native/audio-alsa/tests/fixtures/README.turn.md)。新Native显式turn-relay SDK实际分配/权限/channel/续租/单peer；正常CA/名字/期限的TokioRustls固定TCP；有界分帧、实际fd取消关闭、credential期限、原session保持、无透明重连；同实际allocation IP限制防止把Root登记服务当任意公网代理。实际selected pair与DTLS证书一致后才给传输事实，声音仍使用原Codec/Permit；原RTC引擎提取有限出口复用，默认LAN和v1不扩大。worker.rs仅7行rustfmt，无新worker业务入口。

正式x86_64验证：**原默认Native34（lib23+bin11）/0/0**；显式feature **37（lib26+bin11）/0/0**，新增标准测试3项。Root media28通过；Native整目标与显式manual target严格Clippy、整仓严格Clippy、Native/整仓fmt及diff通过。未在本Native包重跑整仓Rust测试，不借用#162或其他HEAD结果。

显式真实私有Coturn/TLS/RTC目标 **23/0/0**：其中17项新integration函数，另6项实际源码内unit与标准suite重复（3原RTC、3新unit），不重复计数。没有新增ignore。实际Allocate/401/438/permission/channel/refresh、奇数长度双向UDP转发；坏CA/名字/过期证书、错口令、partial/超长/坏头、原fd取消/EOF/credential/协议过期、早到peer、拒绝其他公网IP；实际ICE selected pair/DTLS/Opus双向解码、未有Core Permit拒绝及原Authority撤销全部通过。Core授权primitive是真实模块，可信Context和外部信令是合成；不是完整正常Core daemon/call/身份/Worker v2接线证明。RTC/Codec/Driver为实际生产源码编译，未写另一个密码或业务状态机。

同一干净DAC5源码经tracked git archive交叉构建 **AArch64 release**，actualELF Machine=AArch64、7,748,392字节；GCC14/Cortex-A53、Rust1.98、真实ARM ALSA/Opus FFI依赖均链接。该ARM测试程序在QEMU `-cpu cortex-a53 -L /` 下同一 **23/0/0** 全矩阵通过（另先单独运行强制relay/DTLS/Opus/撤权1项通过，不加到23）。x86和QEMU均实际UID1000/caps空、read-only、network=none、128M/12tasks/2CPU；这些是合成无硬件组件在该容器限额下通过，不等于整板ALSA负载/完整systemd生产预算验收。QEMU中的Rust客户端是ARM ELF，独立Coturn和证书生成器为夹具宿主程序。

原协议时间/票据/PCM门禁没有放宽。私有server正常长期凭据/证书，loopback允许、测试CA/钥匙、私有SQLite userdb、max allocation600、stale nonce1只在夹具。测试only协议时钟小步推进覆盖服务器真实refresh与nonce交互；TLS、服务nonce、凭据、Core/Frame时间均真实且独立。首次续期夹具250秒大步跨过了原channel刷新窗口，正确拒绝；改30秒步长并保留专门601秒过期拒绝用例，不当库漏洞/放松过期。另一初版fixture将负days证书生成成起止倒置，及默认serverDB不可写已修正；均不算产品漏洞。

真实候选修前失败/修后保持断言：

- 误用sans-io Instant::from_std(Instant::now())每次几乎零，使认证后请求不发、实际5秒Deadline。改为保存单连接origin，5秒不延长。失败日志`private-tls-db.log` SHA e5027db3ab72be531df0f1dc9e6a71b96ac16dfa972229de74432a04b68af1c6；修前源码快照SHA 2f18cc6ee10c9e1810ae16bea529f84856ffc8e49dddc9b2a04408195ee300d7。
- 实际同peer早到UDP在准备channel中触发Peer失败；改为等待真实ack前丢弃该数据，不授予ready、不发布RTC。失败`early-negative.log` SHA f37c2d8ff3c8224dd23979c50e6c9fbeb0a86a916c00a1199f2710a4832a6b29。
- 原候选仅检查public IP，实际允许创建8.8.8.8:40000 permission，断言得到Ok而非Peer。未向该地址发数据、容器无出口；修为同实际allocation IP先拒绝。失败`peer-policy-negative.log` SHA 744c67d8bb816f1e975b3af01a9f77034360a8caaa10a3d8993cc82c0d6efb01。

这些是本次新离线候选缺陷，不称旧生产漏洞；未部署、未由生产调用方消费。失败路径实际关旧fd/不重试业务；delete只尽力请求且始终无已验证释放声明。

```sh
source /workspace/toolchains/activate-rustsimhub.sh
CARGO_INCREMENTAL=0 cargo test --manifest-path native/audio-alsa/Cargo.toml --locked --offline
CARGO_INCREMENTAL=0 cargo test --manifest-path native/audio-alsa/Cargo.toml --features turn-relay --locked --offline
CARGO_INCREMENTAL=0 cargo clippy --manifest-path native/audio-alsa/Cargo.toml --all-targets --features turn-relay --locked --offline -- -D warnings
CARGO_INCREMENTAL=0 cargo clippy --manifest-path native/audio-alsa/Cargo.toml --test turn_private_tls --features turn-relay --locked --offline -- -D warnings
CARGO_INCREMENTAL=0 python3 native/audio-alsa/tests/run_private_turn.py --build-fixture
CARGO_INCREMENTAL=0 cargo test -p rustsimhub-media --locked --offline
CARGO_INCREMENTAL=0 cargo clippy --workspace --all-targets --all-features --locked --offline -- -D warnings
cargo fmt --manifest-path native/audio-alsa/Cargo.toml -- --check
cargo fmt --all -- --check
git diff --check
```

ARM交叉命令在network-none只读构建容器中执行：`cargo test --manifest-path /src/native/audio-alsa/Cargo.toml --test turn_private_tls --features turn-relay --target aarch64-unknown-linux-gnu --release --no-run --locked --offline --message-format=json`。Source为DAC5的git archive，CARGO_INCREMENTAL=0、jobs2，CC/AR/linker为aarch64-linux-gnu工具，CFLAGS=-mcpu=cortex-a53，ARM pkg-config目录为/usr/lib/aarch64-linux-gnu/pkgconfig、SYSROOT=/；Rustup及公开Cargo registry/git缓存只读，独立target可写，没有挂载凭据文件。4G/128tasks是编译容器上限，**不冒充**128M/12运行上限。运行则使用相同read-only/UID/caps/network/128M/12配置，命令 `qemu-aarch64 -cpu cortex-a53 -L / /input/turn_private_tls --nocapture --test-threads=1`。完整本地编译/包版本/ELF和运行证据在work/turn-arm-output。

| 正式证据 | SHA256 |
|---|---|
| 默认Native | 30e3cf1b4614b207b49edf08e5386e52c03d1ffd9f4e7306e22304a359a10a6d |
| feature Native | 3168a3b7da3e2167afac72cf642986feb756795e06ee74032ea2b25f96d1eab8 |
| Root media | 28bdd5b6ce6bde33bc2507ebbe14c315eaa7f054ea0c9846f7a6674319f0ca15 |
| x86私有23项日志 | 888fec8cd9e180d37e87f84e680df8f0159a2b27b75a2c6aedbc739e0d63c29e |
| ARM实际ELF | 60adfcc7f63c705e3a20e0586dfe9073597ffa77ff20421de8699d58a7515050 |
| ARM全部23项日志 | 8686dd59c8bb352d3f39a5cc5b1d3e791ae72a4788e81480dd954ab81962f33f |
| ARM编译日志 | 646eb965d6289582f628a48e25164ecf52a398b20b93562714fa674a61d0c95b |
| 实际依赖/许可/OSV快照 | 570f9e272fc28ddf281fc0195de39794cfb3391ca6e27d6be84469326a8d1758 |

Native固定fixture image fc854d016cf274c1d3cb211d7763bc77755683849fa4546df79f38f5d9817c53；ARM image167dce679c9a29712c08138a32c4bd470963c388aac9aa18fc44c3e51d0e7389。正式source archive SHA b6a926163b5a8aa747461149031e12e87b82c30bf68ba791d06bb806f3662f6e。完整正式命令均退出0；首次调试dirty结果单独留存，不作最终clean结果。

依赖：lock163→189，新增26包，仅既有native Tokio1.53.2→1.53.3安全补丁替换；其他旧版本均保留（同名多版本不误报删除）。145 registry活跃版本实际查OSV后只余原DTLS链crypto-bigint0.5.5的RUSTSEC-2026-0387；tokio RUSTSEC-2026-0386已在本native lock修正。旧Root workspace lock不由本包升级。**生产准入false**：旧p256/p384/dimpl依赖^0.5，不能直接换>=0.7.6当兼容修复；不私改或重写密码库，也不借TLS ring掩盖DTLS已有风险。149项实际许可声明完整、固定Git原作者/许可保留，正式分发仍保留全部NOTICE；Git精确提交未索引不称审计通过。原AGPL、Matthew Waters MIT/Apache来源及其余作者保留，Codex辅助，实耗未知。

无生产TURN/提供商/设备/系统BlueZ/NM/真实PCM/电话/短信/普通蜂窝、Root账户/NSS/权限安装、签名或部署。开发stdlib与C交叉包仅开发空间/临时容器；不会启动宿主服务或更改生产角色。没有改Core/wire身份/旧scope、其他成员源文件或全局台账。与main正常合并导入的他人成果不算本人新增。A实际安全会话仍只有他方评估材料，不接管。

本包为D25/D26实际SDK与输入故障交付，完整D25还需正常Core/Worker v2/Root登记装配及依赖风险收口，不从剩余表重复扣除。仍 **22/26离线阶段已提交、剩4（D22/D23/D25/D26），固定P0–P7 80%**；后续继续本人可独立装配/真实IPC与撤权验证，硬件/Root/A生产provider保持主线边界。远端CI按最终HEAD回读，既有workflow只验证其实际运行范围，不说它运行了此显式manual目标。
