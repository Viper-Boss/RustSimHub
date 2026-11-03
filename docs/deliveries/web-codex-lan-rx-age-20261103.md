# WEB2-LAN · 解码后接收帧龄

2026-11-03（Asia/Shanghai）。按`codex/web-mainline-batch2-20261103`分支任务书（blob `94ae3d877fdd1b1de247f4dee32338ef274b4a84`）执行主线辅助包，不新增成员或转移已有业务包。真实作者wnfhsj88，Codex辅助；AGPL-3.0-only，无新增依赖。

起始最新main：`8f9cfeff1a4b5a804b8b138e890f7386caf5a006`。使用独立干净工作树，未切换负责人的checkout。产品/测试最终受测HEAD：**`b0e4c23f8be65a85128df004e22c080325260830`**；随后仅追加本报告，最终40位PR HEAD在PR正文回读登记，不把文档提交冒称重新运行产品。分支`codex/web-lan-receive-age-20261103`，交Viper-Boss审核，不自行合并。

## 实际复现与修复

1. `Session::drain_to`的LAN/可选TURN共用`Event::MediaData`路径只在Opus解码前核对`network_time`，解码后直接更改incoming/received_frames/last_media_receive。`receive()`后续丢旧帧无法撤销这些变化；`service`/relay_tick使用实际Instant的1秒媒体空闲门禁。发送侧PR192的发送前/编码后检查保持，未修改其实现。
2. 增加私有`drain_to_with_clock`，生产固定调用`Instant::now`，无CLI/配置/HTTP测试开关。测试真实UDP完成DTLS指纹验证、实际SRTP解密与Opus解码，仅在解码边界控制本地单调时钟。RTC pacer和重排timer用真实RTC输入驱动；不靠sleep或忙调度碰60ms。握手只做有界yield，不操作ALSA/电话。
3. 初版夹具HEAD `acf251970cedc2fd843c2c14ece3ad0e45b07475`忘记驱动发送pacer，0过/8败、`real sender emitted an encrypted datagram`；这是夹具失败，不能作为产品负对照。日志保留，随后补齐真实pacer。
4. 有效负对照HEAD **`48a92c3d32950f8e18401bb4e8e865387e5169f5`**：`cargo test --manifest-path native/audio-alsa/Cargo.toml --offline --locked --lib decrypted_opus -- --nocapture`退出101，59ms有效帧通过，解码后60ms失败，实际断言为`post-decode stale audio entered queue`。真实decode后的第二次clock已发生，未用复制算法假状态机证明。完整receive_age_tests退出101，**3过/5败**，另含队列被挤掉、存活被续、缺许可和时钟回退。
5. 修复`ea1477118daa2f16e51abb1d8d7d596d5c719103`：解码后重新取可信时间，先重新核对实际Permit，随后检查相对network_time的年龄及时间回退。在队列/成功计数/存活时间更改前，将过期样本清零、准确增加一次drop并继续。没有重写音频原时间戳。已连接且codec存在但Session许可缺失时失败关闭，不能被旧帧分支隐藏；未授权/未创建codec的原握手行为保持。
6. 上述8项修复后全部通过：59ms、恰好60ms、解码中跨60ms、后续有效帧、满队列保护、原媒体空闲期限、解码边界撤权、缺许可、未来时间/回退。随后在最终受测HEAD新增真实认证SRTP中10ms Opus包反例：明确保持Codec错误，不计旧帧drop或成功接收。接收测试共9项，未修改Codec、worker或其他产品文件。

## 实际验证

Debian x86_64，EUID1000，Rust1.98.0，ALSA1.2.14、Opus1.5.2。每个shell加载既有工具链，`CARGO_INCREMENTAL=0 CARGO_PROFILE_DEV_DEBUG=0 CARGO_PROFILE_TEST_DEBUG=0 CARGO_BUILD_JOBS=2 CARGO_TARGET_DIR=/tmp/rustsimhub-web2-native-target`；使用已有锁及官方缓存，未升级依赖/锁。overlay初始40MiB可用，因此新工作树/新target置独立内存临时目录，没有清理他人缓存。原始日志仅本工作树`.local/web2-lan/`。

| 命令 | 结果 |
| --- | --- |
| `cargo test --manifest-path native/audio-alsa/Cargo.toml --offline --locked` | 退出0；**48过/0败/0忽略，3个suite** |
| 同命令加`--features turn-relay` | 退出0；**56过/0败/0忽略，3个suite**；与默认有重叠，不相加为独立测试总数 |
| `cargo clippy --manifest-path native/audio-alsa/Cargo.toml --locked --offline --all-targets -- -D warnings` | 退出0 |
| 同Clippy命令加`--features turn-relay` | 退出0；当前该manifest唯一可选特性，覆盖其全部特性 |
| `rustfmt --edition 2021 --config skip_children=true --check native/audio-alsa/src/rtc.rs native/audio-alsa/src/rtc_receive_age_tests.rs`、`git diff --check` | 退出0；只格式化本包两文件 |

完整默认/可选测试包含现有真实UDP/DTLS/SRTP/Opus回环、指纹替换拒绝、错误授权、撤权、1秒媒体空闲关闭及发送侧过期回归；没有用Mock声卡声称听音。旧main39/47不是本轮数字。首次8项通过版本随后因增加错误Opus时长反例而再次执行两种完整原生组及Clippy，最终只采用上述受测HEAD结果。

## 未运行与交接

未执行`test=false`私有Coturn/远程worker驱动（不在本包授权）、ALSA硬件、真实通话/听音、ARM构建、设备部署、签名或生产启用。没有扩大MAX_AGE_MS=60、4帧队列、媒体空闲期限、租约、指纹或默认开关；没有修改codec.rs/worker.rs/rtc_lan.rs/远程业务/Core/Auth/Storage。云端CI按最终PR HEAD单独回读，未用本机结果冒称云端通过。

回退仅本包rtc接收守卫/私有时钟接入和新增测试；回退也会恢复已复现的旧帧入队/续存活问题。负责人审核后决定合并；本执行者提交本包后继续WEB2-RUN，再做固定来源WEB2-COMBO。整体仍**81.5%**、20/169、加权10.6%，本报告不修改看板或增加验收点。
